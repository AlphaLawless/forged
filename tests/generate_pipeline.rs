//! Integration tests for the message generation pipeline.
//! Tests generate_messages + generate_description combined, simulating
//! the subject+body flow without needing git.

use anyhow::Result;
use async_trait::async_trait;

use forged::ai::provider::{AiProvider, GenerateOpts, generate_description, generate_messages};
use forged::config::CommitType;
use forged::prompt;

// ---------------------------------------------------------------------------
// Mock providers
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct SequentialProvider {
    responses: Vec<String>,
    call_index: std::sync::Mutex<usize>,
}

impl SequentialProvider {
    fn new(responses: Vec<&str>) -> Self {
        Self {
            responses: responses.into_iter().map(String::from).collect(),
            call_index: std::sync::Mutex::new(0),
        }
    }
}

#[async_trait]
impl AiProvider for SequentialProvider {
    fn name(&self) -> &str { "seq" }
    fn default_model(&self) -> &str { "seq-1" }
    async fn complete(&self, _system: &str, _user: &str, _opts: &GenerateOpts) -> Result<String> {
        let mut idx = self.call_index.lock().unwrap();
        let resp = self.responses[*idx % self.responses.len()].clone();
        *idx += 1;
        Ok(resp)
    }
}

/// Provider that always returns the same response (for dedup testing).
#[derive(Debug)]
struct ConstProvider {
    response: String,
}

#[async_trait]
impl AiProvider for ConstProvider {
    fn name(&self) -> &str { "const" }
    fn default_model(&self) -> &str { "const-1" }
    async fn complete(&self, _system: &str, _user: &str, _opts: &GenerateOpts) -> Result<String> {
        Ok(self.response.clone())
    }
}

/// Provider that returns an empty body.
#[derive(Debug)]
struct EmptyBodyProvider {
    subject: String,
}

#[async_trait]
impl AiProvider for EmptyBodyProvider {
    fn name(&self) -> &str { "empty" }
    fn default_model(&self) -> &str { "empty-1" }
    async fn complete(&self, _system: &str, user: &str, _opts: &GenerateOpts) -> Result<String> {
        // If user prompt contains "Title:", it's a description call → return empty
        if user.contains("Title:") {
            Ok("".to_string())
        } else {
            Ok(self.subject.clone())
        }
    }
}

fn test_opts(completions: u8) -> GenerateOpts {
    GenerateOpts {
        model: "test-1".into(),
        temperature: 0.7,
        max_tokens: 200,
        completions,
        timeout_secs: 10,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn pipeline_plain_returns_subjects_only() {
    let system = prompt::build_system_prompt("en", 72, &CommitType::Conventional, None);
    let provider = SequentialProvider::new(vec!["feat: add login", "fix: resolve crash"]);

    let messages = generate_messages(&provider, &system, "some diff", &test_opts(2)).await.unwrap();

    // Plain mode: no description generation, just subjects
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0], "feat: add login");
    assert_eq!(messages[1], "fix: resolve crash");
}

#[tokio::test]
async fn pipeline_subject_body_combines_both() {
    let system = prompt::build_system_prompt("en", 72, &CommitType::SubjectBody, None);
    let desc_system = prompt::build_description_prompt("en", 72, None);
    let diff = "diff --git a/app.rs b/app.rs\n+fn main() {}";

    // Subject generation
    let provider = SequentialProvider::new(vec![
        "feat: add main entry point",
        "- Initialize application with main function",
    ]);
    let subjects = generate_messages(&provider, &system, diff, &test_opts(1)).await.unwrap();
    assert_eq!(subjects[0], "feat: add main entry point");

    // Body generation
    let body = generate_description(&provider, &desc_system, &subjects[0], diff, &test_opts(1))
        .await
        .unwrap();
    assert!(body.contains("Initialize application"));

    // Combine (as commit.rs does internally)
    let full = if body.is_empty() {
        subjects[0].clone()
    } else {
        format!("{}\n\n{}", subjects[0], body)
    };

    assert!(full.starts_with("feat: add main entry point\n\n"));
    assert!(full.contains("Initialize application"));
}

#[tokio::test]
async fn pipeline_dedup_subjects() {
    let system = prompt::build_system_prompt("en", 72, &CommitType::Conventional, None);

    // 3 completions that all return the same thing → deduplicated to 1
    let provider = ConstProvider {
        response: "feat: add login".to_string(),
    };

    let messages = generate_messages(&provider, &system, "diff", &test_opts(3)).await.unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0], "feat: add login");
}

#[tokio::test]
async fn pipeline_empty_body_returns_subject_only() {
    let desc_system = prompt::build_description_prompt("en", 72, None);

    let provider = EmptyBodyProvider {
        subject: "feat: add logging".to_string(),
    };

    // Generate subject
    let system = prompt::build_system_prompt("en", 72, &CommitType::SubjectBody, None);
    let subjects = generate_messages(&provider, &system, "diff", &test_opts(1)).await.unwrap();
    assert_eq!(subjects[0], "feat: add logging");

    // Generate body (empty)
    let body = generate_description(&provider, &desc_system, &subjects[0], "diff", &test_opts(1))
        .await
        .unwrap();

    // Empty body → just use subject
    let full = if body.is_empty() {
        subjects[0].clone()
    } else {
        format!("{}\n\n{}", subjects[0], body)
    };

    assert_eq!(full, "feat: add logging");
}
