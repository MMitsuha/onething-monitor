use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub api: ApiConfig,
    pub telegram: TelegramConfig,
    #[serde(default)]
    pub monitor: MonitorConfig,
    #[serde(default)]
    pub alert: AlertConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ApiConfig {
    pub session_id: String,
    pub user_id: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct TelegramConfig {
    pub bot_token: String,
    pub chat_id: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct MonitorConfig {
    #[serde(default = "default_device_interval")]
    pub device_check_interval_secs: u64,
    #[serde(default = "default_income_interval")]
    pub income_check_interval_secs: u64,
    #[serde(default = "default_report_hour")]
    pub daily_report_hour: u32,
    #[serde(default = "default_log_level")]
    pub log_level: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AlertConfig {
    #[serde(default = "default_income_threshold")]
    pub income_drop_threshold: f64,
    #[serde(default = "default_true")]
    pub notify_on_recovery: bool,
}

fn default_device_interval() -> u64 {
    60
}
fn default_income_interval() -> u64 {
    300
}
fn default_report_hour() -> u32 {
    9
}
fn default_log_level() -> String {
    "info".to_string()
}
fn default_income_threshold() -> f64 {
    0.5
}
fn default_true() -> bool {
    true
}

impl Default for MonitorConfig {
    fn default() -> Self {
        Self {
            device_check_interval_secs: default_device_interval(),
            income_check_interval_secs: default_income_interval(),
            daily_report_hour: default_report_hour(),
            log_level: default_log_level(),
        }
    }
}

impl Default for AlertConfig {
    fn default() -> Self {
        Self {
            income_drop_threshold: default_income_threshold(),
            notify_on_recovery: default_true(),
        }
    }
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {}", path.display()))?;
        let config: Config =
            toml::from_str(&content).with_context(|| "Failed to parse config file")?;
        Ok(config)
    }
}
