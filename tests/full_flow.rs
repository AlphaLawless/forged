//! End-to-end integration tests: config → git → mock AI → sanitize → commit.

use std::fs;
use std::process::Command;

use anyhow::Result;
use async_trait::async_trait;
use tempfile::TempDir;

use forged::ai::provider::{AiProvider, GenerateOpts, generate_description, generate_messages};
use forged::config::CommitType;
use forged::git::{CommitResult, commit, staged_diff};
use forged::prompt;
use serial_test::serial;

// ---------------------------------------------------------------------------
// Mock provider
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct MockProvider {
    /// First call returns subject, second call returns body.
    responses: Vec<String>,
    call_index: std::sync::Mutex<usize>,
}

impl MockProvider {
    fn new(responses: Vec<&str>) -> Self {
        Self {
            responses: responses.into_iter().map(String::from).collect(),
            call_index: std::sync::Mutex::new(0),
        }
    }
}

#[async_trait]
impl AiProvider for MockProvider {
    fn name(&self) -> &str {
        "mock"
    }
    fn default_model(&self) -> &str {
        "mock-1"
    }
    async fn complete(&self, _system: &str, _user: &str, _opts: &GenerateOpts) -> Result<String> {
        let mut idx = self.call_index.lock().unwrap();
        let response = self.responses[*idx % self.responses.len()].clone();
        *idx += 1;
        Ok(response)
    }
}

fn test_opts() -> GenerateOpts {
    GenerateOpts {
        model: "mock-1".into(),
        temperature: 0.7,
        max_tokens: 200,
        completions: 1,
        timeout_secs: 10,
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn setup_repo_with_staged_change() -> TempDir {
    let dir = TempDir::new().unwrap();
    let p = dir.path();

    Command::new("git")
        .args(["init"])
        .current_dir(p)
        .output()
        .unwrap();
    Command::new("git")
        .args(["config", "user.email", "test@test.com"])
        .current_dir(p)
        .output()
        .unwrap();
    Command::new("git")
        .args(["config", "user.name", "Test"])
        .current_dir(p)
        .output()
        .unwrap();

    fs::write(p.join("init.txt"), "init").unwrap();
    Command::new("git")
        .args(["add", "."])
        .current_dir(p)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "initial"])
        .current_dir(p)
        .output()
        .unwrap();

    // Stage a real code change
    fs::write(p.join("app.rs"), "fn main() { println!(\"hello\"); }\n").unwrap();
    Command::new("git")
        .args(["add", "."])
        .current_dir(p)
        .output()
        .unwrap();

    dir
}

fn git_log(dir: &TempDir, format: &str) -> String {
    let output = Command::new("git")
        .args(["log", "-1", &format!("--format={format}")])
        .current_dir(dir.path())
        .output()
        .unwrap();
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial]
async fn full_flow_plain_commit() {
    let dir = setup_repo_with_staged_change();
    std::env::set_current_dir(dir.path()).unwrap();

    // 1. Get staged diff (real git)
    let staged = staged_diff(&[])
        .unwrap()
        .expect("should have staged changes");
    let diff = &staged.diff;
    assert!(!diff.is_empty());

    // 2. Build prompt (real)
    let system = prompt::build_system_prompt("en", 72, &CommitType::Conventional, None);

    // 3. Generate message (mock AI)
    let provider = MockProvider::new(vec!["feat: add hello world app"]);
    let messages = generate_messages(&provider, &system, diff, &test_opts())
        .await
        .unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0], "feat: add hello world app");

    // 4. Commit (real git)
    let result = commit(&messages[0], false, &[]).unwrap();
    assert!(matches!(result, CommitResult::Success));

    // 5. Verify in git log
    let subject = git_log(&dir, "%s");
    assert_eq!(subject, "feat: add hello world app");
}

#[tokio::test]
#[serial]
async fn full_flow_subject_body_commit() {
    let dir = setup_repo_with_staged_change();
    std::env::set_current_dir(dir.path()).unwrap();

    let staged = staged_diff(&[])
        .unwrap()
        .expect("should have staged changes");
    let diff = &staged.diff;

    // Step 1: Generate subject
    let system = prompt::build_system_prompt("en", 72, &CommitType::SubjectBody, None);
    let provider = MockProvider::new(vec![
        "feat: add hello world app",        // subject (call 1)
        "- Add main function with println", // body (call 2)
    ]);
    let subjects = generate_messages(&provider, &system, diff, &test_opts())
        .await
        .unwrap();

    // Step 2: Generate body
    let desc_system = prompt::build_description_prompt("en", 72, None);
    let body = generate_description(&provider, &desc_system, &subjects[0], diff, &test_opts())
        .await
        .unwrap();

    // Step 3: Combine
    let full_message = format!("{}\n\n{}", subjects[0], body);
    assert!(full_message.contains("feat: add hello world app"));
    assert!(full_message.contains("Add main function"));

    // Step 4: Commit
    let result = commit(&full_message, false, &[]).unwrap();
    assert!(matches!(result, CommitResult::Success));

    // Step 5: Verify
    let log_subject = git_log(&dir, "%s");
    assert_eq!(log_subject, "feat: add hello world app");

    let log_body = git_log(&dir, "%b");
    assert!(log_body.contains("Add main function"));
}

#[tokio::test]
#[serial]
async fn full_flow_sanitization_chain() {
    let dir = setup_repo_with_staged_change();
    std::env::set_current_dir(dir.path()).unwrap();

    let staged = staged_diff(&[])
        .unwrap()
        .expect("should have staged changes");
    let diff = &staged.diff;
    let system = prompt::build_system_prompt("en", 72, &CommitType::Conventional, None);

    // Provider returns dirty output with think tags and trailing dot
    let provider = MockProvider::new(vec![
        "<think>hmm let me think</think>feat: add main entry point.",
    ]);
    let messages = generate_messages(&provider, &system, diff, &test_opts())
        .await
        .unwrap();

    // Sanitization should have cleaned it
    assert_eq!(messages[0], "feat: add main entry point");

    // Commit the sanitized message
    let result = commit(&messages[0], false, &[]).unwrap();
    assert!(matches!(result, CommitResult::Success));

    let subject = git_log(&dir, "%s");
    assert_eq!(subject, "feat: add main entry point");
}
