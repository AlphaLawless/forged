use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::ai::provider::{AiProvider, GenerateOpts};

/// Configuration for an OpenAI-compatible provider.
pub struct OpenAiCompatConfig {
    pub name: &'static str,
    pub default_model: &'static str,
    pub default_timeout: u64,
    pub base_url: String,
    pub extra_headers: Vec<(&'static str, String)>,
    pub invalid_key_statuses: &'static [u16],
}

pub struct OpenAiCompatProvider {
    config: OpenAiCompatConfig,
    api_key: String,
    client: Client,
}

impl std::fmt::Debug for OpenAiCompatProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenAiCompatProvider")
            .field("name", &self.config.name)
            .field("base_url", &self.config.base_url)
            .finish()
    }
}

impl OpenAiCompatProvider {
    pub fn new(api_key: String, config: OpenAiCompatConfig) -> Self {
        Self {
            config,
            api_key,
            client: Client::new(),
        }
    }

    /// Create with a custom base URL (used for testing with mockito).
    pub fn with_base_url(
        api_key: String,
        base_url: String,
        mut config: OpenAiCompatConfig,
    ) -> Self {
        config.base_url = base_url;
        Self {
            config,
            api_key,
            client: Client::new(),
        }
    }
}

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

#[async_trait]
impl AiProvider for OpenAiCompatProvider {
    fn name(&self) -> &str {
        self.config.name
    }

    fn default_model(&self) -> &str {
        self.config.default_model
    }

    fn default_timeout(&self) -> u64 {
        self.config.default_timeout
    }

