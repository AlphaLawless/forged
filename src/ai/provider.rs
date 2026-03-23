use anyhow::Result;
use async_trait::async_trait;

use super::sanitize;

#[derive(Debug, Clone)]
pub struct GenerateOpts {
    pub model: String,
    pub temperature: f32,
    pub max_tokens: u32,
    pub completions: u8,
    pub timeout_secs: u64,
}

#[async_trait]
pub trait AiProvider: Send + Sync + std::fmt::Debug {
    fn name(&self) -> &str;
    fn default_model(&self) -> &str;

    /// Send a single completion request and return the raw response text.
    async fn complete(
        &self,
        system: &str,
        user: &str,
        opts: &GenerateOpts,
    ) -> Result<String>;
}

/// Generate N messages in parallel, sanitize, and deduplicate.
pub async fn generate_messages(
    provider: &dyn AiProvider,
    system: &str,
    user: &str,
    opts: &GenerateOpts,
) -> Result<Vec<String>> {
    let futures: Vec<_> = (0..opts.completions)
        .map(|_| provider.complete(system, user, opts))
        .collect();

    let results = futures::future::join_all(futures).await;

    let mut messages = Vec::new();
    for result in results {
        match result {
            Ok(text) => messages.push(sanitize::sanitize_title(&text)),
            Err(e) => {
                // If all completions fail, we'll return the last error below.
                // If at least one succeeds, we skip failures.
                if messages.is_empty() && opts.completions == 1 {
                    return Err(e);
                }
            }
        }
    }

    if messages.is_empty() {
        anyhow::bail!("All AI completions failed. Check your API key and network connection.");
    }

    Ok(sanitize::deduplicate(messages))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct MockProvider {
        responses: Vec<String>,
    }

    #[async_trait]
    impl AiProvider for MockProvider {
        fn name(&self) -> &str { "mock" }
        fn default_model(&self) -> &str { "mock-1" }
        async fn complete(&self, _system: &str, _user: &str, _opts: &GenerateOpts) -> Result<String> {
            // Return responses in rotation based on a simple counter
            // Since we can't easily use interior mutability here without Mutex,
            // just return the first response for simplicity
            Ok(self.responses[0].clone())
        }
    }

    #[derive(Debug)]
    struct MultiMockProvider {
        responses: Vec<String>,
    }

    #[async_trait]
    impl AiProvider for MultiMockProvider {
        fn name(&self) -> &str { "multi-mock" }
        fn default_model(&self) -> &str { "mock-1" }
        async fn complete(&self, _system: &str, _user: &str, _opts: &GenerateOpts) -> Result<String> {
            use std::sync::atomic::{AtomicUsize, Ordering};
            static COUNTER: AtomicUsize = AtomicUsize::new(0);
            let idx = COUNTER.fetch_add(1, Ordering::SeqCst) % self.responses.len();
            Ok(self.responses[idx].clone())
        }
    }

    fn test_opts(completions: u8) -> GenerateOpts {
        GenerateOpts {
            model: "test".into(),
            temperature: 0.4,
            max_tokens: 100,
            completions,
            timeout_secs: 10,
        }
    }

    #[tokio::test]
    async fn test_generate_messages_deduplicates_identical_responses() {
        let provider = MockProvider {
            responses: vec!["feat: add login".into()],
        };
        let result = generate_messages(&provider, "sys", "diff", &test_opts(3)).await.unwrap();
        // All 3 completions return the same thing, should deduplicate to 1
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], "feat: add login");
    }

    #[tokio::test]
    async fn test_generate_messages_returns_all_unique() {
        let provider = MultiMockProvider {
            responses: vec!["feat: add login".into(), "feat: add auth".into()],
        };
        let result = generate_messages(&provider, "sys", "diff", &test_opts(2)).await.unwrap();
        assert_eq!(result.len(), 2);
    }

    #[tokio::test]
    async fn test_generate_messages_sanitizes_each_response() {
        let provider = MockProvider {
            responses: vec!["<think>hmm</think>feat: add login.".into()],
        };
        let result = generate_messages(&provider, "sys", "diff", &test_opts(1)).await.unwrap();
        assert_eq!(result[0], "feat: add login");
    }
}
