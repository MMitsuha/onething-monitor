use serde::{Deserialize, Deserializer, Serialize};

/// Deserialize a number that may come as a string (e.g. "6145") or a number (6145).
fn deserialize_f64_from_string<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringOrNumber {
        String(String),
        Number(f64),
    }
    match StringOrNumber::deserialize(deserializer)? {
        StringOrNumber::String(s) => Ok(s.parse::<f64>().unwrap_or(0.0)),
        StringOrNumber::Number(n) => Ok(n),
    }
}

/// Deserialize a number that may come as "0.00%", "7.41", or a plain number.
/// Strips trailing '%' before parsing.
fn deserialize_f64_from_percent_string<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringOrNumber {
        String(String),
        Number(f64),
    }
    match StringOrNumber::deserialize(deserializer)? {
        StringOrNumber::String(s) => {
            let s = s.trim_end_matches('%').trim();
            if s.is_empty() {
                Ok(0.0)
            } else {
                s.parse::<f64>().map_err(serde::de::Error::custom)
            }
        }
        StringOrNumber::Number(n) => Ok(n),
    }
}

/// Generic API response wrapper
#[derive(Debug, Deserialize)]
pub struct ApiResponse<T> {
    #[serde(rename = "iRet")]
    pub i_ret: i32,
    #[serde(rename = "sMsg", default)]
    pub s_msg: String,
    pub data: Option<T>,
}

/// Auth expired error codes
pub const AUTH_EXPIRED_CODES: &[i32] = &[-11004, -11008, -11014];

// ─── Device List ───

#[derive(Debug, Serialize)]
pub struct DeviceListRequest {
    pub page: u32,
    #[serde(rename = "pageSize")]
    pub page_size: u32,
    #[serde(rename = "deviceGroup")]
    pub device_group: Vec<String>,
    #[serde(rename = "deviceType")]
    pub device_type: Vec<String>,
    #[serde(rename = "deviceStatus")]
    pub device_status: Vec<u8>,
    #[serde(rename = "bizId")]
    pub biz_id: Vec<String>,
    #[serde(rename = "fuzzyQuery")]
    pub fuzzy_query: String,
}

