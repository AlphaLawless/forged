use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::ai::provider::{AiProvider, GenerateOpts};

const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";
const API_VERSION: &str = "2023-06-01";
const DEFAULT_MODEL: &str = "claude-sonnet-4-6-20250514";

#[derive(Serialize)]
struct Message {
    role: &'static str,
    content: String,
}

#[derive(Serialize)]
struct RequestBody {
    model: String,
    max_tokens: u32,
    system: String,
    messages: Vec<Message>,
    temperature: f32,
}

#[derive(Deserialize)]
struct ContentBlock {
    text: Option<String>,
}

#[derive(Deserialize)]
struct ApiResponse {
    content: Vec<ContentBlock>,
}

#[derive(Deserialize)]
struct ApiError {
    error: Option<ApiErrorDetail>,
}

#[derive(Deserialize)]
struct ApiErrorDetail {
    message: Option<String>,
}

#[derive(Debug)]
pub struct ClaudeProvider {
    api_key: String,
    base_url: String,
    client: Client,
}

impl ClaudeProvider {
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            base_url: DEFAULT_BASE_URL.into(),
            client: Client::new(),
        }
    }

    /// Create with a custom base URL (used for testing with mockito).
    pub fn with_base_url(api_key: String, base_url: String) -> Self {
        Self {
            api_key,
            base_url,
            client: Client::new(),
        }
    }
}

#[async_trait]
impl AiProvider for ClaudeProvider {
    fn name(&self) -> &str {
        "claude"
    }

    fn default_model(&self) -> &str {
        DEFAULT_MODEL
    }

    async fn complete(
        &self,
        system: &str,
        user: &str,
        opts: &GenerateOpts,
    ) -> Result<String> {
        let url = format!("{}/v1/messages", self.base_url);

        let body = RequestBody {
            model: opts.model.clone(),
            max_tokens: opts.max_tokens,
            system: system.to_string(),
            messages: vec![Message {
                role: "user",
                content: user.to_string(),
            }],
            temperature: opts.temperature,
        };

        let response = self
            .client
            .post(&url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", API_VERSION)
            .header("content-type", "application/json")
            .json(&body)
            .timeout(Duration::from_secs(opts.timeout_secs))
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    anyhow::anyhow!(
                        "Request timed out after {} seconds. The API took too long to respond.",
                        opts.timeout_secs
                    )
                } else if e.is_connect() {
                    anyhow::anyhow!("Failed to connect to Anthropic API. Are you connected to the internet?")
                } else {
                    anyhow::anyhow!("HTTP request failed: {e}")
                }
            })?;

        let status = response.status();

        if status == 401 {
            bail!("Invalid API key. Check your Claude API key and try again.");
        }

        if status == 429 {
            bail!("Rate limit exceeded. Please wait a moment and try again.");
        }

        if !status.is_success() {
            let body_text = response.text().await.unwrap_or_default();
            // Try to parse error message from API
            if let Ok(api_err) = serde_json::from_str::<ApiError>(&body_text)
                && let Some(detail) = api_err.error
                && let Some(msg) = detail.message
            {
                bail!("Claude API error ({}): {}", status.as_u16(), msg);
            }
            bail!("Claude API error ({}): {}", status.as_u16(), body_text);
        }

        let api_response: ApiResponse = response
            .json()
            .await
            .context("Failed to parse Claude API response")?;

        let text = api_response
            .content
            .into_iter()
            .find_map(|block| block.text)
            .context("Claude API response contained no text")?;

        Ok(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockito::{Mock, Server};

    fn test_opts() -> GenerateOpts {
        GenerateOpts {
            model: "claude-sonnet-4-6-20250514".into(),
            temperature: 0.4,
            max_tokens: 1000,
            completions: 1,
            timeout_secs: 5,
        }
    }

    fn success_response(text: &str) -> String {
        serde_json::json!({
            "content": [{"type": "text", "text": text}],
            "model": "claude-sonnet-4-6-20250514",
            "role": "assistant"
        })
        .to_string()
    }

    async fn setup_mock(server: &mut Server, status: usize, body: &str) -> Mock {
        server
            .mock("POST", "/v1/messages")
            .with_status(status)
            .with_header("content-type", "application/json")
            .with_body(body)
            .create_async()
            .await
    }

    #[tokio::test]
    async fn test_claude_sends_correct_headers() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("POST", "/v1/messages")
            .match_header("x-api-key", "sk-test-key")
            .match_header("anthropic-version", API_VERSION)
            .with_status(200)
            .with_body(success_response("feat: test"))
            .create_async()
            .await;

        let provider = ClaudeProvider::with_base_url("sk-test-key".into(), server.url());
        let _ = provider.complete("system", "user msg", &test_opts()).await;
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_claude_sends_system_and_user_message() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("POST", "/v1/messages")
            .match_body(mockito::Matcher::PartialJsonString(
                serde_json::json!({
                    "system": "my system prompt",
                    "messages": [{"role": "user", "content": "my diff"}]
                })
                .to_string(),
            ))
            .with_status(200)
            .with_body(success_response("feat: test"))
            .create_async()
            .await;

        let provider = ClaudeProvider::with_base_url("key".into(), server.url());
        let _ = provider.complete("my system prompt", "my diff", &test_opts()).await;
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_claude_parses_response_text_correctly() {
        let mut server = Server::new_async().await;
        let _mock = setup_mock(&mut server, 200, &success_response("feat: add authentication")).await;

        let provider = ClaudeProvider::with_base_url("key".into(), server.url());
        let result = provider.complete("sys", "diff", &test_opts()).await.unwrap();
        assert_eq!(result, "feat: add authentication");
    }

    #[tokio::test]
    async fn test_claude_401_returns_invalid_key_error() {
        let mut server = Server::new_async().await;
        let _mock = setup_mock(
            &mut server,
            401,
            &serde_json::json!({"error": {"message": "invalid x-api-key"}}).to_string(),
        )
        .await;

        let provider = ClaudeProvider::with_base_url("bad-key".into(), server.url());
        let err = provider.complete("sys", "diff", &test_opts()).await.unwrap_err();
        assert!(err.to_string().to_lowercase().contains("invalid api key"));
    }

    #[tokio::test]
    async fn test_claude_429_returns_rate_limit_error() {
        let mut server = Server::new_async().await;
        let _mock = setup_mock(
            &mut server,
            429,
            &serde_json::json!({"error": {"message": "rate limited"}}).to_string(),
        )
        .await;

        let provider = ClaudeProvider::with_base_url("key".into(), server.url());
        let err = provider.complete("sys", "diff", &test_opts()).await.unwrap_err();
        assert!(err.to_string().to_lowercase().contains("rate limit"));
    }

    #[tokio::test]
    async fn test_claude_malformed_response_returns_parse_error() {
        let mut server = Server::new_async().await;
        let _mock = setup_mock(&mut server, 200, r#"{"content": []}"#).await;

        let provider = ClaudeProvider::with_base_url("key".into(), server.url());
        let err = provider.complete("sys", "diff", &test_opts()).await.unwrap_err();
        assert!(err.to_string().contains("no text"));
    }
}
