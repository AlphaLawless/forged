use anyhow::{Context, Result, bail};
use colored::Colorize;
use std::path::PathBuf;

use crate::config::{self, CommitType, Config, ProviderEntry};
use crate::tui::widgets::{select::SelectItem, text_input};

#[derive(Debug, Clone)]
pub struct ProviderInfo {
    pub key: &'static str,
    pub label: &'static str,
    pub models: &'static [&'static str],
}

const PROVIDER_LIST: &[ProviderInfo] = &[
    ProviderInfo {
        key: "claude",
        label: "Claude (Anthropic)",
        models: &[
            "claude-sonnet-4-6-20250514",
            "claude-haiku-4-5-20251001",
            "claude-opus-4-6-20250603",
        ],
    },
    ProviderInfo {
        key: "gemini",
        label: "Gemini (Google)",
        models: &["gemini-2.5-flash", "gemini-2.5-pro", "gemini-2.0-flash"],
    },
    ProviderInfo {
        key: "chatgpt",
        label: "ChatGPT (OpenAI)",
        models: &["gpt-4o", "gpt-4o-mini", "o3-mini"],
    },
    ProviderInfo {
        key: "openrouter",
        label: "OpenRouter",
        models: &[
            "anthropic/claude-sonnet-4-6",
            "google/gemini-2.5-flash",
            "openai/gpt-4o",
        ],
    },
];

const COMMIT_TYPES: &[(&str, &str)] = &[
    (
        "conventional",
        "conventional  — feat: / fix: / refactor: ...",
    ),
    ("plain", "plain         — free-form message"),
    ("gitmoji", "gitmoji       — :emoji: message"),
    (
        "subject+body",
        "subject+body  — title + detailed description",
    ),
];

/// Return the list of available provider keys.
pub fn available_providers() -> Vec<&'static str> {
    PROVIDER_LIST.iter().map(|p| p.key).collect()
}

/// Return the list of available provider labels (for display).
pub fn available_provider_labels() -> Vec<&'static str> {
    PROVIDER_LIST.iter().map(|p| p.label).collect()
}

/// Find a provider by key.
pub fn find_provider(key: &str) -> Option<&'static ProviderInfo> {
    PROVIDER_LIST.iter().find(|p| p.key == key)
}

/// Find a provider by its display label.
fn find_provider_by_label(label: &str) -> Option<&'static ProviderInfo> {
    PROVIDER_LIST.iter().find(|p| p.label == label)
}

/// Check if setup is needed (no provider or no api_key configured).
pub fn needs_setup(config: &Config) -> bool {
    config.provider.is_empty() || config.api_key.is_empty()
}

fn exit_cancelled() -> ! {
    use colored::Colorize;
    println!("{}", "Cancelled.".dimmed());
    std::process::exit(0);
}

fn pick_provider(current_key: &str) -> Result<&'static ProviderInfo> {
    let starting_idx = PROVIDER_LIST
        .iter()
        .position(|p| p.key == current_key)
        .unwrap_or(0);
    let items: Vec<SelectItem<&str>> = PROVIDER_LIST
        .iter()
        .map(|p| SelectItem::new(p.label, p.key))
        .collect();
    let key = crate::tui::widgets::select::run("Choose your AI provider", items, starting_idx)?
        .unwrap_or_else(|| exit_cancelled());
    find_provider(key).ok_or_else(|| anyhow::anyhow!("Unknown provider: {key}"))
}

fn pick_api_key(label: &str, current: &str) -> Result<String> {
    let hint = if current.is_empty() {
        "stored in ~/.forged/global".to_string()
    } else {
        format!(
            "stored in ~/.forged/global (current: {}...)",
            &current[..current.len().min(8)]
        )
    };
    let entered = text_input::run_masked(label, &hint)?.unwrap_or_else(|| exit_cancelled());
    Ok(if entered.is_empty() {
        current.to_string()
    } else {
        entered
    })
}

