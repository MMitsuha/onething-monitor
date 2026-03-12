use super::client::{ApiError, OnethingClient};
use super::types::*;

#[allow(dead_code)]
impl OnethingClient {
    pub async fn get_day_bills(&self, request: &DayBillsRequest) -> Result<BillsData, ApiError> {
        self.post("/v3/user/proxy/x86/day/bills", request).await
    }

    pub async fn get_month_bills(&self, request: &DayBillsRequest) -> Result<BillsData, ApiError> {
        self.post("/v3/user/proxy/x86/month/bills", request).await
    }
}
