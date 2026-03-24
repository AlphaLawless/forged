use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

use forged::git::{CommitResult, commit};
use serial_test::serial;
use tempfile::TempDir;

/// Create a temp git repo with an initial commit and a staged change ready to commit.
fn setup_repo_with_staged_change() -> TempDir {
    let dir = TempDir::new().unwrap();
    let p = dir.path();

    // Init repo with initial commit
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

    // Stage a new change
    fs::write(p.join("change.txt"), "hello").unwrap();
    Command::new("git")
        .args(["add", "."])
        .current_dir(p)
        .output()
        .unwrap();

    dir
}

/// Helper to run git log in the repo and return the output.
fn git_log(dir: &TempDir, format: &str) -> String {
    let output = Command::new("git")
        .args(["log", "-1", &format!("--format={format}")])
        .current_dir(dir.path())
        .output()
        .unwrap();
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

#[test]
#[serial]
fn commit_single_line_message() {
    let dir = setup_repo_with_staged_change();
    std::env::set_current_dir(dir.path()).unwrap();

    let result = commit("feat: add login", false, &[]).unwrap();
    assert!(matches!(result, CommitResult::Success));

    let subject = git_log(&dir, "%s");
    assert_eq!(subject, "feat: add login");
}

#[test]
#[serial]
fn commit_subject_body_message() {
    let dir = setup_repo_with_staged_change();
    std::env::set_current_dir(dir.path()).unwrap();

    let msg = "feat: add auth\n\n- Add OAuth2 provider\n- Add token refresh";
    let result = commit(msg, false, &[]).unwrap();
    assert!(matches!(result, CommitResult::Success));

    let subject = git_log(&dir, "%s");
    assert_eq!(subject, "feat: add auth");

    let body = git_log(&dir, "%b");
    assert!(body.contains("Add OAuth2 provider"));
    assert!(body.contains("Add token refresh"));
}

#[test]
#[serial]
fn commit_hook_failure() {
    let dir = setup_repo_with_staged_change();
    std::env::set_current_dir(dir.path()).unwrap();

    // Install a pre-commit hook that always fails
    let hooks_dir = dir.path().join(".git/hooks");
    fs::create_dir_all(&hooks_dir).unwrap();
    let hook_path = hooks_dir.join("pre-commit");
    fs::write(&hook_path, "#!/bin/sh\nexit 1\n").unwrap();
    fs::set_permissions(&hook_path, fs::Permissions::from_mode(0o755)).unwrap();

    let result = commit("feat: should fail", false, &[]).unwrap();
    assert!(matches!(result, CommitResult::HookFailed));
}

#[test]
#[serial]
fn commit_no_verify_bypasses_hook() {
    let dir = setup_repo_with_staged_change();
    std::env::set_current_dir(dir.path()).unwrap();

    // Install a pre-commit hook that always fails
    let hooks_dir = dir.path().join(".git/hooks");
    fs::create_dir_all(&hooks_dir).unwrap();
    let hook_path = hooks_dir.join("pre-commit");
    fs::write(&hook_path, "#!/bin/sh\nexit 1\n").unwrap();
    fs::set_permissions(&hook_path, fs::Permissions::from_mode(0o755)).unwrap();

    let result = commit("feat: bypass hook", true, &[]).unwrap();
    assert!(matches!(result, CommitResult::Success));

    let subject = git_log(&dir, "%s");
    assert_eq!(subject, "feat: bypass hook");
}

#[test]
#[serial]
fn commit_extra_args_author() {
    let dir = setup_repo_with_staged_change();
    std::env::set_current_dir(dir.path()).unwrap();

    let extra = vec!["--author=Custom Author <custom@test.com>".to_string()];
    let result = commit("feat: custom author", false, &extra).unwrap();
    assert!(matches!(result, CommitResult::Success));

    let author = git_log(&dir, "%an <%ae>");
    assert_eq!(author, "Custom Author <custom@test.com>");
}
