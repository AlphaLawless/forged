use async_trait::async_trait;

use super::FailoverFailure;
use super::FailoverReport;
use super::ProviderWithOpts;
use super::sanitize;

/// Error types for AI provider operations, classified for failover decisions.
#[derive(Debug)]
pub enum AiError {
    /// Transient failure: 429 rate limit, timeout, 5xx, connection error.
    /// Failover should try the next provider.
    Retryable(String),
    /// This provider is broken (401/403 invalid key) but others may work.
    /// Failover should try the next provider.
    ProviderFatal(String),
    /// Fundamental failure (malformed config, empty response after parse).
    /// No point trying other providers.
    Fatal(String),
}

impl std::fmt::Display for AiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AiError::Retryable(msg) => write!(f, "{msg}"),
            AiError::ProviderFatal(msg) => write!(f, "{msg}"),
            AiError::Fatal(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for AiError {}

impl AiError {
    /// Returns true if failover to the next provider should be attempted.
    pub fn should_failover(&self) -> bool {
        matches!(self, AiError::Retryable(_) | AiError::ProviderFatal(_))
    }
}

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

    /// Default timeout in seconds for this provider.
    fn default_timeout(&self) -> u64 {
        30
    }

    /// Send a single completion request and return the raw response text.
    async fn complete(
        &self,
        system: &str,
        user: &str,
        opts: &GenerateOpts,
    ) -> Result<String, AiError>;
}

/// Generate a commit body (description) for a given subject line.
/// Makes a single AI call with the subject + diff as context, then sanitizes.
pub async fn generate_description(
    provider: &dyn AiProvider,
    system: &str,
    subject: &str,
    diff: &str,
    opts: &GenerateOpts,
) -> Result<String, AiError> {
    let user_prompt = format!("Title: {subject}\n\nDiff:\n{diff}");
    let raw = provider.complete(system, &user_prompt, opts).await?;
    Ok(sanitize::sanitize_description(&raw))
}

/// Generate N messages in parallel, sanitize, and deduplicate.
pub async fn generate_messages(
    provider: &dyn AiProvider,
    system: &str,
    user: &str,
    opts: &GenerateOpts,
) -> Result<Vec<String>, AiError> {
    let futures: Vec<_> = (0..opts.completions)
        .map(|_| provider.complete(system, user, opts))
        .collect();

    let results = futures::future::join_all(futures).await;

    let mut messages = Vec::new();
    let mut last_error: Option<AiError> = None;
    for result in results {
        match result {
            Ok(text) => messages.push(sanitize::sanitize_title(&text)),
            Err(e) => {
                // If it's Fatal, propagate immediately
                if matches!(e, AiError::Fatal(_)) {
                    return Err(e);
                }
                last_error = Some(e);
            }
        }
    }

    if messages.is_empty() {
        return Err(last_error.unwrap_or_else(|| {
            AiError::Fatal(
                "All AI completions failed. Check your API key and network connection.".into(),
            )
        }));
    }

    Ok(sanitize::deduplicate(messages))
}

/// Generate messages with failover across multiple providers.
/// Tries providers in order; on Retryable/ProviderFatal, moves to next.
/// Fatal errors stop immediately.
pub async fn generate_messages_with_failover(
    providers: &[ProviderWithOpts],
    system: &str,
    user: &str,
    base_opts: &GenerateOpts,
) -> Result<(Vec<String>, FailoverReport), AiError> {
    let mut failures = Vec::new();

    for pw in providers {
        let opts = GenerateOpts {
            model: pw.model.clone(),
            timeout_secs: pw.timeout,
            ..base_opts.clone()
        };
        match generate_messages(pw.provider.as_ref(), system, user, &opts).await {
            Ok(messages) => {
                return Ok((
                    messages,
                    FailoverReport {
                        used_provider: pw.provider.name().to_string(),
                        used_model: pw.model.clone(),
                        failures,
                    },
                ));
            }
            Err(e) if e.should_failover() => {
                failures.push(FailoverFailure {
                    provider: pw.provider.name().to_string(),
                    reason: e.to_string(),
                });
            }
            Err(e) => return Err(e),
        }
    }

    Err(AiError::Fatal(format!(
        "All providers failed. {}",
        failures
            .iter()
            .map(|f| format!("{}: {}", f.provider, f.reason))
            .collect::<Vec<_>>()
            .join("; ")
    )))
}

