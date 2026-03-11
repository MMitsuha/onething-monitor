use chrono::{DateTime, Local};
use std::collections::{HashMap, VecDeque};

#[derive(Debug, Clone)]
pub struct LineSample {
    pub timestamp: DateTime<Local>,
    /// Upload speed in bytes/s (from local device API)
    pub upspeed_bytes: Option<u64>,
    /// Download speed in bytes/s (from local device API)
    pub downspeed_bytes: Option<u64>,
    /// Packet loss % (from cloud API)
    pub lost: Option<f64>,
    /// RTT in ms (from cloud API)
    pub rtt: Option<f64>,
}

/// Per-device, per-line ring buffer of samples.
#[derive(Debug)]
pub struct ChartDataStore {
    /// device SN -> (remark, line_tag -> samples)
    data: HashMap<String, DeviceHistory>,
    max_samples: usize,
}

#[derive(Debug)]
pub struct DeviceHistory {
    pub remark: String,
    /// line identifier (e.g. "line0", "eth1", or nic name) -> ring buffer
    pub lines: HashMap<String, VecDeque<LineSample>>,
}

impl ChartDataStore {
    /// Create a new store.
    /// `history_hours`: how many hours of data to retain.
    /// `collection_interval_secs`: how often samples are collected.
    pub fn new(history_hours: u64, collection_interval_secs: u64) -> Self {
        let max_samples = if collection_interval_secs > 0 {
            ((history_hours * 3600) / collection_interval_secs) as usize
        } else {
            // fallback: 24h at 5min intervals
            288
        };
        Self {
            data: HashMap::new(),
            max_samples: max_samples.max(10),
        }
    }

    /// Push a sample for a specific device + line.
    pub fn push(
        &mut self,
        sn: &str,
        remark: &str,
        line_key: &str,
        sample: LineSample,
    ) {
        let device = self.data.entry(sn.to_string()).or_insert_with(|| DeviceHistory {
            remark: remark.to_string(),
            lines: HashMap::new(),
        });
        device.remark = remark.to_string();

        let buf = device
            .lines
            .entry(line_key.to_string())
            .or_insert_with(VecDeque::new);
        buf.push_back(sample);
        while buf.len() > self.max_samples {
            buf.pop_front();
        }
    }

    /// Get all device SNs that have data.
    pub fn device_sns(&self) -> Vec<String> {
        self.data.keys().cloned().collect()
    }

    /// Get device history by SN.
    pub fn get_device(&self, sn: &str) -> Option<&DeviceHistory> {
        self.data.get(sn)
    }

    /// Check if there's enough data to render a meaningful chart for a device.
    pub fn has_sufficient_data(&self, sn: &str, min_samples: usize) -> bool {
        self.data
            .get(sn)
            .map(|d| d.lines.values().any(|buf| buf.len() >= min_samples))
            .unwrap_or(false)
    }
}
