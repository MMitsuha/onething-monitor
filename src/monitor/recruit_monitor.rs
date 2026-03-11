use crate::api::types::{self, RecruitDeviceInfo};
use crate::state::MonitorState;

#[derive(Debug, Clone)]
pub struct RecruitEvent {
    pub sn: String,
    pub biz_name: String,
    pub kind: RecruitEventKind,
}

#[derive(Debug, Clone)]
pub enum RecruitEventKind {
    StatusChanged {
        old_status: i32,
        new_status: i32,
        old_text: String,
        new_text: String,
    },
    NewRecruit {
        status: i32,
        status_text: String,
    },
    Removed {
        old_status: i32,
        old_text: String,
    },
}

impl RecruitEvent {
    pub fn emoji(&self) -> &str {
        match &self.kind {
            RecruitEventKind::StatusChanged {
                new_status, ..
            } => {
                match *new_status {
                    1 => "\u{1f7e2}",        // green - running
                    9 | 10 => "\u{1f534}",   // red - kicked/blacklisted
                    7 => "\u{26a0}\u{fe0f}",  // warning - pending fix
                    _ => "\u{1f4cb}",         // clipboard
                }
            }
            RecruitEventKind::NewRecruit { .. } => "\u{1f195}",
            RecruitEventKind::Removed { .. } => "\u{274c}",
        }
    }

    pub fn description(&self) -> String {
        match &self.kind {
            RecruitEventKind::StatusChanged {
                old_text,
                new_text,
                ..
            } => {
                format!(
                    "{} [{}] {} 定向状态: {} -> {}",
                    self.emoji(),
                    self.biz_name,
                    self.sn,
                    old_text,
                    new_text
                )
            }
            RecruitEventKind::NewRecruit { status_text, .. } => {
                format!(
                    "{} [{}] {} 新增定向业务 [{}]",
                    self.emoji(),
                    self.biz_name,
                    self.sn,
                    status_text
                )
            }
            RecruitEventKind::Removed { old_text, .. } => {
                format!(
                    "{} [{}] {} 定向业务已移除 (之前: {})",
                    self.emoji(),
                    self.biz_name,
                    self.sn,
                    old_text
                )
            }
        }
    }
}

pub fn check_recruit_changes(
    devices: &[RecruitDeviceInfo],
    state: &MonitorState,
) -> Vec<RecruitEvent> {
    let mut events = Vec::new();

    if state.first_run {
        return events;
    }

    for device in devices {
        let key = format!("{}:{}", device.sn, device.biz_id);
        let prev_status = state.recruit_statuses.get(&key);

        match prev_status {
            None => {
                events.push(RecruitEvent {
                    sn: device.sn.clone(),
                    biz_name: device.biz_name.clone(),
                    kind: RecruitEventKind::NewRecruit {
                        status: device.status,
                        status_text: device.status_text.clone(),
                    },
                });
            }
            Some(&prev) if prev != device.status => {
                events.push(RecruitEvent {
                    sn: device.sn.clone(),
                    biz_name: device.biz_name.clone(),
                    kind: RecruitEventKind::StatusChanged {
                        old_status: prev,
                        new_status: device.status,
                        old_text: types::recruit_status_text(prev).to_string(),
                        new_text: device.status_text.clone(),
                    },
                });
            }
            _ => {}
        }
    }

    // Check for removed recruit entries
    let current_keys: std::collections::HashSet<String> = devices
        .iter()
        .map(|d| format!("{}:{}", d.sn, d.biz_id))
        .collect();
    for (key, &status) in &state.recruit_statuses {
        if !current_keys.contains(key) {
            let parts: Vec<&str> = key.splitn(2, ':').collect();
            let sn = parts.first().unwrap_or(&"").to_string();
            events.push(RecruitEvent {
                sn,
                biz_name: String::new(),
                kind: RecruitEventKind::Removed {
                    old_status: status,
                    old_text: types::recruit_status_text(status).to_string(),
                },
            });
        }
    }

    events
}