/// Generate a description with failover across multiple providers.
pub async fn generate_description_with_failover(
    providers: &[ProviderWithOpts],
    system: &str,
    subject: &str,
    diff: &str,
    base_opts: &GenerateOpts,
) -> Result<(String, FailoverReport), AiError> {
    let mut failures = Vec::new();

    for pw in providers {
        let opts = GenerateOpts {
            model: pw.model.clone(),
            timeout_secs: pw.timeout,
            ..base_opts.clone()
        };
        match generate_description(pw.provider.as_ref(), system, subject, diff, &opts).await {
            Ok(desc) => {
                return Ok((
                    desc,
                    FailoverReport {
                        used_provider: pw.provider.name().to_string(),
                        used_model: pw.model.clone(),
                        failures,
                    },
                ));
            }
            Err(e) if e.should_failover() => {
                failures.push(FailoverFailure {
                    provider: pw.provider.name().to_string(),
                    reason: e.to_string(),
                });
            }
            Err(e) => return Err(e),
        }
    }

    Err(AiError::Fatal(format!(
        "All providers failed. {}",
        failures
            .iter()
            .map(|f| format!("{}: {}", f.provider, f.reason))
            .collect::<Vec<_>>()
            .join("; ")
    )))
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
        fn name(&self) -> &str {
            "mock"
        }
        fn default_model(&self) -> &str {
            "mock-1"
        }
        async fn complete(
            &self,
            _system: &str,
            _user: &str,
            _opts: &GenerateOpts,
        ) -> Result<String, AiError> {
            Ok(self.responses[0].clone())
        }
    }

    #[derive(Debug)]
    struct MultiMockProvider {
        responses: Vec<String>,
    }

    #[async_trait]
    impl AiProvider for MultiMockProvider {
        fn name(&self) -> &str {
            "multi-mock"
        }
        fn default_model(&self) -> &str {
            "mock-1"
        }
        async fn complete(
            &self,
            _system: &str,
            _user: &str,
            _opts: &GenerateOpts,
        ) -> Result<String, AiError> {
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
        let result = generate_messages(&provider, "sys", "diff", &test_opts(3))
            .await
            .unwrap();
        // All 3 completions return the same thing, should deduplicate to 1
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], "feat: add login");
    }

    #[tokio::test]
    async fn test_generate_messages_returns_all_unique() {
        let provider = MultiMockProvider {
            responses: vec!["feat: add login".into(), "feat: add auth".into()],
        };
        let result = generate_messages(&provider, "sys", "diff", &test_opts(2))
            .await
            .unwrap();
        assert_eq!(result.len(), 2);
    }

    #[tokio::test]
    async fn test_generate_messages_sanitizes_each_response() {
        let provider = MockProvider {
            responses: vec!["<think>hmm</think>feat: add login.".into()],
        };
        let result = generate_messages(&provider, "sys", "diff", &test_opts(1))
            .await
            .unwrap();
        assert_eq!(result[0], "feat: add login");
    }

    #[tokio::test]
    async fn test_generate_description_returns_sanitized_body() {
        let provider = MockProvider {
            responses: vec![
                "<think>ok</think>- Add OAuth2 provider\n- Implement token refresh".into(),
            ],
        };
        let result = generate_description(
            &provider,
            "sys",
            "feat: add auth",
            "diff content",
            &test_opts(1),
        )
        .await
        .unwrap();
        assert!(result.contains("Add OAuth2 provider"));
        assert!(result.contains("Implement token refresh"));
        assert!(!result.contains("<think>"));
    }

    #[derive(Debug)]
    struct CapturingProvider {
        captured_user: std::sync::Mutex<Vec<String>>,
    }

    #[async_trait]
    impl AiProvider for CapturingProvider {
        fn name(&self) -> &str {
            "capture"
        }
        fn default_model(&self) -> &str {
            "capture-1"
        }
        async fn complete(
            &self,
            _system: &str,
            user: &str,
            _opts: &GenerateOpts,
        ) -> Result<String, AiError> {
            self.captured_user.lock().unwrap().push(user.to_string());
            Ok("- Some change".to_string())
        }
    }

    #[tokio::test]
    async fn test_generate_description_includes_subject_in_user_prompt() {
        let provider = CapturingProvider {
            captured_user: std::sync::Mutex::new(Vec::new()),
        };
        generate_description(
            &provider,
            "sys",
            "feat: add login",
            "diff here",
            &test_opts(1),
        )
        .await
        .unwrap();
        let captured = provider.captured_user.lock().unwrap();
        assert_eq!(captured.len(), 1);
        assert!(captured[0].contains("Title: feat: add login"));
        assert!(captured[0].contains("Diff:\ndiff here"));
    }

    // --- Failover tests ---

    #[derive(Debug)]
    struct FailingProvider {
        name: String,
        error: AiError,
    }

    #[async_trait]
    impl AiProvider for FailingProvider {
        fn name(&self) -> &str {
            &self.name
        }
        fn default_model(&self) -> &str {
            "fail-model"
        }
        async fn complete(
            &self,
            _system: &str,
            _user: &str,
            _opts: &GenerateOpts,
        ) -> Result<String, AiError> {
            Err(match &self.error {
                AiError::Retryable(m) => AiError::Retryable(m.clone()),
                AiError::ProviderFatal(m) => AiError::ProviderFatal(m.clone()),
                AiError::Fatal(m) => AiError::Fatal(m.clone()),
            })
        }
    }

    fn make_pw(provider: Box<dyn AiProvider>) -> ProviderWithOpts {
        let model = provider.default_model().to_string();
        ProviderWithOpts {
            provider,
            model,
            timeout: 10,
        }
    }

    #[tokio::test]
    async fn test_failover_to_second_on_retryable() {
        let providers = vec![
            make_pw(Box::new(FailingProvider {
                name: "fail1".into(),
                error: AiError::Retryable("rate limit".into()),
            })),
            make_pw(Box::new(MockProvider {
                responses: vec!["feat: success".into()],
            })),
        ];
        let (msgs, report) = generate_messages_with_failover(
            &providers,
            "sys",
            "diff",
            &test_opts(1),
        )
        .await
        .unwrap();

        assert_eq!(msgs[0], "feat: success");
        assert_eq!(report.used_provider, "mock");
        assert_eq!(report.failures.len(), 1);
        assert_eq!(report.failures[0].provider, "fail1");
    }

    #[tokio::test]
    async fn test_failover_report_tracks_failures() {
        let providers = vec![
            make_pw(Box::new(FailingProvider {
                name: "p1".into(),
                error: AiError::Retryable("timeout".into()),
            })),
            make_pw(Box::new(FailingProvider {
                name: "p2".into(),
                error: AiError::ProviderFatal("invalid key".into()),
            })),
            make_pw(Box::new(MockProvider {
                responses: vec!["fix: it works".into()],
            })),
        ];
        let (_, report) = generate_messages_with_failover(
            &providers,
            "sys",
            "diff",
            &test_opts(1),
        )
        .await
        .unwrap();

        assert_eq!(report.failures.len(), 2);
        assert_eq!(report.failures[0].provider, "p1");
        assert!(report.failures[0].reason.contains("timeout"));
        assert_eq!(report.failures[1].provider, "p2");
        assert!(report.failures[1].reason.contains("invalid key"));
    }

    #[tokio::test]
    async fn test_fatal_stops_failover() {
        let providers = vec![
            make_pw(Box::new(FailingProvider {
                name: "fatal".into(),
                error: AiError::Fatal("parse error".into()),
            })),
            make_pw(Box::new(MockProvider {
                responses: vec!["should not reach".into()],
            })),
        ];
        let err = generate_messages_with_failover(
            &providers,
            "sys",
            "diff",
            &test_opts(1),
        )
        .await
        .unwrap_err();

        assert!(matches!(err, AiError::Fatal(_)));
        assert!(err.to_string().contains("parse error"));
    }

    #[tokio::test]
    async fn test_single_provider_no_failover() {
        let providers = vec![make_pw(Box::new(MockProvider {
            responses: vec!["feat: single".into()],
        }))];
        let (msgs, report) = generate_messages_with_failover(
            &providers,
            "sys",
            "diff",
            &test_opts(1),
        )
        .await
        .unwrap();

        assert_eq!(msgs[0], "feat: single");
        assert!(report.failures.is_empty());
    }

    #[tokio::test]
    async fn test_all_providers_fail() {
        let providers = vec![
            make_pw(Box::new(FailingProvider {
                name: "p1".into(),
                error: AiError::Retryable("timeout".into()),
            })),
            make_pw(Box::new(FailingProvider {
                name: "p2".into(),
                error: AiError::ProviderFatal("bad key".into()),
            })),
        ];
        let err = generate_messages_with_failover(
            &providers,
            "sys",
            "diff",
            &test_opts(1),
        )
        .await
        .unwrap_err();

        assert!(matches!(err, AiError::Fatal(_)));
        assert!(err.to_string().contains("All providers failed"));
        assert!(err.to_string().contains("p1"));
        assert!(err.to_string().contains("p2"));
    }

    #[tokio::test]
    async fn test_description_failover() {
        let providers = vec![
            make_pw(Box::new(FailingProvider {
                name: "fail".into(),
                error: AiError::Retryable("429".into()),
            })),
            make_pw(Box::new(MockProvider {
                responses: vec!["- Change description".into()],
            })),
        ];
        let (desc, report) = generate_description_with_failover(
            &providers,
            "sys",
            "feat: add auth",
            "diff",
            &test_opts(1),
        )
        .await
        .unwrap();

        assert!(desc.contains("Change description"));
        assert_eq!(report.used_provider, "mock");
        assert_eq!(report.failures.len(), 1);
    }
}
