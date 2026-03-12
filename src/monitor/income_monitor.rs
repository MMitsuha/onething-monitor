use crate::api::types::DeviceInfo;
use crate::state::MonitorState;

#[derive(Debug, Clone)]
pub struct IncomeEvent {
    pub sn: String,
    pub device_type: String,
    pub remark: String,
    pub kind: IncomeEventKind,
}

#[derive(Debug, Clone)]
pub enum IncomeEventKind {
    ZeroIncome,
    SignificantDrop {
        previous: f64,
        current: f64,
        drop_ratio: f64,
    },
}

#[derive(Debug)]
pub struct IncomeSummary {
    pub total_income: f64,
    pub device_count: u32,
    pub online_count: u32,
    pub offline_count: u32,
    pub error_count: u32,
    pub zero_income_count: u32,
    pub per_device: Vec<DeviceIncome>,
}

#[derive(Debug)]
pub struct DeviceIncome {
    pub sn: String,
    pub device_type: String,
    pub remark: String,
    pub income: f64,
    pub status: u8,
}

pub fn check_income_changes(
    devices: &[DeviceInfo],
    state: &MonitorState,
    drop_threshold: f64,
) -> Vec<IncomeEvent> {
    let mut events = Vec::new();

    if state.first_run {
        return events;
    }

    for device in devices {
        let prev_income = state.device_incomes.get(&device.sn);

        if let Some(&prev) = prev_income {
            // Zero income alert (only if previously had income)
            if device.y_income == 0.0 && prev > 0.0 {
                events.push(IncomeEvent {
                    sn: device.sn.clone(),
                    device_type: device.device_type.clone(),
                    remark: device.device_remark.clone(),
                    kind: IncomeEventKind::ZeroIncome,
                });
            }
            // Significant drop alert
            else if prev > 0.0 && device.y_income > 0.0 {
                let drop = (prev - device.y_income) / prev;
                if drop > drop_threshold {
                    events.push(IncomeEvent {
                        sn: device.sn.clone(),
                        device_type: device.device_type.clone(),
                        remark: device.device_remark.clone(),
                        kind: IncomeEventKind::SignificantDrop {
                            previous: prev,
                            current: device.y_income,
                            drop_ratio: drop,
                        },
                    });
                }
            }
        }
    }

    events
}

pub fn build_income_summary(devices: &[DeviceInfo]) -> IncomeSummary {
    let mut total = 0.0;
    let mut online = 0u32;
    let mut offline = 0u32;
    let mut error = 0u32;
    let mut zero_income = 0u32;
    let mut per_device = Vec::new();

    for device in devices {
        total += device.y_income;
        match device.device_status {
            0 => offline += 1,
            1 => online += 1,
            2 => error += 1,
            _ => {}
        }
        if device.y_income == 0.0 {
            zero_income += 1;
        }
        per_device.push(DeviceIncome {
            sn: device.sn.clone(),
            device_type: device.device_type.clone(),
            remark: device.device_remark.clone(),
            income: device.y_income,
            status: device.device_status,
        });
    }

    // Sort by income descending
    per_device.sort_by(|a, b| {
        b.income
            .partial_cmp(&a.income)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    IncomeSummary {
        total_income: total,
        device_count: devices.len() as u32,
        online_count: online,
        offline_count: offline,
        error_count: error,
        zero_income_count: zero_income,
        per_device,
    }
}
