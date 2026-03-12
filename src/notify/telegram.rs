use anyhow::Result;
use serde::Deserialize;
use tracing::{debug, error, warn};

use crate::config::TelegramConfig;

// ─── Telegram getUpdates types ───

#[derive(Debug, Deserialize)]
pub struct TelegramUpdateResponse {
    pub ok: bool,
    #[serde(default)]
    pub result: Vec<TelegramUpdate>,
}

#[derive(Debug, Deserialize)]
pub struct TelegramUpdate {
    pub update_id: i64,
    pub message: Option<TelegramMessage>,
}

#[derive(Debug, Deserialize)]
pub struct TelegramMessage {
    pub chat: TelegramChat,
    pub text: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TelegramChat {
    pub id: i64,
}

// ─── Notifier ───

#[derive(Clone)]
pub struct TelegramNotifier {
    bot_token: String,
    chat_id: String,
    client: reqwest::Client,
}

impl TelegramNotifier {
    pub fn chat_id(&self) -> &str {
        &self.chat_id
    }
}

impl TelegramNotifier {
    pub fn new(config: &TelegramConfig) -> Self {
        Self {
            bot_token: config.bot_token.clone(),
            chat_id: config.chat_id.clone(),
            client: reqwest::Client::new(),
        }
    }

    pub async fn send_message(&self, text: &str) -> Result<()> {
        let url = format!("https://api.telegram.org/bot{}/sendMessage", self.bot_token);

        // Split long messages (Telegram limit is 4096 chars)
        let chunks = split_message(text, 4000);

        for chunk in &chunks {
            let resp = self
                .client
                .post(&url)
                .json(&serde_json::json!({
                    "chat_id": self.chat_id,
                    "text": chunk,
                    "parse_mode": "HTML",
                    "disable_web_page_preview": true,
                }))
                .send()
                .await?;

            if !resp.status().is_success() {
                let body = resp.text().await.unwrap_or_default();
                error!("Telegram API error: {}", body);
                // If HTML parse fails, retry without parse_mode
                if body.contains("can't parse entities") {
                    warn!("Retrying without HTML parse mode");
                    self.send_plain_message(chunk).await?;
                }
            } else {
                debug!("Telegram message sent successfully");
            }

            // Small delay between chunks to respect rate limits
            if chunks.len() > 1 {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
        }

        Ok(())
    }

    async fn send_plain_message(&self, text: &str) -> Result<()> {
        let url = format!("https://api.telegram.org/bot{}/sendMessage", self.bot_token);

        self.client
            .post(&url)
            .json(&serde_json::json!({
                "chat_id": self.chat_id,
                "text": text,
                "disable_web_page_preview": true,
            }))
            .send()
            .await?;

        Ok(())
    }

    /// Send a photo (PNG bytes) with an optional caption via Telegram sendPhoto.
    pub async fn send_photo(&self, png_bytes: Vec<u8>, caption: &str) -> Result<()> {
        let url = format!("https://api.telegram.org/bot{}/sendPhoto", self.bot_token);

        let photo_part = reqwest::multipart::Part::bytes(png_bytes)
            .file_name("chart.png")
            .mime_str("image/png")?;

        let form = reqwest::multipart::Form::new()
            .text("chat_id", self.chat_id.clone())
            .text("caption", caption.to_string())
            .part("photo", photo_part);

        let resp = self.client.post(&url).multipart(form).send().await?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            error!("Telegram sendPhoto error: {}", body);
            anyhow::bail!("Telegram sendPhoto failed: {}", body);
        } else {
            debug!("Telegram photo sent successfully");
        }

        Ok(())
    }

    /// Long-poll for new updates from Telegram Bot API.
    pub async fn get_updates(&self, offset: i64, timeout: u32) -> Result<Vec<TelegramUpdate>> {
        let url = format!("https://api.telegram.org/bot{}/getUpdates", self.bot_token);

        let resp = self
            .client
            .get(&url)
            .query(&[
                ("offset", offset.to_string()),
                ("timeout", timeout.to_string()),
                ("allowed_updates", "message".to_string()),
            ])
            .timeout(std::time::Duration::from_secs((timeout + 10) as u64))
            .send()
            .await?;

        let body: TelegramUpdateResponse = resp.json().await?;
        if !body.ok {
            anyhow::bail!("Telegram getUpdates returned ok=false");
        }
        Ok(body.result)
    }

    pub async fn send_alerts(&self, messages: &[String]) -> Result<()> {
        if messages.is_empty() {
            return Ok(());
        }

        let combined = messages.join("\n\n");
        self.send_message(&combined).await
    }
}

fn split_message(text: &str, max_len: usize) -> Vec<String> {
    if text.len() <= max_len {
        return vec![text.to_string()];
    }

    let mut chunks = Vec::new();
    let mut current = String::new();

    for line in text.lines() {
        if current.len() + line.len() + 1 > max_len {
            if !current.is_empty() {
                chunks.push(current.clone());
                current.clear();
            }
            // If a single line exceeds max, truncate it
            if line.len() > max_len {
                chunks.push(line[..max_len].to_string());
                continue;
            }
        }
        if !current.is_empty() {
            current.push('\n');
        }
        current.push_str(line);
    }

    if !current.is_empty() {
        chunks.push(current);
    }

    chunks
}
