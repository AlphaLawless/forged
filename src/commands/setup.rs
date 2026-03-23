use anyhow::Result;
use colored::Colorize;
use inquire::{Password, Select, Text};

use crate::config::{CommitType, Config};

const PROVIDERS: &[(&str, &str)] = &[
    ("claude", "Claude (Anthropic)"),
    // Future: ("gemini", "Gemini (Google)"),
    // Future: ("openrouter", "OpenRouter"),
    // Future: ("chatgpt", "ChatGPT (OpenAI)"),
];

const COMMIT_TYPES: &[(&str, &str)] = &[
    ("conventional", "conventional  — feat: / fix: / refactor: ..."),
    ("plain", "plain         — free-form message"),
    ("gitmoji", "gitmoji       — :emoji: message"),
    ("subject+body", "subject+body  — title + detailed description"),
];

const MODELS_CLAUDE: &[&str] = &[
    "claude-sonnet-4-6-20250514",
    "claude-haiku-4-5-20251001",
    "claude-opus-4-6-20250603",
];

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
    let provider_labels: Vec<&str> = PROVIDERS.iter().map(|(_, label)| *label).collect();
    let provider_idx = Select::new("Choose your AI provider:", provider_labels)
        .with_help_message("More providers coming soon")
        .prompt()?;
    let selected_provider = PROVIDERS
        .iter()
        .find(|(_, label)| *label == provider_idx)
        .map(|(name, _)| *name)
        .unwrap_or("claude");
    config.provider = selected_provider.into();

    // 2. API Key
    let key_hint = if !config.api_key.is_empty() {
        let visible = &config.api_key[..config.api_key.len().min(8)];
        format!(" (current: {visible}...)")
    } else {
        String::new()
    };
    let api_key = Password::new(&format!("Enter your API key{key_hint}:"))
        .without_confirmation()
        .with_help_message("Your key is stored locally in ~/.forged")
        .prompt()?;
    if !api_key.is_empty() {
        config.api_key = api_key;
    }

    // 3. Model selection
    let models: Vec<&str> = match selected_provider {
        "claude" => MODELS_CLAUDE.to_vec(),
        _ => vec![],
    };
    if !models.is_empty() {
        let default_idx = models
            .iter()
            .position(|m| *m == config.model)
            .unwrap_or(0);
        let model = Select::new("Choose a model:", models.clone())
            .with_starting_cursor(default_idx)
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
}
