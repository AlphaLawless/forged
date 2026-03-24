//! Integration tests for git hook install/uninstall.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

use forged::commands::hook;
use serial_test::serial;
use tempfile::TempDir;

fn setup_git_repo() -> TempDir {
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
    dir
}

#[test]
#[serial]
fn hook_install_creates_executable() {
    let dir = setup_git_repo();
    std::env::set_current_dir(dir.path()).unwrap();

    hook::install(false).unwrap();

    let hook_path = dir.path().join(".git/hooks/prepare-commit-msg");
    assert!(hook_path.exists(), "Hook file should exist");

    let metadata = fs::metadata(&hook_path).unwrap();
    let mode = metadata.permissions().mode();
    assert!(
        mode & 0o111 != 0,
        "Hook should be executable, mode: {mode:o}"
    );

    let content = fs::read_to_string(&hook_path).unwrap();
    assert!(content.contains("forged:auto-generated"));
    assert!(content.contains("forged --hook"));
}

#[test]
#[serial]
fn hook_uninstall_removes_file() {
    let dir = setup_git_repo();
    std::env::set_current_dir(dir.path()).unwrap();

    hook::install(false).unwrap();
    let hook_path = dir.path().join(".git/hooks/prepare-commit-msg");
    assert!(hook_path.exists());

    hook::uninstall(false).unwrap();
    assert!(!hook_path.exists(), "Hook file should be removed");
}

#[test]
#[serial]
fn hook_install_refuses_existing_non_forged_hook() {
    let dir = setup_git_repo();
    std::env::set_current_dir(dir.path()).unwrap();

    // Create a manual hook
    let hooks_dir = dir.path().join(".git/hooks");
    fs::create_dir_all(&hooks_dir).unwrap();
    fs::write(
        hooks_dir.join("prepare-commit-msg"),
        "#!/bin/sh\necho custom\n",
    )
    .unwrap();

    let result = hook::install(false);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("already exists"), "Error: {err}");
}

#[test]
#[serial]
fn hook_install_force_overwrites_non_forged_hook() {
    let dir = setup_git_repo();
    std::env::set_current_dir(dir.path()).unwrap();

    // Create a manual hook
    let hooks_dir = dir.path().join(".git/hooks");
    fs::create_dir_all(&hooks_dir).unwrap();
    fs::write(
        hooks_dir.join("prepare-commit-msg"),
        "#!/bin/sh\necho custom\n",
    )
    .unwrap();

    // Force install should succeed
    hook::install(true).unwrap();

    let content = fs::read_to_string(hooks_dir.join("prepare-commit-msg")).unwrap();
    assert!(content.contains("forged:auto-generated"));
}

#[test]
#[serial]
fn hook_uninstall_refuses_non_forged_hook() {
    let dir = setup_git_repo();
    std::env::set_current_dir(dir.path()).unwrap();

    let hooks_dir = dir.path().join(".git/hooks");
    fs::create_dir_all(&hooks_dir).unwrap();
    fs::write(
        hooks_dir.join("prepare-commit-msg"),
        "#!/bin/sh\necho custom\n",
    )
    .unwrap();

    let result = hook::uninstall(false);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("not created by forged"), "Error: {err}");
}

#[test]
#[serial]
fn hook_reinstall_updates_existing_forged_hook() {
    let dir = setup_git_repo();
    std::env::set_current_dir(dir.path()).unwrap();

    // Install twice should succeed (update)
    hook::install(false).unwrap();
    hook::install(false).unwrap();

    let hook_path = dir.path().join(".git/hooks/prepare-commit-msg");
    assert!(hook_path.exists());
}
