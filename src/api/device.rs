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
            let mut data: NetLineDataResponse =
                self.post("/v1/device/net_line_data", &req).await?;
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

    pub async fn get_filter_config(&self) -> Result<FilterConfigData, ApiError> {
        self.post("/v1/device/device_filter", &serde_json::json!({}))
            .await
    }
}