    async fn complete(&self, system: &str, user: &str, opts: &GenerateOpts) -> Result<String> {
        let url = format!("{}/chat/completions", self.config.base_url);

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

        let provider_name = self.config.name;

        let mut request = self
            .client
            .post(&url)
            .header("authorization", format!("Bearer {}", self.api_key))
            .header("content-type", "application/json");

        for (key, value) in &self.config.extra_headers {
            request = request.header(*key, value);
        }

        let response = request
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
                        "Failed to connect to {} API. Are you connected to the internet?",
                        provider_name
                    )
                } else {
                    anyhow::anyhow!("HTTP request failed: {e}")
                }
            })?;

        let status = response.status();

        if !status.is_success() {
            let body_text = response.text().await.unwrap_or_default();
            let api_msg = serde_json::from_str::<ApiError>(&body_text)
                .ok()
                .and_then(|e| e.error)
                .and_then(|d| d.message);

            if self.config.invalid_key_statuses.contains(&status.as_u16()) {
                if let Some(msg) = api_msg {
                    bail!(
                        "Invalid API key for {} ({}): {}",
                        provider_name,
                        status.as_u16(),
                        msg
                    );
                }
                bail!(
                    "Invalid API key. Check your {} API key and try again.",
                    provider_name
                );
            }

            if status == 429 {
                bail!("Rate limit exceeded. Please wait a moment and try again.");
            }

            if let Some(msg) = api_msg {
                bail!("{} API error ({}): {}", provider_name, status.as_u16(), msg);
            }
            bail!(
                "{} API error ({}): {}",
                provider_name,
                status.as_u16(),
                body_text
            );
        }

        let api_response: ApiResponse = response
            .json()
            .await
            .context(format!("Failed to parse {} API response", provider_name))?;

        let text = api_response
            .choices
            .into_iter()
            .next()
            .and_then(|c| c.message.content)
            .context(format!("{} API response contained no text", provider_name))?;

        Ok(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockito::{Mock, Server};

    fn test_config() -> OpenAiCompatConfig {
        OpenAiCompatConfig {
            name: "test-provider",
            default_model: "test-model",
            default_timeout: 30,
            base_url: String::new(), // overridden by with_base_url
            extra_headers: vec![],
            invalid_key_statuses: &[401],
        }
    }

    fn test_opts() -> GenerateOpts {
        GenerateOpts {
            model: "test-model".into(),
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
            "model": "test-model"
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
    async fn test_openai_compat_parses_response() {
        let mut server = Server::new_async().await;
        let _mock = setup_mock(&mut server, 200, &success_response("feat: add auth")).await;

        let provider =
            OpenAiCompatProvider::with_base_url("key".into(), server.url(), test_config());
        let result = provider
            .complete("sys", "diff", &test_opts())
            .await
            .unwrap();
        assert_eq!(result, "feat: add auth");
    }

    #[tokio::test]
    async fn test_openai_compat_429_rate_limit() {
        let mut server = Server::new_async().await;
        let _mock = setup_mock(
            &mut server,
            429,
            &serde_json::json!({"error": {"message": "rate limited"}}).to_string(),
        )
        .await;

        let provider =
            OpenAiCompatProvider::with_base_url("key".into(), server.url(), test_config());
        let err = provider
            .complete("sys", "diff", &test_opts())
            .await
            .unwrap_err();
        assert!(err.to_string().to_lowercase().contains("rate limit"));
    }

    #[tokio::test]
    async fn test_openai_compat_api_error_with_message() {
        let mut server = Server::new_async().await;
        let _mock = setup_mock(
            &mut server,
            500,
            &serde_json::json!({"error": {"message": "internal server error"}}).to_string(),
        )
        .await;

        let provider =
            OpenAiCompatProvider::with_base_url("key".into(), server.url(), test_config());
        let err = provider
            .complete("sys", "diff", &test_opts())
            .await
            .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("test-provider API error"));
        assert!(msg.contains("internal server error"));
    }

    #[tokio::test]
    async fn test_openai_compat_empty_choices() {
        let mut server = Server::new_async().await;
        let _mock = setup_mock(&mut server, 200, r#"{"choices": []}"#).await;

        let provider =
            OpenAiCompatProvider::with_base_url("key".into(), server.url(), test_config());
        let err = provider
            .complete("sys", "diff", &test_opts())
            .await
            .unwrap_err();
        assert!(err.to_string().contains("no text"));
    }

    #[tokio::test]
    async fn test_openai_compat_custom_invalid_key_statuses() {
        let mut server = Server::new_async().await;
        let _mock = setup_mock(&mut server, 403, r#"{"error": {"message": "forbidden"}}"#).await;

        let mut config = test_config();
        config.invalid_key_statuses = &[401, 403];
        let provider = OpenAiCompatProvider::with_base_url("key".into(), server.url(), config);
        let err = provider
            .complete("sys", "diff", &test_opts())
            .await
            .unwrap_err();
        assert!(err.to_string().to_lowercase().contains("invalid api key"));
    }

    #[tokio::test]
    async fn test_openai_compat_401_with_default_statuses() {
        let mut server = Server::new_async().await;
        let _mock = setup_mock(
            &mut server,
            401,
            &serde_json::json!({"error": {"message": "unauthorized"}}).to_string(),
        )
        .await;

        let provider =
            OpenAiCompatProvider::with_base_url("bad-key".into(), server.url(), test_config());
        let err = provider
            .complete("sys", "diff", &test_opts())
            .await
            .unwrap_err();
        assert!(err.to_string().to_lowercase().contains("invalid api key"));
    }

    #[tokio::test]
    async fn test_openai_compat_extra_headers_sent() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("POST", "/chat/completions")
            .match_header("http-referer", "https://example.com")
            .match_header("x-custom", "custom-value")
            .with_status(200)
            .with_body(success_response("feat: test"))
            .create_async()
            .await;

        let mut config = test_config();
        config.extra_headers = vec![
            ("http-referer", "https://example.com".into()),
            ("x-custom", "custom-value".into()),
        ];
        let provider = OpenAiCompatProvider::with_base_url("key".into(), server.url(), config);
        let _ = provider.complete("sys", "diff", &test_opts()).await;
        mock.assert_async().await;
    }
}
