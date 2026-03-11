use anyhow::Result;
use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use tracing::{debug, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Serialize, Deserialize)]
pub struct ChartDataStore {
    /// device SN -> (remark, line_tag -> samples)
    data: HashMap<String, DeviceHistory>,
    #[serde(skip)]
    max_samples: usize,
}

#[derive(Debug, Serialize, Deserialize)]
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

    /// Load chart data from disk. Falls back to empty store on any error.
    pub fn load(path: &Path, history_hours: u64, collection_interval_secs: u64) -> Self {
        let max_samples = Self::calc_max_samples(history_hours, collection_interval_secs);

        match std::fs::read_to_string(path) {
            Ok(content) => match serde_json::from_str::<ChartDataStore>(&content) {
                Ok(mut store) => {
                    store.max_samples = max_samples;
                    // Trim buffers in case max_samples changed
                    for device in store.data.values_mut() {
                        for buf in device.lines.values_mut() {
                            while buf.len() > max_samples {
                                buf.pop_front();
                            }
                        }
                    }
                    debug!(
                        "Loaded chart data from {} ({} devices)",
                        path.display(),
                        store.data.len()
                    );
                    store
                }
                Err(e) => {
                    warn!("Failed to parse chart data file: {}, starting fresh", e);
                    Self::new(history_hours, collection_interval_secs)
                }
            },
            Err(_) => {
                debug!("No chart data file found, starting fresh");
                Self::new(history_hours, collection_interval_secs)
            }
        }
    }

    /// Save chart data to disk.
    pub fn save(&self, path: &Path) -> Result<()> {
        let content = serde_json::to_string(self)?;
        std::fs::write(path, content)?;
        debug!("Saved chart data to {}", path.display());
        Ok(())
    }

    pub fn chart_data_path() -> PathBuf {
        PathBuf::from("chart_data.json")
    }

    fn calc_max_samples(history_hours: u64, collection_interval_secs: u64) -> usize {
        let max = if collection_interval_secs > 0 {
            ((history_hours * 3600) / collection_interval_secs) as usize
        } else {
            288
        };
        max.max(10)
    }
}