fn pick_model(provider_info: &ProviderInfo, current_model: &str) -> Result<String> {
    if provider_info.models.is_empty() {
        return Ok(String::new());
    }
    let default_idx = provider_info
        .models
        .iter()
        .position(|m| *m == current_model)
        .unwrap_or(0);
    let items: Vec<SelectItem<&str>> = provider_info
        .models
        .iter()
        .map(|m| SelectItem::new(*m, *m))
        .collect();
    let model = crate::tui::widgets::select::run("Choose a model", items, default_idx)?
        .unwrap_or_else(|| exit_cancelled());
    Ok(model.to_string())
}

fn pick_commit_type(current: &CommitType) -> Result<CommitType> {
    let current_idx = COMMIT_TYPES
        .iter()
        .position(|(name, _)| *name == current.as_str())
        .unwrap_or(0);
    let items: Vec<SelectItem<&str>> = COMMIT_TYPES
        .iter()
        .map(|(key, label)| SelectItem::new(*label, *key))
        .collect();
    let key = crate::tui::widgets::select::run("Commit message format", items, current_idx)?
        .unwrap_or_else(|| exit_cancelled());
    CommitType::from_str_loose(key)
}

fn pick_locale(current: &str) -> Result<String> {
    let value = text_input::run("Message language", current, "e.g. en, pt-br, ja, es")?
        .unwrap_or_else(|| exit_cancelled());
    Ok(if value.is_empty() {
        current.to_string()
    } else {
        value
    })
}

/// Run the interactive first-time setup wizard.
pub fn run(existing: Option<Config>) -> Result<Config> {
    println!();
    println!("{}", "  Welcome to forged! ".bold().on_cyan().black());
    println!("{}", "  Let's set up your AI provider.\n".dimmed());

    let mut config = existing.unwrap_or_default();

    let provider_info = pick_provider(&config.provider)?;
    config.provider = provider_info.key.into();

    config.api_key = pick_api_key("Enter your API key", &config.api_key)?;

    config.model = pick_model(provider_info, &config.model)?;

    config.commit_type = pick_commit_type(&config.commit_type)?;

    config.locale = pick_locale(&config.locale)?;

    config.fallback_providers = collect_fallback_providers(&config.provider)?;

    config.save_global()?;

    println!();
    println!("{} Configuration saved to ~/.forged/global", "✔".green());
    println!(
        "{}",
        "  Run `forged config list` to review your settings.\n".dimmed()
    );

    Ok(config)
}

/// Interactive loop to collect up to 3 fallback providers.
fn collect_fallback_providers(primary_key: &str) -> Result<Vec<ProviderEntry>> {
    let mut fallbacks = Vec::new();
    let max_fallbacks = 3;

    loop {
        if fallbacks.len() >= max_fallbacks {
            break;
        }

        let confirm_items = vec![SelectItem::new("No", false), SelectItem::new("Yes", true)];
        let add = crate::tui::widgets::select::run("Add a fallback provider?", confirm_items, 0)?
            .unwrap_or(false);

        if !add {
            break;
        }

        let used: Vec<&str> = std::iter::once(primary_key)
            .chain(fallbacks.iter().map(|f: &ProviderEntry| f.name.as_str()))
            .collect();
        let remaining: Vec<SelectItem<&str>> = PROVIDER_LIST
            .iter()
            .filter(|p| !used.contains(&p.key))
            .map(|p| SelectItem::new(p.label, p.key))
            .collect();

        if remaining.is_empty() {
            println!("{}", "  All providers already configured.".dimmed());
            break;
        }

        let key = crate::tui::widgets::select::run("Fallback provider", remaining, 0)?
            .unwrap_or_else(|| exit_cancelled());
        let info = find_provider(key).ok_or_else(|| anyhow::anyhow!("Unknown provider"))?;

        let api_key = text_input::run_masked(
            &format!("API key for {}", info.label),
            "stored in ~/.forged/global",
        )?
        .unwrap_or_else(|| exit_cancelled());

        let model = pick_model(info, "")?;

        fallbacks.push(ProviderEntry {
            name: info.key.into(),
            api_key,
            model,
        });
    }

    Ok(fallbacks)
}

