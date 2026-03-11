use anyhow::Result;
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE, COOKIE};
use serde::{de::DeserializeOwned, Serialize};
use tracing::{debug, trace, warn};

use super::types::{ApiResponse, AUTH_EXPIRED_CODES};
use crate::config::ApiConfig;

const BASE_URL: &str = "https://api-consolepro.onethingcloud.com";

#[derive(Clone)]
pub struct OnethingClient {
    client: reqwest::Client,
    base_url: String,
    cookie_value: String,
}

#[derive(Debug)]
pub enum ApiError {
    AuthExpired(String),
    ApiError { code: i32, msg: String },
    HttpError(reqwest::Error),
    Other(String),
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApiError::AuthExpired(msg) => write!(f, "Auth expired: {}", msg),
            ApiError::ApiError { code, msg } => write!(f, "API error (code={}): {}", code, msg),
            ApiError::HttpError(e) => write!(f, "HTTP error: {}", e),
            ApiError::Other(msg) => write!(f, "{}", msg),
        }
    }
}

impl std::error::Error for ApiError {}

impl From<reqwest::Error> for ApiError {
    fn from(e: reqwest::Error) -> Self {
        ApiError::HttpError(e)
    }
}

impl OnethingClient {
    pub fn new(config: &ApiConfig) -> Result<Self> {
        let cookie_value = format!("sessionid={}; userid={}", config.session_id, config.user_id);

        let mut headers = HeaderMap::new();
        headers.insert(COOKIE, HeaderValue::from_str(&cookie_value)?);
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .build()?;

        Ok(Self {
            client,
            base_url: BASE_URL.to_string(),
            cookie_value: cookie_value.clone(),
        })
    }

    /// GET request to an arbitrary URL (used for local device API).
    /// Returns the raw response text. The caller is responsible for parsing.
    pub async fn get_local(&self, url: &str) -> Result<String, ApiError> {
        debug!("GET {}", url);
        let response = self
            .client
            .get(url)
            .timeout(std::time::Duration::from_secs(10))
            .header(COOKIE, &self.cookie_value)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            return Err(ApiError::Other(format!(
                "Local API HTTP {}: {}",
                status.as_u16(),
                url
            )));
        }

        let text = response.text().await.map_err(ApiError::HttpError)?;
        trace!("Local response: {}", text);
        Ok(text)
    }

    pub async fn post<Req, Resp>(&self, path: &str, body: &Req) -> Result<Resp, ApiError>
    where
        Req: Serialize,
        Resp: DeserializeOwned,
    {
        let url = format!("{}{}", self.base_url, path);
        debug!("POST {}", url);

        let response = self.client.post(&url).json(body).send().await?;

        let status = response.status();
        if status.as_u16() == 401 {
            return Err(ApiError::AuthExpired("HTTP 401 Unauthorized".into()));
        }

        let raw_text = response.text().await.map_err(ApiError::HttpError)?;
        trace!("Response from {}: {}", path, raw_text);

        let api_resp: ApiResponse<Resp> = serde_json::from_str(&raw_text).map_err(|e| {
            warn!("Failed to parse response from {}: {}", path, e);
            debug!("Raw response body: {}", raw_text);
            ApiError::Other(format!("JSON parse error: {}", e))
        })?;

        if AUTH_EXPIRED_CODES.contains(&api_resp.i_ret) {
            return Err(ApiError::AuthExpired(format!(
                "iRet={}: {}",
                api_resp.i_ret, api_resp.s_msg
            )));
        }

        if api_resp.i_ret != 0 {
            warn!(
                "API error on {}: iRet={}, sMsg={}",
                path, api_resp.i_ret, api_resp.s_msg
            );
            return Err(ApiError::ApiError {
                code: api_resp.i_ret,
                msg: api_resp.s_msg,
            });
        }

        api_resp.data.ok_or_else(|| ApiError::ApiError {
            code: api_resp.i_ret,
            msg: "Response data is null".into(),
        })
    }
}
