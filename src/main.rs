mod api;
mod chart;
mod config;
mod monitor;
mod notify;
mod state;

use anyhow::Result;
use chrono::{Local, Timelike};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{error, info, warn};

use api::client::{ApiError, OnethingClient};
use chart::history::{ChartDataStore, LineSample};
use config::Config;
use monitor::{alert_monitor, device_monitor, income_monitor, line_monitor};
use notify::telegram::TelegramNotifier;
use state::MonitorState;

#[tokio::main]
async fn main() -> Result<()> {
    let config_path = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("config.toml"));

    let config = Config::load(&config_path)?;

    let default_filter = format!("onething_monitor={}", config.monitor.log_level);
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| default_filter.into()),
        )
        .init();

    info!("Loading config from {}", config_path.display());

    let client = OnethingClient::new(&config.api)?;
    let notifier = TelegramNotifier::new(&config.telegram);
    let state_path = MonitorState::state_path();
    let state = Arc::new(Mutex::new(MonitorState::load(&state_path)));

    // Chart data store (load from disk if available)
    let chart_data_path = ChartDataStore::chart_data_path();
    let chart_store = Arc::new(Mutex::new(ChartDataStore::load(
        &chart_data_path,
        config.monitor.chart_history_hours,
        config.monitor.income_check_interval_secs,
    )));

    // Startup: fetch devices and send summary
    info!("Fetching initial device list...");
    match client.get_all_devices().await {
        Ok(devices) => {
            let summary = income_monitor::build_income_summary(&devices);

            // Fetch line data for online devices
            let line_data_map = fetch_line_data(&client, &devices, &config.api.user_id).await;
            let line_summary = line_monitor::build_line_summary(&line_data_map);

            let msg = alert_monitor::format_startup_summary(&summary, Some(&line_summary));
            if let Err(e) = notifier.send_message(&msg).await {
                error!("Failed to send startup message: {}", e);
            }

            // Initialize state
            let mut s = state.lock().await;
            for d in &devices {
                s.device_statuses.insert(d.sn.clone(), d.device_status);
                s.device_incomes.insert(d.sn.clone(), d.y_income);
            }
            for (sn, (_remark, data)) in &line_data_map {
                s.line_statuses
                    .insert(sn.clone(), line_monitor::line_status_from_response(data));
            }
            s.first_run = false;
            let _ = s.save(&state_path);
        }
        Err(e) => {
            error!("Failed to fetch initial devices: {}", e);
            let msg = format!("\u{26a0}\u{fe0f} 网心云监控启动失败: {}", e);
            let _ = notifier.send_message(&msg).await;
            if matches!(e, ApiError::AuthExpired(_)) {
                error!("Auth expired on startup. Please update cookies in config.toml");
                return Ok(());
            }
        }
    }

    let config = Arc::new(config);

    // Spawn fast loop (device status)
    let fast_client = client.clone();
    let fast_notifier = notifier.clone();
    let fast_state = state.clone();
    let fast_config = config.clone();
    let fast_state_path = state_path.clone();

    let fast_handle = tokio::spawn(async move {
        let interval =
            std::time::Duration::from_secs(fast_config.monitor.device_check_interval_secs);
        info!(
            "Device status monitor started (interval: {}s)",
            fast_config.monitor.device_check_interval_secs
        );

        loop {
            tokio::time::sleep(interval).await;

            match fast_client.get_all_devices().await {
                Ok(devices) => {
                    let mut s = fast_state.lock().await;

                    let device_events = device_monitor::check_device_changes(
                        &devices,
                        &s,
                        fast_config.alert.notify_on_recovery,
                    );

                    if !device_events.is_empty() {
                        let alerts: Vec<String> =
                            alert_monitor::format_device_alerts(&device_events)
                                .into_iter()
                                .map(|a| a.message)
                                .collect();

                        info!("Device changes detected: {}", alerts.len());
                        if let Err(e) = fast_notifier.send_alerts(&alerts).await {
                            error!("Failed to send device alerts: {}", e);
                        }
                    }

                    // Update state
                    for d in &devices {
                        s.device_statuses.insert(d.sn.clone(), d.device_status);
                    }
                    let _ = s.save(&fast_state_path);
                }
                Err(ApiError::AuthExpired(msg)) => {
                    warn!("Auth expired: {}", msg);
                    let _ = fast_notifier
                        .send_message("\u{1f511} 登录已过期，请更新config.toml中的Cookie!")
                        .await;
                    tokio::time::sleep(std::time::Duration::from_secs(600)).await;
                }
                Err(e) => {
                    error!("Device check failed: {}", e);
                }
            }
        }
    });

    // Spawn slow loop (income + line + chart data collection)
    let slow_client = client.clone();
    let slow_notifier = notifier.clone();
    let slow_state = state.clone();
    let slow_config = config.clone();
    let slow_state_path = state_path.clone();
    let slow_chart_store = chart_store.clone();
    let slow_chart_data_path = chart_data_path.clone();

    let slow_handle = tokio::spawn(async move {
        let interval =
            std::time::Duration::from_secs(slow_config.monitor.income_check_interval_secs);
        let chart_enabled = slow_config.monitor.chart_interval_secs > 0;
        let chart_interval =
            std::time::Duration::from_secs(slow_config.monitor.chart_interval_secs.max(1));
        let mut last_chart_send = std::time::Instant::now();

        info!(
            "Income/line monitor started (interval: {}s, chart: {})",
            slow_config.monitor.income_check_interval_secs,
            if chart_enabled {
                format!("every {}s", slow_config.monitor.chart_interval_secs)
            } else {
                "disabled".to_string()
            }
        );

        loop {
            tokio::time::sleep(interval).await;
            let mut all_alerts = Vec::new();

            // Income + line check
            match slow_client.get_all_devices().await {
                Ok(devices) => {
                    let s = slow_state.lock().await;

                    let income_events = income_monitor::check_income_changes(
                        &devices,
                        &s,
                        slow_config.alert.income_drop_threshold,
                    );

                    if !income_events.is_empty() {
                        let alerts: Vec<String> =
                            alert_monitor::format_income_alerts(&income_events)
                                .into_iter()
                                .map(|a| a.message)
                                .collect();
                        all_alerts.extend(alerts);
                    }

                    // Line status check (online devices only)
                    let line_data_map =
                        fetch_line_data(&slow_client, &devices, &slow_config.api.user_id).await;

                    let line_events = line_monitor::check_line_changes(
                        &line_data_map,
                        &s,
                        slow_config.alert.notify_on_recovery,
                        slow_config.alert.line_loss_threshold,
                        slow_config.alert.line_rtt_threshold,
                    );

                    if !line_events.is_empty() {
                        let alerts: Vec<String> = alert_monitor::format_line_alerts(&line_events)
                            .into_iter()
                            .map(|a| a.message)
                            .collect();
                        all_alerts.extend(alerts);
                    }

                    drop(s);

                    // ── Collect chart data ──
                    if chart_enabled {
                        collect_chart_data(
                            &slow_client,
                            &devices,
                            &line_data_map,
                            &slow_chart_store,
                        )
                        .await;

                        // Persist chart data to disk
                        let store = slow_chart_store.lock().await;
                        if let Err(e) = store.save(&slow_chart_data_path) {
                            error!("Failed to save chart data: {}", e);
                        }
                    }

                    // ── Send charts on interval ──
                    if chart_enabled && last_chart_send.elapsed() >= chart_interval {
                        send_charts(&slow_chart_store, &slow_notifier).await;
                        last_chart_send = std::time::Instant::now();
                    }

                    // Daily report
                    let now = Local::now();
                    let today = now.format("%Y-%m-%d").to_string();
                    let hour = now.hour();

                    let mut s = slow_state.lock().await;
                    if hour >= slow_config.monitor.daily_report_hour
                        && s.last_daily_report_date != today
                    {
                        let summary = income_monitor::build_income_summary(&devices);
                        let report = alert_monitor::format_daily_report(&summary);
                        if let Err(e) = slow_notifier.send_message(&report).await {
                            error!("Failed to send daily report: {}", e);
                        } else {
                            // Also send charts with daily report
                            if chart_enabled {
                                send_charts(&slow_chart_store, &slow_notifier).await;
                                last_chart_send = std::time::Instant::now();
                            }
                            s.last_daily_report_date = today;
                        }
                    }

                    // Update income + line state
                    for d in &devices {
                        s.device_incomes.insert(d.sn.clone(), d.y_income);
                    }
                    // Clear line statuses for offline devices, update for online
                    s.line_statuses.retain(|sn, _| {
                        devices.iter().any(|d| d.sn == *sn && d.device_status == 1)
                    });
                    for (sn, (_remark, data)) in &line_data_map {
                        s.line_statuses
                            .insert(sn.clone(), line_monitor::line_status_from_response(data));
                    }
                    let _ = s.save(&slow_state_path);
                }
                Err(ApiError::AuthExpired(_)) => {}
                Err(e) => {
                    error!("Income check failed: {}", e);
                }
            }

            if !all_alerts.is_empty() {
                info!("Income/line alerts: {}", all_alerts.len());
                if let Err(e) = slow_notifier.send_alerts(&all_alerts).await {
                    error!("Failed to send alerts: {}", e);
                }
            }
        }
    });

    // Spawn bot command listener
    let bot_client = client.clone();
    let bot_notifier = notifier.clone();
    let bot_config = config.clone();
    let bot_chart_store = chart_store.clone();

    let bot_handle = tokio::spawn(async move {
        notify::bot::run_bot_polling(bot_client, bot_notifier, bot_config, bot_chart_store).await;
    });

    info!("Monitor running. Press Ctrl+C to stop.");

    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            info!("Shutting down...");
        }
        r = fast_handle => {
            error!("Fast loop exited: {:?}", r);
        }
        r = slow_handle => {
            error!("Slow loop exited: {:?}", r);
        }
        r = bot_handle => {
            error!("Bot loop exited: {:?}", r);
        }
    }

    // Save final state
    let s = state.lock().await;
    let _ = s.save(&state_path);
    let cs = chart_store.lock().await;
    let _ = cs.save(&chart_data_path);
    info!("State saved. Goodbye!");

    Ok(())
}

