use super::device_monitor::DeviceEvent;
use super::income_monitor::{IncomeEvent, IncomeEventKind, IncomeSummary};
use super::line_monitor::{LineEvent, LineEventKind, LineSummary};
#[derive(Debug)]
pub enum AlertLevel {
    Critical,
    Warning,
    Info,
}

#[derive(Debug)]
pub struct Alert {
    pub level: AlertLevel,
    pub message: String,
}

pub fn format_device_alerts(events: &[DeviceEvent]) -> Vec<Alert> {
    events
        .iter()
        .map(|e| {
            use super::device_monitor::DeviceEventKind::*;
            let level = match &e.kind {
                WentOffline | WentError { .. } | Disappeared => AlertLevel::Critical,
                WentOnline | RecoveredFromError => AlertLevel::Info,
                NewDevice { .. } => AlertLevel::Info,
            };
            Alert {
                level,
                message: e.description(),
            }
        })
        .collect()
}

pub fn format_income_alerts(events: &[IncomeEvent]) -> Vec<Alert> {
    events
        .iter()
        .map(|e| {
            let name = if e.remark.is_empty() {
                &e.sn
            } else {
                &e.remark
            };
            let (level, msg) = match &e.kind {
                IncomeEventKind::ZeroIncome => (
                    AlertLevel::Warning,
                    format!("\u{1f4b0} {} ({}) 昨日收益为0", name, e.device_type),
                ),
                IncomeEventKind::SignificantDrop {
                    previous,
                    current,
                    drop_ratio,
                } => (
                    AlertLevel::Warning,
                    format!(
                        "\u{1f4c9} {} ({}) 收益大幅下降: {:.1} -> {:.1} (下降{:.0}%)",
                        name,
                        e.device_type,
                        previous,
                        current,
                        drop_ratio * 100.0
                    ),
                ),
            };
            Alert {
                level,
                message: msg,
            }
        })
        .collect()
}

pub fn format_line_alerts(events: &[LineEvent]) -> Vec<Alert> {
    events
        .iter()
        .map(|e| {
            let name = if e.remark.is_empty() {
                &e.sn
            } else {
                &e.remark
            };
            let (level, mut msg) = match &e.kind {
                LineEventKind::OfflineIncreased { prev, current } => (
                    AlertLevel::Warning,
                    format!("\u{1f50c} {} 离线线路: {} \u{2192} {}", name, prev, current),
                ),
                LineEventKind::LostHighIncreased { prev, current } => (
                    AlertLevel::Warning,
                    format!(
                        "\u{1f4e1} {} 丢包过高线路: {} \u{2192} {}",
                        name, prev, current
                    ),
                ),
                LineEventKind::RttHighIncreased { prev, current } => (
                    AlertLevel::Warning,
                    format!(
                        "\u{23f1}\u{fe0f} {} 时延过高线路: {} \u{2192} {}",
                        name, prev, current
                    ),
                ),
                LineEventKind::Recovered { field_name, prev } => (
                    AlertLevel::Info,
                    format!("\u{2705} {} {}已全部恢复 (之前:{})", name, field_name, prev),
                ),
            };

            // Append problem line details (max 5)
            if !e.problem_lines.is_empty() {
                let show_count = e.problem_lines.len().min(5);
                for pl in &e.problem_lines[..show_count] {
                    let detail = match &e.kind {
                        LineEventKind::LostHighIncreased { .. } => {
                            format!("丢包:{:.1}%", pl.lost)
                        }
                        LineEventKind::RttHighIncreased { .. } => {
                            format!("时延:{:.0}ms", pl.rtt)
                        }
                        _ => super::line_monitor::line_status_text(pl.status).to_string(),
                    };
                    msg.push_str(&format!(
                        "\n  \u{b7} {} ({}) {} - {}",
                        pl.dial_account, pl.ipv4, pl.nic, detail
                    ));
                }
                if e.problem_lines.len() > 5 {
                    msg.push_str(&format!("\n  ... 还有{}条", e.problem_lines.len() - 5));
                }
            }

            Alert {
                level,
                message: msg,
            }
        })
        .collect()
}

pub fn format_daily_report(summary: &IncomeSummary) -> String {
    let mut msg = String::new();
    msg.push_str(&format!(
        "\u{1f4ca} <b>每日收益报告</b>\n\n\
         总收益: <b>{:.1}</b> 云豆\n\
         设备数: {} (在线:{} 离线:{} 异常:{})\n\
         0收益设备: {}\n\n",
        summary.total_income,
        summary.device_count,
        summary.online_count,
        summary.offline_count,
        summary.error_count,
        summary.zero_income_count,
    ));

    if !summary.per_device.is_empty() {
        msg.push_str("<b>设备明细:</b>\n<pre>\n");
        for d in &summary.per_device {
            let status_icon = match d.status {
                0 => "\u{26aa}",
                1 => "\u{1f7e2}",
                2 => "\u{1f534}",
                _ => "\u{2753}",
            };
            let name = if d.remark.is_empty() {
                &d.sn
            } else {
                &d.remark
            };
            // Truncate name to fit
            let display_name: String = name.chars().take(20).collect();
            msg.push_str(&format!(
                "{} {:<20} {:>8.1}\n",
                status_icon, display_name, d.income
            ));
        }
        msg.push_str("</pre>");
    }

    msg
}

pub fn format_startup_summary(
    summary: &IncomeSummary,
    line_summary: Option<&LineSummary>,
) -> String {
    let mut msg = format!(
        "\u{1f680} <b>网心云监控已启动</b>\n\n\
         监控设备: {} 台\n\
         在线: {} | 离线: {} | 异常: {}\n\
         昨日总收益: {:.1} 云豆",
        summary.device_count,
        summary.online_count,
        summary.offline_count,
        summary.error_count,
        summary.total_income,
    );

    if let Some(ls) = line_summary {
        msg.push_str(&format!(
            "\n\n\u{1f310} 线路: {} 条 (离线:{} 丢包高:{} 时延高:{})",
            ls.total_lines, ls.total_offline, ls.total_lost_high, ls.total_rtt_high,
        ));
    }

    msg
}
