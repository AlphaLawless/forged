use anyhow::Result;
use colored::Colorize;
use inquire::{Password, PasswordDisplayMode, Select, Text};

use crate::config::{CommitType, Config};

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

/// Run the interactive first-time setup wizard.
pub fn run(existing: Option<Config>) -> Result<Config> {
    println!();
    println!("{}", "  Welcome to forged! ".bold().on_cyan().black());
    println!("{}", "  Let's set up your AI provider.\n".dimmed());

    let mut config = existing.unwrap_or_default();

    // 1. Provider selection
    let labels = available_provider_labels();
    let starting_idx = if !config.provider.is_empty() {
        PROVIDER_LIST
            .iter()
            .position(|p| p.key == config.provider)
            .unwrap_or(0)
    } else {
        0
    };
    let selected_label = Select::new("Choose your AI provider:", labels)
        .with_starting_cursor(starting_idx)
        .with_page_size(10)
        .prompt()?;
    let provider_info =
        find_provider_by_label(selected_label).expect("selected label must match a provider");
    config.provider = provider_info.key.into();

    // 2. API Key
    let key_hint = if !config.api_key.is_empty() {
        let visible = &config.api_key[..config.api_key.len().min(8)];
        format!(" (current: {visible}...)")
    } else {
        String::new()
    };
    let api_key = Password::new(&format!(
        "Enter your API key{key_hint} (stored in ~/.forged):"
    ))
    .with_display_mode(PasswordDisplayMode::Masked)
    .without_confirmation()
    .prompt()?;
    if !api_key.is_empty() {
        config.api_key = api_key;
    }

    // 3. Model selection
    if !provider_info.models.is_empty() {
        let models: Vec<&str> = provider_info.models.to_vec();
        let default_idx = models.iter().position(|m| *m == config.model).unwrap_or(0);
        let model = Select::new("Choose a model:", models)
            .with_starting_cursor(default_idx)
            .with_page_size(10)
            .prompt()?;
        config.model = model.to_string();
    }

    // 4. Commit type
    let type_labels: Vec<&str> = COMMIT_TYPES.iter().map(|(_, label)| *label).collect();
    let current_type_idx = COMMIT_TYPES
        .iter()
        .position(|(name, _)| *name == config.commit_type.as_str())
        .unwrap_or(0);
    let type_selection = Select::new("Commit message format:", type_labels)
        .with_starting_cursor(current_type_idx)
        .with_page_size(10)
        .prompt()?;
    let selected_type = COMMIT_TYPES
        .iter()
        .find(|(_, label)| *label == type_selection)
        .map(|(name, _)| *name)
        .unwrap_or("conventional");
    config.commit_type = CommitType::from_str_loose(selected_type)?;

    // 5. Locale
    let locale = Text::new("Message language:")
        .with_default(&config.locale)
        .with_help_message("e.g. en, pt-br, ja, es")
        .prompt()?;
    config.locale = locale;

    // Save
    config.save()?;

    println!();
    println!("{} Configuration saved to ~/.forged", "✔".green());
    println!(
        "{}",
        "  Run `forged config list` to review your settings.\n".dimmed()
    );

    Ok(config)
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