/// Run the interactive local setup wizard (per-repo overrides).
pub fn run_local() -> Result<Config> {
    let repo_root = crate::git::assert_git_repo()?;
    let profile = crate::git::repo_name(&repo_root)
        .context("Could not determine repo directory name")?
        .to_string();

    println!();
    println!("{}", "  Local config setup ".bold().on_cyan().black());
    println!("{}", format!("  Profile: {profile}\n").dimmed());

    let _global = Config::load_global()?;
    let mut config = Config::load()?;

    let provider_info = pick_provider(&config.provider)?;
    config.provider = provider_info.key.into();

    // API key is optional for local — inherits from global if empty
    let hint = if config.api_key.is_empty() {
        "leave empty to inherit from global".to_string()
    } else {
        format!(
            "current: {}... — leave empty to inherit from global",
            &config.api_key[..config.api_key.len().min(8)]
        )
    };
    let entered =
        text_input::run_masked("Enter your API key", &hint)?.unwrap_or_else(|| exit_cancelled());
    if !entered.is_empty() {
        config.api_key = entered;
    }

    config.model = pick_model(provider_info, &config.model)?;

    config.commit_type = pick_commit_type(&config.commit_type)?;

    config.locale = pick_locale(&config.locale)?;

    config.fallback_providers = collect_fallback_providers(&config.provider)?;

    config.save_local(&profile)?;

    let dot_forged = PathBuf::from(&repo_root).join(".forged");
    std::fs::write(&dot_forged, format!("{profile}\n"))
        .context("Failed to write .forged profile file")?;

    println!();
    println!(
        "{} Local config saved to ~/.forged/locals/{profile}",
        "✔".green()
    );
    println!("{} Profile file created at .forged", "✔".green());
    println!(
        "{}",
        "  Consider adding .forged to your .gitignore\n".dimmed()
    );

    Ok(config)
}

/// Remove the local config for the current repo.
pub fn remove_local() -> Result<()> {
    let repo_root = crate::git::assert_git_repo()?;
    let dot_forged = PathBuf::from(&repo_root).join(".forged");

    if !dot_forged.is_file() {
        println!("{}", "No local config for this repo.".dimmed());
        return Ok(());
    }

    let profile = std::fs::read_to_string(&dot_forged)
        .context("Failed to read .forged profile file")?
        .trim()
        .to_string();

    if profile.is_empty() {
        std::fs::remove_file(&dot_forged).ok();
        println!("{}", "No local config for this repo.".dimmed());
        return Ok(());
    }

    let removed = config::remove_local_profile(&profile)?;

    std::fs::remove_file(&dot_forged).context("Failed to remove .forged file")?;

    println!();
    if removed {
        println!(
            "{} Removed local profile '{profile}' and .forged pointer",
            "✔".green()
        );
    } else {
        println!(
            "{} Removed .forged pointer (profile '{profile}' was already absent)",
            "✔".green()
        );
    }
    println!();

    Ok(())
}

/// List all available local profiles.
pub fn list_profiles() -> Result<()> {
    let profiles = config::list_profiles()?;

    if profiles.is_empty() {
        println!("{}", "No local profiles found.".dimmed());
        return Ok(());
    }

    let locals_dir = config::locals_dir()?;
    println!();
    println!("{}", "  Available profiles ".bold());
    for name in &profiles {
        let path = locals_dir.join(name);
        println!("  {} {}", name.green(), path.display().to_string().dimmed());
    }
    println!();

    Ok(())
}

