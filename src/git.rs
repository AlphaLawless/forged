use anyhow::{Context, Result, bail};
use std::path::Path;
use std::process::Command;

pub const MAX_DIFF_LENGTH: usize = 30_000;

const LOCK_FILE_PATTERNS: &[&str] = &[
    "package-lock.json",
    "pnpm-lock.yaml",
];

const LOCK_FILE_EXTENSION: &str = ".lock";

#[derive(Debug, Clone)]
pub struct StagedDiff {
    pub files: Vec<String>,
    pub diff: String,
}

fn is_lock_file(path: &str) -> bool {
    if path.ends_with(LOCK_FILE_EXTENSION) {
        return true;
    }
    let basename = path.rsplit('/').next().unwrap_or(path);
    LOCK_FILE_PATTERNS.contains(&basename)
}

fn run_git(args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .args(args)
        .output()
        .context("Failed to execute git command")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git {} failed: {}", args.join(" "), stderr.trim());
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Run git with -C <dir> to target a specific repo path.
fn run_git_in(dir: &Path, args: &[&str]) -> Result<String> {
    let dir_str = dir.to_str().context("Invalid directory path")?;
    let mut full_args = vec!["-C", dir_str];
    full_args.extend_from_slice(args);
    run_git(&full_args)
}

/// Check that the current directory is inside a git repository.
pub fn assert_git_repo() -> Result<String> {
    run_git(&["rev-parse", "--show-toplevel"])
        .context("The current directory must be a Git repository")
}

/// Get the staged diff, excluding lock files when there are also non-lock files.
pub fn staged_diff(exclude_files: &[String]) -> Result<Option<StagedDiff>> {
    staged_diff_impl(None, exclude_files)
}

fn staged_diff_impl(dir: Option<&Path>, exclude_files: &[String]) -> Result<Option<StagedDiff>> {
    let git = |args: &[&str]| -> Result<String> {
        match dir {
            Some(d) => run_git_in(d, args),
            None => run_git(args),
        }
    };

    let mut name_args: Vec<&str> = vec!["diff", "--cached", "--diff-algorithm=minimal", "--name-only"];
    let exclude_pathspecs: Vec<String> = exclude_files.iter().map(|f| format!(":(exclude){f}")).collect();
    let exclude_refs: Vec<&str> = exclude_pathspecs.iter().map(|s| s.as_str()).collect();
    name_args.extend(&exclude_refs);

    let files_output = git(&name_args)?;
    if files_output.is_empty() {
        return Ok(None);
    }

    let all_files: Vec<String> = files_output.lines().filter(|l| !l.is_empty()).map(String::from).collect();
    let has_non_lock = all_files.iter().any(|f| !is_lock_file(f));

    let mut lock_excludes: Vec<String> = Vec::new();
    if has_non_lock {
        for pat in LOCK_FILE_PATTERNS {
            lock_excludes.push(format!(":(exclude){pat}"));
        }
        lock_excludes.push(format!(":(exclude)*{LOCK_FILE_EXTENSION}"));
    }

    let all_excludes: Vec<String> = lock_excludes.iter().chain(exclude_pathspecs.iter()).cloned().collect();
    let all_exclude_refs: Vec<&str> = all_excludes.iter().map(|s| s.as_str()).collect();

    let mut file_args: Vec<&str> = vec!["diff", "--cached", "--diff-algorithm=minimal", "--name-only"];
    file_args.extend(&all_exclude_refs);
    let filtered_output = git(&file_args)?;
    if filtered_output.is_empty() {
        return Ok(None);
    }
    let files: Vec<String> = filtered_output.lines().filter(|l| !l.is_empty()).map(String::from).collect();

    let mut diff_args: Vec<&str> = vec!["diff", "--cached", "--diff-algorithm=minimal"];
    diff_args.extend(&all_exclude_refs);
    let diff = git(&diff_args)?;

    Ok(Some(StagedDiff { files, diff }))
}

/// Truncate diff to MAX_DIFF_LENGTH if needed.
pub fn truncate_diff(diff: &str) -> String {
    if diff.len() <= MAX_DIFF_LENGTH {
        diff.to_string()
    } else {
        format!("{}\n\n[Diff truncated due to size]", &diff[..MAX_DIFF_LENGTH])
    }
}

/// Commit with the given message.
pub fn commit(message: &str, no_verify: bool, extra_args: &[String]) -> Result<()> {
    let mut args: Vec<&str> = vec!["commit"];

    if let Some((subject, body)) = message.split_once("\n\n") {
        args.extend(&["-m", subject, "-m", body]);
    } else {
        args.extend(&["-m", message]);
    }

    if no_verify {
        args.push("--no-verify");
    }

    let extra_refs: Vec<&str> = extra_args.iter().map(|s| s.as_str()).collect();
    args.extend(&extra_refs);

    let output = Command::new("git")
        .args(&args)
        .status()
        .context("Failed to run git commit")?;

    if !output.success() {
        bail!("git commit failed");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use tempfile::TempDir;

    fn setup_git_repo() -> TempDir {
        let dir = TempDir::new().unwrap();
        let path = dir.path();

        Command::new("git").args(["init"]).current_dir(path).output().unwrap();
        Command::new("git").args(["config", "user.email", "test@test.com"]).current_dir(path).output().unwrap();
        Command::new("git").args(["config", "user.name", "Test"]).current_dir(path).output().unwrap();

        dir
    }

    #[test]
    fn test_lock_file_detection_matches_patterns() {
        assert!(is_lock_file("package-lock.json"));
        assert!(is_lock_file("pnpm-lock.yaml"));
        assert!(is_lock_file("Cargo.lock"));
        assert!(is_lock_file("yarn.lock"));
        assert!(is_lock_file("sub/dir/Gemfile.lock"));
        assert!(!is_lock_file("src/main.rs"));
        assert!(!is_lock_file("README.md"));
    }

    #[test]
    fn test_staged_diff_returns_none_when_nothing_staged() {
        let dir = setup_git_repo();
        let result = staged_diff_impl(Some(dir.path()), &[]).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_truncate_diff_short() {
        let diff = "short diff";
        assert_eq!(truncate_diff(diff), "short diff");
    }

    #[test]
    fn test_truncate_diff_at_limit() {
        let diff = "a".repeat(MAX_DIFF_LENGTH + 100);
        let truncated = truncate_diff(&diff);
        assert!(truncated.contains("[Diff truncated due to size]"));
        assert!(truncated.len() < diff.len());
    }

    #[test]
    fn test_staged_diff_with_files() {
        let dir = setup_git_repo();
        let path = dir.path();

        std::fs::write(path.join("test.txt"), "hello").unwrap();
        Command::new("git").args(["add", "test.txt"]).current_dir(path).output().unwrap();

        let result = staged_diff_impl(Some(path), &[]).unwrap();
        assert!(result.is_some());
        let staged = result.unwrap();
        assert!(staged.files.contains(&"test.txt".to_string()));
        assert!(staged.diff.contains("hello"));
    }

    #[test]
    fn test_lock_file_not_excluded_when_only_locks_staged() {
        let dir = setup_git_repo();
        let path = dir.path();

        // Stage only a lock file
        std::fs::write(path.join("Cargo.lock"), "lock content").unwrap();
        Command::new("git").args(["add", "Cargo.lock"]).current_dir(path).output().unwrap();

        let result = staged_diff_impl(Some(path), &[]).unwrap();
        assert!(result.is_some());
        let staged = result.unwrap();
        assert!(staged.files.contains(&"Cargo.lock".to_string()));
    }
}
