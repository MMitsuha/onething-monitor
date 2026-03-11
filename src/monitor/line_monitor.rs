use std::collections::HashMap;

use crate::api::types::{NetLineDataResponse, NetLineInfo};
use crate::state::{LineStatus, MonitorState};

#[derive(Debug, Clone)]
pub struct ProblemLine {
    pub dial_account: String,
    pub ipv4: String,
    pub nic: String,
    pub status: u8,
    pub lost: f64,
    pub rtt: f64,
}

impl ProblemLine {
    fn from_line_info(info: &NetLineInfo) -> Self {
        Self {
            dial_account: info.dial_account.clone(),
            ipv4: info.ipv4.clone(),
            nic: info.nic.clone(),
            status: info.status,
            lost: info.lost,
            rtt: info.rtt,
        }
    }
}

#[derive(Debug, Clone)]
pub enum LineEventKind {
    OfflineIncreased { prev: u32, current: u32 },
    LostHighIncreased { prev: u32, current: u32 },
    RttHighIncreased { prev: u32, current: u32 },
    Recovered { field_name: String, prev: u32 },
}

#[derive(Debug, Clone)]
pub struct LineEvent {
    pub sn: String,
    pub remark: String,
    pub kind: LineEventKind,
    pub problem_lines: Vec<ProblemLine>,
}

pub fn check_line_changes(
    line_data_map: &HashMap<String, (String, NetLineDataResponse)>,
    state: &MonitorState,
    notify_on_recovery: bool,
    loss_threshold: f64,
    rtt_threshold: f64,
) -> Vec<LineEvent> {
    let mut events = Vec::new();

    if state.first_run {
        return events;
    }

    for (sn, (remark, data)) in line_data_map {
        let prev = state.line_statuses.get(sn);
        let default = LineStatus::default();
        let prev = prev.unwrap_or(&default);

        // Offline lines increased
        if data.offline_num > prev.offline_num {
            let problem_lines: Vec<ProblemLine> = data
                .line_data_list
                .iter()
                .filter(|l| l.status != 1)
                .map(|l| ProblemLine::from_line_info(l))
                .collect();
            events.push(LineEvent {
                sn: sn.clone(),
                remark: remark.clone(),
                kind: LineEventKind::OfflineIncreased {
                    prev: prev.offline_num,
                    current: data.offline_num,
                },
                problem_lines,
            });
        } else if notify_on_recovery && prev.offline_num > 0 && data.offline_num == 0 {
            events.push(LineEvent {
                sn: sn.clone(),
                remark: remark.clone(),
                kind: LineEventKind::Recovered {
                    field_name: "离线线路".to_string(),
                    prev: prev.offline_num,
                },
                problem_lines: vec![],
            });
        }

        // High packet loss increased
        if data.lost_high_num > prev.lost_high_num {
            let problem_lines: Vec<ProblemLine> = data
                .line_data_list
                .iter()
                .filter(|l| l.lost >= loss_threshold * 100.0)
                .map(|l| ProblemLine::from_line_info(l))
                .collect();
            events.push(LineEvent {
                sn: sn.clone(),
                remark: remark.clone(),
                kind: LineEventKind::LostHighIncreased {
                    prev: prev.lost_high_num,
                    current: data.lost_high_num,
                },
                problem_lines,
            });
        } else if notify_on_recovery && prev.lost_high_num > 0 && data.lost_high_num == 0 {
            events.push(LineEvent {
                sn: sn.clone(),
                remark: remark.clone(),
                kind: LineEventKind::Recovered {
                    field_name: "丢包过高".to_string(),
                    prev: prev.lost_high_num,
                },
                problem_lines: vec![],
            });
        }

        // High latency increased
        if data.rtt_high_num > prev.rtt_high_num {
            let problem_lines: Vec<ProblemLine> = data
                .line_data_list
                .iter()
                .filter(|l| l.rtt >= rtt_threshold)
                .map(|l| ProblemLine::from_line_info(l))
                .collect();
            events.push(LineEvent {
                sn: sn.clone(),
                remark: remark.clone(),
                kind: LineEventKind::RttHighIncreased {
                    prev: prev.rtt_high_num,
                    current: data.rtt_high_num,
                },
                problem_lines,
            });
        } else if notify_on_recovery && prev.rtt_high_num > 0 && data.rtt_high_num == 0 {
            events.push(LineEvent {
                sn: sn.clone(),
                remark: remark.clone(),
                kind: LineEventKind::Recovered {
                    field_name: "时延过高".to_string(),
                    prev: prev.rtt_high_num,
                },
                problem_lines: vec![],
            });
        }
    }

    events
}

pub fn line_status_from_response(resp: &NetLineDataResponse) -> LineStatus {
    LineStatus {
        offline_num: resp.offline_num,
        lost_high_num: resp.lost_high_num,
        rtt_high_num: resp.rtt_high_num,
        busy_offline_num: resp.busy_offline_num,
        total_count: resp.count,
    }
}

#[derive(Debug, Default)]
pub struct LineSummary {
    pub total_lines: u32,
    pub total_offline: u32,
    pub total_lost_high: u32,
    pub total_rtt_high: u32,
}

pub fn build_line_summary(
    line_data_map: &HashMap<String, (String, NetLineDataResponse)>,
) -> LineSummary {
    let mut summary = LineSummary::default();
    for (_sn, (_remark, data)) in line_data_map {
        summary.total_lines += data.count;
        summary.total_offline += data.offline_num;
        summary.total_lost_high += data.lost_high_num;
        summary.total_rtt_high += data.rtt_high_num;
    }
    summary
}

pub fn line_status_text(status: u8) -> &'static str {
    match status {
        0 => "未连接",
        1 => "已连接",
        2 => "连接中",
        _ => "未知",
    }
}
