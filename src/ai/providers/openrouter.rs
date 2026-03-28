use super::openai_compat::{OpenAiCompatConfig, OpenAiCompatProvider};

const DEFAULT_BASE_URL: &str = "https://openrouter.ai/api/v1";

fn openrouter_config() -> OpenAiCompatConfig {
    OpenAiCompatConfig {
        name: "openrouter",
        default_model: "anthropic/claude-sonnet-4-6",
        default_timeout: 60,
        base_url: DEFAULT_BASE_URL.into(),
        extra_headers: vec![(
            "http-referer",
            "https://github.com/SrVariable/forged".into(),
        )],
        invalid_key_statuses: &[401, 403],
    }
}

pub fn new(api_key: String) -> OpenAiCompatProvider {
    OpenAiCompatProvider::new(api_key, openrouter_config())
}

/// Create with a custom base URL (used for testing with mockito).
pub fn with_base_url(api_key: String, base_url: String) -> OpenAiCompatProvider {
    OpenAiCompatProvider::with_base_url(api_key, base_url, openrouter_config())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::provider::{AiProvider, GenerateOpts};
    use mockito::Server;

    fn test_opts() -> GenerateOpts {
        GenerateOpts {
            model: "anthropic/claude-sonnet-4-6".into(),
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
            "model": "anthropic/claude-sonnet-4-6"
        })
        .to_string()
    }

    #[test]
    fn test_openrouter_name() {
        let provider = new("key".into());
        assert_eq!(provider.name(), "openrouter");
    }

    #[test]
    fn test_openrouter_default_model() {
        let provider = new("key".into());
        assert_eq!(provider.default_model(), "anthropic/claude-sonnet-4-6");
    }

    #[test]
    fn test_openrouter_default_timeout() {
        let provider = new("key".into());
        assert_eq!(provider.default_timeout(), 60);
    }

    #[tokio::test]
    async fn test_openrouter_sends_referer_header() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("POST", "/chat/completions")
            .match_header("http-referer", "https://github.com/SrVariable/forged")
            .match_header("authorization", "Bearer sk-or-test")
            .with_status(200)
            .with_body(success_response("feat: test"))
            .create_async()
            .await;

        let provider = with_base_url("sk-or-test".into(), server.url());
        let _ = provider.complete("system", "user msg", &test_opts()).await;
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_openrouter_parses_response() {
        let mut server = Server::new_async().await;
        let _mock = server
            .mock("POST", "/chat/completions")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(success_response("feat: add authentication"))
            .create_async()
            .await;

        let provider = with_base_url("key".into(), server.url());
        let result = provider
            .complete("sys", "diff", &test_opts())
            .await
            .unwrap();
        assert_eq!(result, "feat: add authentication");
    }
}
