use super::openai_compat::{OpenAiCompatConfig, OpenAiCompatProvider};

const DEFAULT_BASE_URL: &str = "https://generativelanguage.googleapis.com/v1beta/openai";

fn gemini_config() -> OpenAiCompatConfig {
    OpenAiCompatConfig {
        name: "gemini",
        default_model: "gemini-2.5-flash",
        default_timeout: 60,
        base_url: DEFAULT_BASE_URL.into(),
        extra_headers: vec![],
        invalid_key_statuses: &[401, 403],
    }
}

pub fn new(api_key: String) -> OpenAiCompatProvider {
    OpenAiCompatProvider::new(api_key, gemini_config())
}

/// Create with a custom base URL (used for testing with mockito).
pub fn with_base_url(api_key: String, base_url: String) -> OpenAiCompatProvider {
    OpenAiCompatProvider::with_base_url(api_key, base_url, gemini_config())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::provider::{AiProvider, GenerateOpts};
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

        let provider = with_base_url("test-gemini-key".into(), server.url());
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

        let provider = with_base_url("key".into(), server.url());
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

        let provider = with_base_url("key".into(), server.url());
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

        let provider = with_base_url("bad-key".into(), server.url());
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

        let provider = with_base_url("key".into(), server.url());
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

        let provider = with_base_url("key".into(), server.url());
        let err = provider
            .complete("sys", "diff", &test_opts())
            .await
            .unwrap_err();
        assert!(err.to_string().contains("no text"));
    }
}
