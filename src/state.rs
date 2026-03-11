use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{debug, warn};

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct LineStatus {
    pub offline_num: u32,
    pub lost_high_num: u32,
    pub rtt_high_num: u32,
    pub busy_offline_num: u32,
    pub total_count: u32,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct MonitorState {
    /// device SN -> device status (0=offline, 1=online, 2=error)
    #[serde(default)]
    pub device_statuses: HashMap<String, u8>,
    /// device SN -> yesterday income
    #[serde(default)]
    pub device_incomes: HashMap<String, f64>,
    /// device SN -> recruit status code
    #[serde(default)]
    pub recruit_statuses: HashMap<String, i32>,
    /// device SN -> line status summary
    #[serde(default)]
    pub line_statuses: HashMap<String, LineStatus>,
    /// last daily report date (YYYY-MM-DD)
    #[serde(default)]
    pub last_daily_report_date: String,
    /// whether this is the first run (no previous state)
    #[serde(default = "default_true")]
    pub first_run: bool,
}

fn default_true() -> bool {
    true
}

impl MonitorState {
    pub fn load(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(content) => match serde_json::from_str(&content) {
                Ok(state) => {
                    debug!("Loaded state from {}", path.display());
                    state
                }
                Err(e) => {
                    warn!("Failed to parse state file: {}, starting fresh", e);
                    Self::default()
                }
            },
            Err(_) => {
                debug!("No state file found, starting fresh");
                Self::default()
            }
        }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        debug!("Saved state to {}", path.display());
        Ok(())
    }

    pub fn state_path() -> PathBuf {
        PathBuf::from("state.json")
    }
}