/// Use an existing profile for the current repo.
pub fn use_profile(name: Option<&str>) -> Result<()> {
    let repo_root = crate::git::assert_git_repo()?;
    let profiles = config::list_profiles()?;

    if profiles.is_empty() {
        bail!("No local profiles available. Run `forged setup local` first.");
    }

    let selected = match name {
        Some(n) => {
            if !config::profile_exists(n)? {
                println!();
                println!(
                    "{} Profile '{}' not found. Available profiles:",
                    "✗".red(),
                    n
                );
                for p in &profiles {
                    println!("  - {}", p.green());
                }
                println!();
                bail!("Profile '{n}' does not exist");
            }
            n.to_string()
        }
        None => {
            let items: Vec<SelectItem<String>> = profiles
                .iter()
                .map(|p| SelectItem::new(p.clone(), p.clone()))
                .collect();
            crate::tui::widgets::select::run("Choose a profile to use", items, 0)?
                .unwrap_or_else(|| exit_cancelled())
        }
    };

    let dot_forged = PathBuf::from(&repo_root).join(".forged");
    std::fs::write(&dot_forged, format!("{selected}\n"))
        .context("Failed to write .forged profile file")?;

    println!();
    println!(
        "{} Now using profile '{selected}' for this repo",
        "✔".green()
    );
    println!(
        "{}",
        "  Consider adding .forged to your .gitignore\n".dimmed()
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_needs_setup_when_no_provider() {
        let config = Config::default();
        assert!(needs_setup(&config));
    }

    #[test]
    fn test_needs_setup_when_no_api_key() {
        let config = Config {
            provider: "claude".into(),
            api_key: "".into(),
            ..Config::default()
        };
        assert!(needs_setup(&config));
    }

    #[test]
    fn test_no_setup_needed_when_configured() {
        let config = Config {
            provider: "claude".into(),
            api_key: "sk-test".into(),
            ..Config::default()
        };
        assert!(!needs_setup(&config));
    }

    #[test]
    fn test_available_providers_contains_all() {
        let providers = available_providers();
        assert!(providers.contains(&"claude"));
        assert!(providers.contains(&"gemini"));
        assert!(providers.contains(&"chatgpt"));
        assert!(providers.contains(&"openrouter"));
    }

    #[test]
    fn test_available_provider_labels_match_count() {
        assert_eq!(
            available_providers().len(),
            available_provider_labels().len()
        );
    }

    #[test]
    fn test_find_provider_claude() {
        let p = find_provider("claude").unwrap();
        assert_eq!(p.key, "claude");
        assert!(!p.models.is_empty());
        assert!(p.models.contains(&"claude-sonnet-4-6-20250514"));
    }

    #[test]
    fn test_find_provider_gemini() {
        let p = find_provider("gemini").unwrap();
        assert_eq!(p.key, "gemini");
        assert!(!p.models.is_empty());
        assert!(p.models.contains(&"gemini-2.5-flash"));
    }

    #[test]
    fn test_find_provider_chatgpt() {
        let p = find_provider("chatgpt").unwrap();
        assert_eq!(p.key, "chatgpt");
        assert!(!p.models.is_empty());
        assert!(p.models.contains(&"gpt-4o"));
    }

    #[test]
    fn test_find_provider_openrouter() {
        let p = find_provider("openrouter").unwrap();
        assert_eq!(p.key, "openrouter");
        assert!(!p.models.is_empty());
        assert!(p.models.contains(&"anthropic/claude-sonnet-4-6"));
    }

    #[test]
    fn test_find_provider_unknown_returns_none() {
        assert!(find_provider("nonexistent").is_none());
    }

    #[test]
    fn test_find_provider_by_label() {
        let p = find_provider_by_label("Claude (Anthropic)").unwrap();
        assert_eq!(p.key, "claude");
        let p = find_provider_by_label("Gemini (Google)").unwrap();
        assert_eq!(p.key, "gemini");
        let p = find_provider_by_label("ChatGPT (OpenAI)").unwrap();
        assert_eq!(p.key, "chatgpt");
        let p = find_provider_by_label("OpenRouter").unwrap();
        assert_eq!(p.key, "openrouter");
    }
}
