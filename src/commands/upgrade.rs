use anyhow::{Context, Result, bail};
use colored::Colorize;
use reqwest::Client;
use serde::Deserialize;
use std::process::Command;

const GITHUB_REPO: &str = "AlphaLawless/forged";
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

fn platform_asset_name() -> Result<String> {
    let os = match std::env::consts::OS {
        "linux" => "linux",
        "macos" => "darwin",
        _ => bail!("Unsupported OS: {}", std::env::consts::OS),
    };
    let arch = match std::env::consts::ARCH {
        "x86_64" => "x86_64",
        "aarch64" => "aarch64",
        _ => bail!("Unsupported architecture: {}", std::env::consts::ARCH),
    };
    Ok(format!("forged-{os}-{arch}.tar.gz"))
}

/// Strip "v" prefix from tag_name for comparison.
fn tag_to_version(tag: &str) -> &str {
    tag.strip_prefix('v').unwrap_or(tag)
}

pub async fn run() -> Result<()> {
    println!(":: Checking for updates...");

    let client = Client::new();
    let url = format!("https://api.github.com/repos/{GITHUB_REPO}/releases/latest");

    let release: Release = client
        .get(&url)
        .header("user-agent", "forged")
        .header("accept", "application/vnd.github+json")
        .send()
        .await
        .context("Failed to check for updates. Are you connected to the internet?")?
        .json()
        .await
        .context("Failed to parse GitHub release response")?;

    let latest = tag_to_version(&release.tag_name);

    if latest == CURRENT_VERSION {
        println!(
            "{} forged v{CURRENT_VERSION} is already up to date.",
            "✔".green()
        );
        return Ok(());
    }

    println!(":: New version available: v{CURRENT_VERSION} → v{latest}");

    // Find the right asset for this platform
    let asset_name = platform_asset_name()?;
    let asset = release
        .assets
        .iter()
        .find(|a| a.name == asset_name)
        .with_context(|| format!("No binary available for your platform ({asset_name})"))?;

    println!(":: Downloading {asset_name}...");

    let bytes = client
        .get(&asset.browser_download_url)
        .header("user-agent", "forged")
        .header("accept", "application/octet-stream")
        .send()
        .await
        .context("Failed to download update")?
        .bytes()
        .await
        .context("Failed to read download")?;

    // Extract to temp directory
    let tmp_dir = std::env::temp_dir().join(format!("forged-upgrade-{}", std::process::id()));
    std::fs::create_dir_all(&tmp_dir).context("Failed to create temp directory")?;
    let tar_path = tmp_dir.join(&asset_name);
    std::fs::write(&tar_path, &bytes).context("Failed to write downloaded archive")?;

    let status = Command::new("tar")
        .args([
            "xzf",
            tar_path.to_str().unwrap(),
            "-C",
            tmp_dir.to_str().unwrap(),
        ])
        .status()
        .context("Failed to extract archive. Is 'tar' installed?")?;

    if !status.success() {
        bail!("Failed to extract archive (tar exited with non-zero status)");
    }

    let new_binary = tmp_dir.join("forged");
    if !new_binary.exists() {
        bail!("Extracted archive does not contain 'forged' binary");
    }

    // Replace current binary
    let current_exe =
        std::env::current_exe().context("Failed to determine current executable path")?;

    let backup = current_exe.with_extension("old");

    // Rename current → .old, then move new → current
    // This way if something fails, the .old backup is still there
    if backup.exists() {
        std::fs::remove_file(&backup).ok();
    }
    std::fs::rename(&current_exe, &backup)
        .context("Failed to backup current binary. Do you have write permission?")?;

    if let Err(e) = std::fs::copy(&new_binary, &current_exe) {
        // Restore backup
        std::fs::rename(&backup, &current_exe).ok();
        bail!("Failed to install new binary: {e}");
    }

    // Set permissions
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&current_exe, std::fs::Permissions::from_mode(0o755)).ok();
    }

    // Clean up
    std::fs::remove_file(&backup).ok();
    std::fs::remove_dir_all(&tmp_dir).ok();

    println!(
        "{} Updated forged v{CURRENT_VERSION} → v{latest}",
        "✔".green()
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_platform_asset_name_format() {
        let name = platform_asset_name().unwrap();
        assert!(name.starts_with("forged-"));
        assert!(name.ends_with(".tar.gz"));
        // Should contain a valid os-arch combo
        assert!(
            name.contains("linux-") || name.contains("darwin-"),
            "Unexpected asset name: {name}"
        );
        assert!(
            name.contains("x86_64") || name.contains("aarch64"),
            "Unexpected asset name: {name}"
        );
    }

    #[test]
    fn test_tag_to_version_strips_prefix() {
        assert_eq!(tag_to_version("v0.2.0"), "0.2.0");
        assert_eq!(tag_to_version("v1.0.0"), "1.0.0");
    }

    #[test]
    fn test_tag_to_version_no_prefix() {
        assert_eq!(tag_to_version("0.2.0"), "0.2.0");
    }

    #[test]
    fn test_current_version_is_set() {
        assert!(!CURRENT_VERSION.is_empty());
    }
}
