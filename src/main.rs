mod api;
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

    // Startup: fetch devices and send summary
    info!("Fetching initial device list...");
    match client.get_all_devices().await {
        Ok(devices) => {
            let summary = income_monitor::build_income_summary(&devices);

            // Fetch line data for online devices
            let line_data_map = fetch_line_data(&client, &devices, &config.api.user_id).await;
            let line_summary = line_monitor::build_line_summary(&line_data_map);

            let msg =
                alert_monitor::format_startup_summary(&summary, Some(&line_summary));
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

    // Spawn slow loop (income + line)
    let slow_client = client.clone();
    let slow_notifier = notifier.clone();
    let slow_state = state.clone();
    let slow_config = config.clone();
    let slow_state_path = state_path.clone();

    let slow_handle = tokio::spawn(async move {
        let interval =
            std::time::Duration::from_secs(slow_config.monitor.income_check_interval_secs);
        info!(
            "Income/line monitor started (interval: {}s)",
            slow_config.monitor.income_check_interval_secs
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
                    let line_data_map = fetch_line_data(
                        &slow_client,
                        &devices,
                        &slow_config.api.user_id,
                    )
                    .await;

                    let line_events = line_monitor::check_line_changes(
                        &line_data_map,
                        &s,
                        slow_config.alert.notify_on_recovery,
                        slow_config.alert.line_loss_threshold,
                        slow_config.alert.line_rtt_threshold,
                    );

                    if !line_events.is_empty() {
                        let alerts: Vec<String> =
                            alert_monitor::format_line_alerts(&line_events)
                                .into_iter()
                                .map(|a| a.message)
                                .collect();
                        all_alerts.extend(alerts);
                    }

                    drop(s);

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
                        s.line_statuses.insert(
                            sn.clone(),
                            line_monitor::line_status_from_response(data),
                        );
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
    }

    // Save final state
    let s = state.lock().await;
    let _ = s.save(&state_path);
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
