use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{error, info, warn};

use crate::api::client::{ApiError, OnethingClient};
use crate::chart;
use crate::chart::history::ChartDataStore;
use crate::config::Config;

use super::telegram::TelegramNotifier;

/// Run the Telegram bot command polling loop.
/// Listens for `/status` and `/chart` commands via getUpdates long-polling.
pub async fn run_bot_polling(
    client: OnethingClient,
    notifier: TelegramNotifier,
    config: Arc<Config>,
    chart_store: Arc<Mutex<ChartDataStore>>,
) {
    let mut offset: i64 = 0;
    let allowed_chat_id = notifier.chat_id().to_string();

    info!("Bot command listener started");

    loop {
        match notifier.get_updates(offset, 30).await {
            Ok(updates) => {
                for update in updates {
                    offset = update.update_id + 1;

                    let message = match &update.message {
                        Some(m) => m,
                        None => continue,
                    };

                    // Only respond to the configured chat_id
                    if message.chat.id.to_string() != allowed_chat_id {
                        continue;
                    }

                    let text = match &message.text {
                        Some(t) => t.as_str(),
                        None => continue,
                    };

                    // Extract command (strip @botname suffix if present)
                    let cmd = text.split_whitespace().next().unwrap_or("");
                    let cmd = cmd.split('@').next().unwrap_or(cmd);

                    match cmd {
                        "/status" => {
                            handle_status(&client, &notifier, &config).await;
                        }
                        "/chart" => {
                            handle_chart(&notifier, &chart_store).await;
                        }
                        _ => {}
                    }
                }
            }
            Err(e) => {
                warn!("Bot getUpdates failed: {}", e);
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            }
        }
    }
}

/// Handle `/status` - fetch and display real-time line status for all online devices.
async fn handle_status(
    client: &OnethingClient,
    notifier: &TelegramNotifier,
    config: &Config,
) {
    let devices = match client.get_all_devices().await {
        Ok(d) => d,
        Err(ApiError::AuthExpired(msg)) => {
            let _ = notifier
                .send_message(&format!("\u{1f511} 登录已过期: {}", msg))
                .await;
            return;
        }
        Err(e) => {
            let _ = notifier
                .send_message(&format!("\u{26a0}\u{fe0f} 获取设备列表失败: {}", e))
                .await;
            return;
        }
    };

    let online_devices: Vec<_> = devices.iter().filter(|d| d.device_status == 1).collect();

    if online_devices.is_empty() {
        let _ = notifier
            .send_message("\u{1f4ca} 当前无在线设备")
            .await;
        return;
    }

    let mut msg = String::from("\u{1f4ca} <b>实时线路状态</b>\n");

    for device in &online_devices {
        // Fetch cloud line data (loss, RTT)
        let cloud_data = match client
            .get_net_line_data(&device.sn, &config.api.user_id)
            .await
        {
            Ok(data) => Some(data),
            Err(e) => {
                warn!("Failed to fetch cloud line data for {}: {}", device.sn, e);
                None
            }
        };

        // Fetch local device data (speed)
        let local_data = match client.get_local_line_status(&device.sn).await {
            Ok(data) => data,
            Err(e) => {
                warn!("Failed to fetch local line data for {}: {}", device.sn, e);
                None
            }
        };

        let name = if device.device_remark.is_empty() {
            &device.sn
        } else {
            &device.device_remark
        };

        msg.push_str(&format!("\n\u{1f4e6} <b>{}</b>\n", name));

        if let Some(ref cloud) = cloud_data {
            let online = cloud.count - cloud.offline_num;
            msg.push_str(&format!(
                "线路: {}/{} 在线\n",
                online, cloud.count
            ));
        }

        // Build per-line status by merging cloud + local data
        if let Some(ref cloud) = cloud_data {
            for cl in &cloud.line_data_list {
                let status_icon = if cl.status == 1 {
                    "\u{1f7e2}"
                } else {
                    "\u{1f534}"
                };

                // Find matching local line for speed data (prefer tag match)
                let tag = format!("line{}", cl.line_no);
                let local_line = local_data.as_ref().and_then(|ld| {
                    ld.multidial
                        .iter()
                        .find(|ll| ll.tag == tag)
                        .or_else(|| ld.multidial.iter().find(|ll| ll.nic == cl.nic))
                });

                let speed_str = if let Some(ll) = local_line {
                    let up_mb = ll.upspeed as f64 / 1_000_000.0;
                    let down_mb = ll.downspeed as f64 / 1_000_000.0;
                    format!("\u{2191}{:.1} \u{2193}{:.1} MB/s", up_mb, down_mb)
                } else {
                    String::new()
                };

                let nic_display = if cl.nic.is_empty() {
                    format!("line{}", cl.line_no)
                } else {
                    cl.nic.clone()
                };

                msg.push_str(&format!(
                    "  {} {}  {}  {}  丢包:{:.1}% 时延:{:.0}ms\n",
                    status_icon, nic_display, cl.ipv4, speed_str, cl.lost, cl.rtt,
                ));
            }
        } else if let Some(ref local) = local_data {
            // Only local data available
            msg.push_str(&format!(
                "线路: {}/{}\n",
                local.connectedline, local.totalline
            ));
            for ll in &local.multidial {
                let status_icon = if ll.status == "connected" {
                    "\u{1f7e2}"
                } else {
                    "\u{1f534}"
                };
                let up_mb = ll.upspeed as f64 / 1_000_000.0;
                let down_mb = ll.downspeed as f64 / 1_000_000.0;
                let nic_display = if ll.nic.is_empty() {
                    &ll.tag
                } else {
                    &ll.nic
                };
                msg.push_str(&format!(
                    "  {} {}  {}  \u{2191}{:.1} \u{2193}{:.1} MB/s\n",
                    status_icon, nic_display, ll.ipaddr, up_mb, down_mb,
                ));
            }
        } else {
            msg.push_str("  (无法获取线路数据)\n");
        }
    }

    if let Err(e) = notifier.send_message(&msg).await {
        error!("Failed to send status response: {}", e);
    }
}

/// Handle `/chart` - render and send current charts immediately.
async fn handle_chart(
    notifier: &TelegramNotifier,
    chart_store: &Arc<Mutex<ChartDataStore>>,
) {
    let charts: Vec<(String, Vec<u8>)> = {
        let store = chart_store.lock().await;
        let sns = store.device_sns();
        let mut result = Vec::new();

        if sns.is_empty() {
            drop(store);
            let _ = notifier
                .send_message("\u{1f4c8} 暂无图表数据，请等待数据采集")
                .await;
            return;
        }

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

    if charts.is_empty() {
        let _ = notifier
            .send_message("\u{1f4c8} 数据点不足，请等待更多数据采集")
            .await;
        return;
    }

    for (caption, png_bytes) in charts {
        if let Err(e) = notifier.send_photo(png_bytes, &caption).await {
            error!("Failed to send chart: {}", e);
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
}
