use super::client::{ApiError, OnethingClient};
use super::types::*;

impl OnethingClient {
    pub async fn get_device_list(
        &self,
        request: &DeviceListRequest,
    ) -> Result<DeviceListData, ApiError> {
        self.post("/v1/device/device_list", request).await
    }

    pub async fn get_all_devices(&self) -> Result<Vec<DeviceInfo>, ApiError> {
        let mut all_devices = Vec::new();
        let mut page = 1u32;

        loop {
            let req = DeviceListRequest {
                page,
                page_size: 200,
                ..Default::default()
            };
            let data = self.get_device_list(&req).await?;
            let count = data.device_info_list.len();
            all_devices.extend(data.device_info_list);

            if all_devices.len() as u32 >= data.count || count == 0 {
                break;
            }
            page += 1;
        }

        Ok(all_devices)
    }

    #[allow(dead_code)]
    pub async fn get_device_alarm_detail(&self, sn: &str) -> Result<DeviceAlarmData, ApiError> {
        let req = DeviceAlarmRequest { sn: sn.to_string() };
        self.post("/v1/device/device_alarm_detail", &req).await
    }

    pub async fn get_net_line_data(
        &self,
        sn: &str,
        user_id: &str,
    ) -> Result<NetLineDataResponse, ApiError> {
        let mut all_lines = Vec::new();
        let mut page = 1u32;
        let mut first_resp: Option<NetLineDataResponse> = None;

        loop {
            let req = NetLineDataRequest {
                sn: sn.to_string(),
                user_id: user_id.to_string(),
                invitee_user_id: String::new(),
                page,
                page_size: 20,
            };
            let mut data: NetLineDataResponse = self.post("/v1/device/net_line_data", &req).await?;
            let fetched = data.line_data_list.len();
            all_lines.append(&mut data.line_data_list);

            let total = data.count;
            if first_resp.is_none() {
                first_resp = Some(data);
            }

            if all_lines.len() as u32 >= total || fetched == 0 {
                break;
            }
            page += 1;
        }

        match first_resp {
            Some(mut resp) => {
                resp.line_data_list = all_lines;
                Ok(resp)
            }
            None => Ok(NetLineDataResponse {
                offline_num: 0,
                lost_high_num: 0,
                rtt_high_num: 0,
                busy_offline_num: 0,
                count: 0,
                update_time: String::new(),
                line_data_list: Vec::new(),
            }),
        }
    }

    #[allow(dead_code)]
    pub async fn get_filter_config(&self) -> Result<FilterConfigData, ApiError> {
        self.post("/v1/device/device_filter", &serde_json::json!({}))
            .await
    }

    /// Get signed URL for local device access via frp tunnel.
    pub async fn generate_url(&self, sn: &str) -> Result<GenerateUrlData, ApiError> {
        let req = GenerateUrlRequest { sn: sn.to_string() };
        self.post("/v1/device/generate_url", &req).await
    }

    /// Fetch real-time line status (speed data) from the local device via frp tunnel.
    /// Returns None if the local device is unreachable (frp flaky).
    pub async fn get_local_line_status(
        &self,
        sn: &str,
    ) -> Result<Option<LocalMultPPPoEStatus>, ApiError> {
        // Step 1: get signed URL
        let url_data = self.generate_url(sn).await?;
        let base_url = url_data.url.trim_end_matches('/').to_string();

        // Step 2: extract query params from the signed URL
        // URL format: http://{sn}.x86localweb.onethingcloud.com?expire=...&sign=...
        let query_params = if let Some(idx) = base_url.find('?') {
            &base_url[idx..]
        } else {
            ""
        };

        let local_url = if let Some(idx) = base_url.find('?') {
            format!(
                "{}/v1.0/devices/multpppoe/status{}",
                &base_url[..idx],
                query_params
            )
        } else {
            format!("{}/v1.0/devices/multpppoe/status", base_url)
        };

        // Step 3: fetch from local device (may fail due to frp tunnel issues)
        match self.get_local(&local_url).await {
            Ok(text) => match serde_json::from_str::<LocalMultPPPoEStatus>(&text) {
                Ok(status) => Ok(Some(status)),
                Err(e) => {
                    tracing::warn!("Failed to parse local line status for {}: {}", sn, e);
                    tracing::debug!("Raw local response: {}", text);
                    Ok(None)
                }
            },
            Err(e) => {
                tracing::warn!("Failed to reach local device {}: {}", sn, e);
                Ok(None)
            }
        }
    }
}
