use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::ai::provider::{AiProvider, GenerateOpts};

/// Gemini uses the OpenAI-compatible endpoint for simplicity.
const DEFAULT_BASE_URL: &str = "https://generativelanguage.googleapis.com/v1beta/openai";
const DEFAULT_MODEL: &str = "gemini-2.5-flash";

#[derive(Serialize)]
struct Message {
    role: String,
    content: String,
}

#[derive(Serialize)]
struct RequestBody {
    model: String,
    messages: Vec<Message>,
    temperature: f32,
    max_tokens: u32,
}

#[derive(Deserialize)]
struct Choice {
    message: ChoiceMessage,
}

#[derive(Deserialize)]
struct ChoiceMessage {
    content: Option<String>,
}

#[derive(Deserialize)]
struct ApiResponse {
    choices: Vec<Choice>,
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
pub struct GeminiProvider {
    api_key: String,
    base_url: String,
    client: Client,
}

impl GeminiProvider {
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
impl AiProvider for GeminiProvider {
    fn name(&self) -> &str {
        "gemini"
    }

    fn default_model(&self) -> &str {
        DEFAULT_MODEL
    }

    fn default_timeout(&self) -> u64 {
        60 // Gemini thinking models need more time
    }

    async fn complete(&self, system: &str, user: &str, opts: &GenerateOpts) -> Result<String> {
        let url = format!("{}/chat/completions", self.base_url);

        let body = RequestBody {
            model: opts.model.clone(),
            messages: vec![
                Message {
                    role: "system".into(),
                    content: system.to_string(),
                },
                Message {
                    role: "user".into(),
                    content: user.to_string(),
                },
            ],
            temperature: opts.temperature,
            max_tokens: opts.max_tokens,
        };

        let response = self
            .client
            .post(&url)
            .header("authorization", format!("Bearer {}", self.api_key))
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
                    anyhow::anyhow!(
                        "Failed to connect to Gemini API. Are you connected to the internet?"
                    )
                } else {
                    anyhow::anyhow!("HTTP request failed: {e}")
                }
            })?;

        let status = response.status();

        if status == 401 || status == 403 {
            bail!("Invalid API key. Check your Gemini API key and try again.");
        }

        if status == 429 {
            bail!("Rate limit exceeded. Please wait a moment and try again.");
        }

        if !status.is_success() {
            let body_text = response.text().await.unwrap_or_default();
            if let Ok(api_err) = serde_json::from_str::<ApiError>(&body_text)
                && let Some(detail) = api_err.error
                && let Some(msg) = detail.message
            {
                bail!("Gemini API error ({}): {}", status.as_u16(), msg);
            }
            bail!("Gemini API error ({}): {}", status.as_u16(), body_text);
        }

        let api_response: ApiResponse = response
            .json()
            .await
            .context("Failed to parse Gemini API response")?;

        let text = api_response
            .choices
            .into_iter()
            .next()
            .and_then(|c| c.message.content)
            .context("Gemini API response contained no text")?;

        Ok(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockito::{Mock, Server};

    fn test_opts() -> GenerateOpts {
        GenerateOpts {
            model: "gemini-2.5-flash".into(),
            temperature: 0.4,
            max_tokens: 1000,
            completions: 1,
            timeout_secs: 5,
        }
    }

    fn success_response(text: &str) -> String {
        serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": text
                },
                "finish_reason": "stop",
                "index": 0
            }],
            "model": "gemini-2.5-flash"
        })
        .to_string()
    }

    async fn setup_mock(server: &mut Server, status: usize, body: &str) -> Mock {
        server
            .mock("POST", "/chat/completions")
            .with_status(status)
            .with_header("content-type", "application/json")
            .with_body(body)
            .create_async()
            .await
    }

    #[tokio::test]
    async fn test_gemini_sends_bearer_auth_header() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("POST", "/chat/completions")
            .match_header("authorization", "Bearer test-gemini-key")
            .with_status(200)
            .with_body(success_response("feat: test"))
            .create_async()
            .await;

        let provider = GeminiProvider::with_base_url("test-gemini-key".into(), server.url());
        let _ = provider.complete("system", "user msg", &test_opts()).await;
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_gemini_sends_system_as_message_role() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("POST", "/chat/completions")
            .match_body(mockito::Matcher::PartialJsonString(
                serde_json::json!({
                    "messages": [
                        {"role": "system", "content": "my system prompt"},
                        {"role": "user", "content": "my diff"}
                    ]
                })
                .to_string(),
            ))
            .with_status(200)
            .with_body(success_response("feat: test"))
            .create_async()
            .await;

        let provider = GeminiProvider::with_base_url("key".into(), server.url());
        let _ = provider
            .complete("my system prompt", "my diff", &test_opts())
            .await;
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_gemini_parses_choices_response() {
        let mut server = Server::new_async().await;
        let _mock = setup_mock(
            &mut server,
            200,
            &success_response("feat: add authentication"),
        )
        .await;

        let provider = GeminiProvider::with_base_url("key".into(), server.url());
        let result = provider
            .complete("sys", "diff", &test_opts())
            .await
            .unwrap();
        assert_eq!(result, "feat: add authentication");
    }

    #[tokio::test]
    async fn test_gemini_401_returns_invalid_key_error() {
        let mut server = Server::new_async().await;
        let _mock = setup_mock(
            &mut server,
            401,
            &serde_json::json!({"error": {"message": "invalid api key"}}).to_string(),
        )
        .await;

        let provider = GeminiProvider::with_base_url("bad-key".into(), server.url());
        let err = provider
            .complete("sys", "diff", &test_opts())
            .await
            .unwrap_err();
        assert!(err.to_string().to_lowercase().contains("invalid api key"));
    }

    #[tokio::test]
    async fn test_gemini_429_returns_rate_limit_error() {
        let mut server = Server::new_async().await;
        let _mock = setup_mock(
            &mut server,
            429,
            &serde_json::json!({"error": {"message": "rate limited"}}).to_string(),
        )
        .await;

        let provider = GeminiProvider::with_base_url("key".into(), server.url());
        let err = provider
            .complete("sys", "diff", &test_opts())
            .await
            .unwrap_err();
        assert!(err.to_string().to_lowercase().contains("rate limit"));
    }

    #[tokio::test]
    async fn test_gemini_empty_choices_returns_error() {
        let mut server = Server::new_async().await;
        let _mock = setup_mock(&mut server, 200, r#"{"choices": []}"#).await;

        let provider = GeminiProvider::with_base_url("key".into(), server.url());
        let err = provider
            .complete("sys", "diff", &test_opts())
            .await
            .unwrap_err();
        assert!(err.to_string().contains("no text"));
    }
}