/// Fetch line data for all online devices.
/// Returns HashMap<SN, (remark, NetLineDataResponse)>.
async fn fetch_line_data(
    client: &OnethingClient,
    devices: &[api::types::DeviceInfo],
    user_id: &str,
) -> HashMap<String, (String, api::types::NetLineDataResponse)> {
    let mut map = HashMap::new();
    for d in devices {
        if d.device_status != 1 {
            continue;
        }
        match client.get_net_line_data(&d.sn, user_id).await {
            Ok(data) => {
                map.insert(d.sn.clone(), (d.device_remark.clone(), data));
            }
            Err(ApiError::AuthExpired(_)) => break,
            Err(e) => {
                warn!("Failed to fetch line data for {}: {}", d.sn, e);
            }
        }
    }
    map
}

/// Collect chart data from both cloud and local device APIs.
async fn collect_chart_data(
    client: &OnethingClient,
    devices: &[api::types::DeviceInfo],
    line_data_map: &HashMap<String, (String, api::types::NetLineDataResponse)>,
    chart_store: &Arc<Mutex<ChartDataStore>>,
) {
    let now = Local::now();

    for d in devices {
        if d.device_status != 1 {
            continue;
        }

        // Get cloud API data (loss, rtt) - already fetched
        let cloud_lines = line_data_map.get(&d.sn);

        // Get local device data (speed) - may fail
        let local_status = match client.get_local_line_status(&d.sn).await {
            Ok(status) => status,
            Err(e) => {
                warn!("Failed to get local line status for {}: {}", d.sn, e);
                None
            }
        };

        let mut store = chart_store.lock().await;

        // Build a combined view: match lines by NIC name or line number
        if let Some((_remark, cloud_data)) = cloud_lines {
            for cloud_line in &cloud_data.line_data_list {
                let line_key = format!("line{}", cloud_line.line_no);

                // Try to find matching local line data by tag (line_no), then NIC
                let tag = format!("line{}", cloud_line.line_no);
                let local_line = local_status.as_ref().and_then(|ls| {
                    ls.multidial
                        .iter()
                        .find(|ll| ll.tag == tag)
                        .or_else(|| ls.multidial.iter().find(|ll| ll.nic == cloud_line.nic))
                });

                let sample = LineSample {
                    timestamp: now,
                    upspeed_bytes: local_line.map(|l| l.upspeed),
                    downspeed_bytes: local_line.map(|l| l.downspeed),
                    lost: Some(cloud_line.lost),
                    rtt: Some(cloud_line.rtt),
                };

                store.push(&d.sn, &d.device_remark, &line_key, sample);
            }
        } else if let Some(local_data) = &local_status {
            // No cloud data, but have local data
            for ll in &local_data.multidial {
                let line_key = ll.tag.clone();

                let sample = LineSample {
                    timestamp: now,
                    upspeed_bytes: Some(ll.upspeed),
                    downspeed_bytes: Some(ll.downspeed),
                    lost: None,
                    rtt: None,
                };

                store.push(&d.sn, &d.device_remark, &line_key, sample);
            }
        }
    }
}

/// Render and send charts for all devices.
/// Collects all chart PNGs while holding the lock, then sends them.
async fn send_charts(chart_store: &Arc<Mutex<ChartDataStore>>, notifier: &TelegramNotifier) {
    let charts: Vec<(String, Vec<u8>)> = {
        let store = chart_store.lock().await;
        let sns = store.device_sns();
        let mut result = Vec::new();

        for sn in &sns {
            if !store.has_sufficient_data(sn, 2) {
                continue;
            }
            if let Some(history) = store.get_device(sn) {
                match chart::renderer::render_device_chart(sn, history) {
                    Ok(png_bytes) => {
                        let caption = format!("{} - {}", history.remark, sn);
                        result.push((caption, png_bytes));
                    }
                    Err(e) => {
                        error!("Failed to render chart for {}: {}", sn, e);
                    }
                }
            }
        }
        result
    };

    for (caption, png_bytes) in charts {
        if let Err(e) = notifier.send_photo(png_bytes, &caption).await {
            error!("Failed to send chart: {}", e);
        }
        // Small delay between photos to respect rate limits
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
}