impl Default for DeviceListRequest {
    fn default() -> Self {
        Self {
            page: 1,
            page_size: 200,
            device_group: vec![],
            device_type: vec![],
            device_status: vec![],
            biz_id: vec![],
            fuzzy_query: String::new(),
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct DeviceListData {
    #[serde(rename = "deviceInfoList", default)]
    pub device_info_list: Vec<DeviceInfo>,
    #[serde(default)]
    pub count: u32,
    #[serde(default)]
    pub page: u32,
    #[serde(rename = "deviceExist", default)]
    pub device_exist: u32,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DeviceInfo {
    pub sn: String,
    #[serde(rename = "deviceType", default)]
    pub device_type: String,
    #[serde(rename = "deviceStatus", default)]
    pub device_status: u8,
    #[serde(rename = "deviceStatusError", default)]
    pub device_status_error: String,
    #[serde(rename = "deviceGroup", default)]
    pub device_group: String,
    #[serde(rename = "yIncome", default, deserialize_with = "deserialize_f64_from_string")]
    pub y_income: f64,
    #[serde(rename = "bizType", default)]
    pub biz_type: String,
    #[serde(rename = "bizId", default)]
    pub biz_id: String,
    #[serde(rename = "recruitStatus", default)]
    pub recruit_status: i32,
    #[serde(rename = "recruitStatusText", default)]
    pub recruit_status_text: String,
    #[serde(default)]
    pub province: Option<i32>,
    #[serde(default)]
    pub isp: Option<i32>,
    #[serde(rename = "deviceRemark", default)]
    pub device_remark: String,
    #[serde(rename = "limitSpeedFlag", default)]
    pub limit_speed_flag: u8,
}

// ─── Device Alarm ───

#[derive(Debug, Serialize)]
pub struct DeviceAlarmRequest {
    pub sn: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DeviceAlarmData {
    #[serde(default)]
    pub alarms: Vec<AlarmItem>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AlarmItem {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub level: String,
    #[serde(default)]
    pub message: String,
}

// ─── Day Bills ───

#[derive(Debug, Serialize)]
pub struct DayBillsRequest {
    #[serde(default)]
    pub page: Option<u32>,
    #[serde(rename = "pageSize", default)]
    pub page_size: Option<u32>,
}

impl Default for DayBillsRequest {
    fn default() -> Self {
        Self {
            page: Some(1),
            page_size: Some(50),
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct BillsData {
    #[serde(default)]
    pub list: Vec<BillItem>,
    #[serde(default)]
    pub total: u32,
}

#[derive(Debug, Deserialize, Clone)]
pub struct BillItem {
    #[serde(default)]
    pub date: String,
    #[serde(default)]
    pub income: f64,
    #[serde(rename = "deviceCount", default)]
    pub device_count: u32,
}

// ─── Filter Config ───

#[derive(Debug, Deserialize, Clone)]
pub struct FilterConfigData {
    #[serde(rename = "deviceType", default)]
    pub device_type: Vec<FilterItem>,
    #[serde(rename = "deviceStatus", default)]
    pub device_status: Vec<FilterItem>,
    #[serde(rename = "deviceGroup", default)]
    pub device_group: Vec<FilterItem>,
    #[serde(rename = "bizType", default)]
    pub biz_type: Vec<FilterItem>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct FilterItem {
    pub id: serde_json::Value,
    pub name: String,
}

// ─── Net Line Data ───

#[derive(Debug, Serialize)]
pub struct NetLineDataRequest {
    pub sn: String,
    #[serde(rename = "userId")]
    pub user_id: String,
    #[serde(rename = "inviteeUserId")]
    pub invitee_user_id: String,
    #[serde(default)]
    pub page: u32,
    #[serde(rename = "pageSize", default)]
    pub page_size: u32,
}

#[derive(Debug, Deserialize, Clone)]
pub struct NetLineDataResponse {
    #[serde(rename = "offLineNum", default)]
    pub offline_num: u32,
    #[serde(rename = "lostHighNum", default)]
    pub lost_high_num: u32,
    #[serde(rename = "rttHighNum", default)]
    pub rtt_high_num: u32,
    #[serde(rename = "busyOffLineNum", default)]
    pub busy_offline_num: u32,
    #[serde(default)]
    pub count: u32,
    #[serde(rename = "updateTime", default)]
    pub update_time: String,
    #[serde(rename = "lineDataList", default)]
    pub line_data_list: Vec<NetLineInfo>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct NetLineInfo {
    #[serde(rename = "dialAccount", default)]
    pub dial_account: String,
    #[serde(default)]
    pub ipv4: String,
    #[serde(default)]
    pub nic: String,
    #[serde(default)]
    pub status: u8,
    #[serde(rename = "natType", default)]
    pub nat_type: u8,
    #[serde(default, deserialize_with = "deserialize_f64_from_percent_string")]
    pub lost: f64,
    #[serde(default, deserialize_with = "deserialize_f64_from_percent_string")]
    pub rtt: f64,
    #[serde(default)]
    pub ipv6: String,
    #[serde(rename = "vlanId", default)]
    pub vlan_id: String,
    #[serde(rename = "lineNo", default)]
    pub line_no: u32,
}

// ─── Generate URL (local device access) ───

#[derive(Debug, Serialize)]
pub struct GenerateUrlRequest {
    pub sn: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct GenerateUrlData {
    pub url: String,
}

// ─── Local Device MultPPPoE Status ───

#[derive(Debug, Deserialize, Clone)]
pub struct LocalMultPPPoEStatus {
    #[serde(default)]
    pub sn: String,
    #[serde(default)]
    pub totalline: u32,
    #[serde(default)]
    pub connectedline: u32,
    #[serde(default)]
    pub multidial: Vec<LocalLineInfo>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct LocalLineInfo {
    #[serde(default)]
    pub tag: String,
    #[serde(default)]
    pub ipaddr: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub upspeed: u64,
    #[serde(default)]
    pub downspeed: u64,
    #[serde(default)]
    pub sentbytes: String,
    #[serde(default)]
    pub recvbytes: String,
    #[serde(default)]
    pub nic: String,
    #[serde(default)]
    pub ipaddr6: String,
    #[serde(default)]
    pub vlanid: u32,
    #[serde(default)]
    pub lineid: u32,
}

// ─── Status helpers ───

impl DeviceInfo {
    pub fn status_text(&self) -> &str {
        match self.device_status {
            0 => "离线",
            1 => "在线",
            2 => "异常",
            _ => "未知",
        }
    }
}

