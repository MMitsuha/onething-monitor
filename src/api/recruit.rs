use super::client::{ApiError, OnethingClient};
use super::types::*;

impl OnethingClient {
    pub async fn get_recruit_device_list(
        &self,
        request: &RecruitDeviceListRequest,
    ) -> Result<RecruitDeviceListData, ApiError> {
        self.post("/v3/recruit/device/list", request).await
    }

    pub async fn get_all_recruit_devices(&self) -> Result<Vec<RecruitDeviceInfo>, ApiError> {
        let mut all = Vec::new();
        let mut page = 1u32;

        loop {
            let req = RecruitDeviceListRequest {
                page: Some(page),
                page_size: Some(200),
                platform_type: 1,
            };
            let data = self.get_recruit_device_list(&req).await?;
            let count = data.list.len();
            all.extend(data.list);

            if all.len() as u32 >= data.count || count == 0 {
                break;
            }
            page += 1;
        }

        Ok(all)
    }
}
