pub mod provider;
pub mod providers;
pub mod sanitize;

use anyhow::{Result, bail};
use crate::config::Config;
use provider::AiProvider;
use providers::claude::ClaudeProvider;
use providers::gemini::GeminiProvider;

/// Build the appropriate AI provider based on config.
pub fn build_provider(config: &Config) -> Result<Box<dyn AiProvider>> {
    match config.provider.as_str() {
        "claude" => {
            if config.api_key.is_empty() {
                bail!("Claude API key is required. Run `forged config set api_key <your-key>`");
            }
            Ok(Box::new(ClaudeProvider::new(config.api_key.clone())))
        }
        "gemini" => {
            if config.api_key.is_empty() {
                bail!("Gemini API key is required. Run `forged config set api_key <your-key>`");
            }
            Ok(Box::new(GeminiProvider::new(config.api_key.clone())))
        }
        "" => bail!("No provider configured. Run `forged config set provider <claude|gemini>`"),
        other => bail!("Unknown provider: '{other}'. Available: claude, gemini"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_claude_provider_from_config() {
        let config = Config {
            provider: "claude".into(),
            api_key: "sk-test".into(),
            ..Config::default()
        };
        let provider = build_provider(&config).unwrap();
        assert_eq!(provider.name(), "claude");
    }

    #[test]
    fn test_build_gemini_provider_from_config() {
        let config = Config {
            provider: "gemini".into(),
            api_key: "AIza-test".into(),
            ..Config::default()
        };
        let provider = build_provider(&config).unwrap();
        assert_eq!(provider.name(), "gemini");
    }

    #[test]
    fn test_build_gemini_without_api_key_returns_error() {
        let config = Config {
            provider: "gemini".into(),
            api_key: "".into(),
            ..Config::default()
        };
        let err = build_provider(&config).unwrap_err();
        assert!(err.to_string().contains("API key is required"));
    }

    #[test]
    fn test_build_unknown_provider_returns_error() {
        let config = Config {
            provider: "nonexistent".into(),
            api_key: "key".into(),
            ..Config::default()
        };
        let err = build_provider(&config).unwrap_err();
        assert!(err.to_string().contains("Unknown provider"));
    }

    #[test]
    fn test_build_provider_without_api_key_returns_error() {
        let config = Config {
            provider: "claude".into(),
            api_key: "".into(),
            ..Config::default()
        };
        let err = build_provider(&config).unwrap_err();
        assert!(err.to_string().contains("API key is required"));
    }
}
