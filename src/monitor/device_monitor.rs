use crate::api::types::DeviceInfo;
use crate::state::MonitorState;

#[derive(Debug, Clone)]
pub struct DeviceEvent {
    pub sn: String,
    pub device_type: String,
    pub remark: String,
    pub kind: DeviceEventKind,
}

#[derive(Debug, Clone)]
pub enum DeviceEventKind {
    WentOffline,
    WentOnline,
    WentError { error: String },
    RecoveredFromError,
    NewDevice { status: u8 },
    Disappeared,
}

impl DeviceEvent {
    pub fn emoji(&self) -> &str {
        match &self.kind {
            DeviceEventKind::WentOffline => "\u{1f534}",       // red circle
            DeviceEventKind::WentOnline => "\u{1f7e2}",        // green circle
            DeviceEventKind::WentError { .. } => "\u{26a0}\u{fe0f}",  // warning
            DeviceEventKind::RecoveredFromError => "\u{2705}",  // check
            DeviceEventKind::NewDevice { .. } => "\u{1f195}",   // new
            DeviceEventKind::Disappeared => "\u{274c}",         // cross
        }
    }

    pub fn description(&self) -> String {
        let name = if self.remark.is_empty() {
            &self.sn
        } else {
            &self.remark
        };
        match &self.kind {
            DeviceEventKind::WentOffline => {
                format!("{} {} ({}) 已离线", self.emoji(), name, self.device_type)
            }
            DeviceEventKind::WentOnline => {
                format!("{} {} ({}) 已恢复在线", self.emoji(), name, self.device_type)
            }
            DeviceEventKind::WentError { error } => {
                format!(
                    "{} {} ({}) 异常: {}",
                    self.emoji(),
                    name,
                    self.device_type,
                    error
                )
            }
            DeviceEventKind::RecoveredFromError => {
                format!(
                    "{} {} ({}) 从异常恢复",
                    self.emoji(),
                    name,
                    self.device_type
                )
            }
            DeviceEventKind::NewDevice { status } => {
                let status_text = match status {
                    0 => "离线",
                    1 => "在线",
                    2 => "异常",
                    _ => "未知",
                };
                format!(
                    "{} 发现新设备 {} ({}) [{}]",
                    self.emoji(),
                    name,
                    self.device_type,
                    status_text
                )
            }
            DeviceEventKind::Disappeared => {
                format!("{} 设备 {} 已消失", self.emoji(), name)
            }
        }
    }
}

pub fn check_device_changes(
    devices: &[DeviceInfo],
    state: &MonitorState,
    notify_on_recovery: bool,
) -> Vec<DeviceEvent> {
    let mut events = Vec::new();

    for device in devices {
        let prev_status = state.device_statuses.get(&device.sn);

        match prev_status {
            None => {
                // Skip new device events on first run
                if !state.first_run {
                    events.push(DeviceEvent {
                        sn: device.sn.clone(),
                        device_type: device.device_type.clone(),
                        remark: device.device_remark.clone(),
                        kind: DeviceEventKind::NewDevice {
                            status: device.device_status,
                        },
                    });
                }
            }
            Some(&prev) if prev != device.device_status => {
                let kind = match (prev, device.device_status) {
                    (1, 0) => Some(DeviceEventKind::WentOffline),
                    (_, 0) => Some(DeviceEventKind::WentOffline),
                    (0, 1) if notify_on_recovery => Some(DeviceEventKind::WentOnline),
                    (2, 1) if notify_on_recovery => Some(DeviceEventKind::RecoveredFromError),
                    (_, 2) => Some(DeviceEventKind::WentError {
                        error: device.device_status_error.clone(),
                    }),
                    (2, _) if notify_on_recovery => Some(DeviceEventKind::RecoveredFromError),
                    _ => None,
                };

                if let Some(kind) = kind {
                    events.push(DeviceEvent {
                        sn: device.sn.clone(),
                        device_type: device.device_type.clone(),
                        remark: device.device_remark.clone(),
                        kind,
                    });
                }
            }
            _ => {}
        }
    }

    // Check for disappeared devices (skip on first run)
    if !state.first_run {
        let current_sns: std::collections::HashSet<&str> =
            devices.iter().map(|d| d.sn.as_str()).collect();
        for sn in state.device_statuses.keys() {
            if !current_sns.contains(sn.as_str()) {
                events.push(DeviceEvent {
                    sn: sn.clone(),
                    device_type: String::new(),
                    remark: String::new(),
                    kind: DeviceEventKind::Disappeared,
                });
            }
        }
    }

    events
}
