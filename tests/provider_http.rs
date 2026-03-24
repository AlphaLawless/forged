//! Integration tests for HTTP provider round-trips.
//! Validates full JSON request body serialization and timeout enforcement.

use forged::ai::provider::{AiProvider, GenerateOpts};
use forged::ai::providers::claude::ClaudeProvider;
use forged::ai::providers::gemini::GeminiProvider;
use mockito::Server;

fn claude_opts() -> GenerateOpts {
    GenerateOpts {
        model: "claude-sonnet-4-6-20250514".into(),
        temperature: 0.4,
        max_tokens: 200,
        completions: 1,
        timeout_secs: 5,
    }
}

fn gemini_opts() -> GenerateOpts {
    GenerateOpts {
        model: "gemini-2.5-flash".into(),
        temperature: 0.5,
        max_tokens: 300,
        completions: 1,
        timeout_secs: 5,
    }
}

// ---------------------------------------------------------------------------
// Claude full round-trip
// ---------------------------------------------------------------------------

#[tokio::test]
async fn claude_full_request_response_roundtrip() {
    let mut server = Server::new_async().await;

    // Match the COMPLETE request body, not just partial
    let mock = server
        .mock("POST", "/v1/messages")
        .match_header("x-api-key", "sk-full-test")
        .match_header("anthropic-version", "2023-06-01")
        .match_header("content-type", "application/json")
        .match_body(mockito::Matcher::JsonString(
            serde_json::json!({
                "model": "claude-sonnet-4-6-20250514",
                "max_tokens": 200,
                "system": "Generate a commit message",
                "messages": [{"role": "user", "content": "diff --git a/file.rs"}],
                "temperature": 0.4
            })
            .to_string(),
        ))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            serde_json::json!({
                "id": "msg_123",
                "type": "message",
                "role": "assistant",
                "content": [{"type": "text", "text": "feat: add file processing"}],
                "model": "claude-sonnet-4-6-20250514",
                "stop_reason": "end_turn",
                "usage": {"input_tokens": 50, "output_tokens": 10}
            })
            .to_string(),
        )
        .create_async()
        .await;

    let provider = ClaudeProvider::with_base_url("sk-full-test".into(), server.url());
    let result = provider
        .complete(
            "Generate a commit message",
            "diff --git a/file.rs",
            &claude_opts(),
        )
        .await
        .unwrap();

    assert_eq!(result, "feat: add file processing");
    mock.assert_async().await;
}

// ---------------------------------------------------------------------------
// Gemini full round-trip
// ---------------------------------------------------------------------------

#[tokio::test]
async fn gemini_full_request_response_roundtrip() {
    let mut server = Server::new_async().await;

    let mock = server
        .mock("POST", "/chat/completions")
        .match_header("authorization", "Bearer gem-full-test")
        .match_header("content-type", "application/json")
        .match_body(mockito::Matcher::JsonString(
            serde_json::json!({
                "model": "gemini-2.5-flash",
                "messages": [
                    {"role": "system", "content": "Generate a commit message"},
                    {"role": "user", "content": "diff --git a/file.rs"}
                ],
                "temperature": 0.5,
                "max_tokens": 300
            })
            .to_string(),
        ))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            serde_json::json!({
                "id": "chatcmpl-123",
                "object": "chat.completion",
                "choices": [{
                    "index": 0,
                    "message": {
                        "role": "assistant",
                        "content": "feat: add file processing"
                    },
                    "finish_reason": "stop"
                }],
                "model": "gemini-2.5-flash",
                "usage": {"prompt_tokens": 50, "completion_tokens": 10}
            })
            .to_string(),
        )
        .create_async()
        .await;

    let provider = GeminiProvider::with_base_url("gem-full-test".into(), server.url());
    let result = provider
        .complete(
            "Generate a commit message",
            "diff --git a/file.rs",
            &gemini_opts(),
        )
        .await
        .unwrap();

    assert_eq!(result, "feat: add file processing");
    mock.assert_async().await;
}

// ---------------------------------------------------------------------------
// Timeout enforcement
// Uses a raw TCP listener that accepts but never responds, forcing a true timeout.
// ---------------------------------------------------------------------------

use std::net::TcpListener;

fn start_silent_server() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    // Accept connections in a background thread but never send a response
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let _conn = stream.unwrap();
            // Hold the connection open, never respond
            std::thread::sleep(std::time::Duration::from_secs(30));
        }
    });
    format!("http://127.0.0.1:{port}")
}

#[tokio::test]
async fn claude_timeout_enforced() {
    let base_url = start_silent_server();

    let mut opts = claude_opts();
    opts.timeout_secs = 1;

    let provider = ClaudeProvider::with_base_url("key".into(), base_url);
    let result = provider.complete("sys", "diff", &opts).await;

    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.to_lowercase().contains("timed out") || err.to_lowercase().contains("timeout"),
        "Expected timeout error, got: {err}"
    );
}

#[tokio::test]
async fn gemini_timeout_enforced() {
    let base_url = start_silent_server();

    let mut opts = gemini_opts();
    opts.timeout_secs = 1;

    let provider = GeminiProvider::with_base_url("key".into(), base_url);
    let result = provider.complete("sys", "diff", &opts).await;

    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.to_lowercase().contains("timed out") || err.to_lowercase().contains("timeout"),
        "Expected timeout error, got: {err}"
    );
}
