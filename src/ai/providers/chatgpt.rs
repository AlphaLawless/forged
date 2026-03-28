use super::openai_compat::{OpenAiCompatConfig, OpenAiCompatProvider};

const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";

fn chatgpt_config() -> OpenAiCompatConfig {
    OpenAiCompatConfig {
        name: "chatgpt",
        default_model: "gpt-4o",
        default_timeout: 30,
        base_url: DEFAULT_BASE_URL.into(),
        extra_headers: vec![],
        invalid_key_statuses: &[401],
    }
}

pub fn new(api_key: String) -> OpenAiCompatProvider {
    OpenAiCompatProvider::new(api_key, chatgpt_config())
}

/// Create with a custom base URL (used for testing with mockito).
pub fn with_base_url(api_key: String, base_url: String) -> OpenAiCompatProvider {
    OpenAiCompatProvider::with_base_url(api_key, base_url, chatgpt_config())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::provider::{AiProvider, GenerateOpts};
    use mockito::{Mock, Server};

    fn test_opts() -> GenerateOpts {
        GenerateOpts {
            model: "gpt-4o".into(),
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
            "model": "gpt-4o"
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

    #[test]
    fn test_chatgpt_name() {
        let provider = new("key".into());
        assert_eq!(provider.name(), "chatgpt");
    }

    #[test]
    fn test_chatgpt_default_model() {
        let provider = new("key".into());
        assert_eq!(provider.default_model(), "gpt-4o");
    }

    #[test]
    fn test_chatgpt_default_timeout() {
        let provider = new("key".into());
        assert_eq!(provider.default_timeout(), 30);
    }

    #[tokio::test]
    async fn test_chatgpt_sends_bearer_auth() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("POST", "/chat/completions")
            .match_header("authorization", "Bearer sk-openai-test")
            .with_status(200)
            .with_body(success_response("feat: test"))
            .create_async()
            .await;

        let provider = with_base_url("sk-openai-test".into(), server.url());
        let _ = provider.complete("system", "user msg", &test_opts()).await;
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_chatgpt_401_returns_invalid_key_error() {
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
    async fn test_chatgpt_parses_response() {
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
}
