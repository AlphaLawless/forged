pub mod provider;
pub mod providers;
pub mod sanitize;

use crate::config::Config;
use anyhow::{Result, bail};
use provider::AiProvider;
use providers::claude::ClaudeProvider;

/// A provider instance with its resolved model and timeout.
pub struct ProviderWithOpts {
    pub provider: Box<dyn AiProvider>,
    pub model: String,
    pub timeout: u64,
}

/// Report of which provider was used and which failed during failover.
#[derive(Debug, Clone)]
pub struct FailoverReport {
    pub used_provider: String,
    pub used_model: String,
    pub failures: Vec<FailoverFailure>,
}

/// A single provider failure during failover.
#[derive(Debug, Clone)]
pub struct FailoverFailure {
    pub provider: String,
    pub reason: String,
}

/// Build a provider instance from a provider name and API key.
fn build_provider_from_entry(name: &str, api_key: &str) -> Result<Box<dyn AiProvider>> {
    match name {
        "claude" => {
            if api_key.is_empty() {
                bail!("Claude API key is required. Run `forged config set api_key <your-key>`");
            }
            Ok(Box::new(ClaudeProvider::new(api_key.to_string())))
        }
        "gemini" => {
            if api_key.is_empty() {
                bail!("Gemini API key is required. Run `forged config set api_key <your-key>`");
            }
            Ok(Box::new(providers::gemini::new(api_key.to_string())))
        }
        "chatgpt" => {
            if api_key.is_empty() {
                bail!("ChatGPT API key is required. Run `forged config set api_key <your-key>`");
            }
            Ok(Box::new(providers::chatgpt::new(api_key.to_string())))
        }
        "openrouter" => {
            if api_key.is_empty() {
                bail!("OpenRouter API key is required. Run `forged config set api_key <your-key>`");
            }
            Ok(Box::new(providers::openrouter::new(api_key.to_string())))
        }
        "" => bail!(
            "No provider configured. Run `forged config set provider <claude|gemini|chatgpt|openrouter>`"
        ),
        other => {
            bail!("Unknown provider: '{other}'. Available: claude, gemini, chatgpt, openrouter")
        }
    }
}

/// Build the appropriate AI provider based on config (single provider, backwards compat).
pub fn build_provider(config: &Config) -> Result<Box<dyn AiProvider>> {
    build_provider_from_entry(&config.provider, &config.api_key)
}

/// Build all configured providers (primary + fallbacks) with resolved opts.
pub fn build_providers(config: &Config) -> Result<Vec<ProviderWithOpts>> {
    let mut result = Vec::new();

    // Primary
    let primary = build_provider_from_entry(&config.provider, &config.api_key)?;
    let model = if config.model.is_empty() {
        primary.default_model().to_string()
    } else {
        config.model.clone()
    };
    let timeout = if config.timeout > 0 {
        config.timeout
    } else {
        primary.default_timeout()
    };
    result.push(ProviderWithOpts {
        provider: primary,
        model,
        timeout,
    });

    // Fallbacks
    for entry in &config.fallback_providers {
        let p = build_provider_from_entry(&entry.name, &entry.api_key)?;
        let m = if entry.model.is_empty() {
            p.default_model().to_string()
        } else {
            entry.model.clone()
        };
        let t = if config.timeout > 0 {
            config.timeout
        } else {
            p.default_timeout()
        };
        result.push(ProviderWithOpts {
            provider: p,
            model: m,
            timeout: t,
        });
    }

    Ok(result)
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
    fn test_build_chatgpt_provider_from_config() {
        let config = Config {
            provider: "chatgpt".into(),
            api_key: "sk-openai-test".into(),
            ..Config::default()
        };
        let provider = build_provider(&config).unwrap();
        assert_eq!(provider.name(), "chatgpt");
    }

    #[test]
    fn test_build_chatgpt_without_api_key_returns_error() {
        let config = Config {
            provider: "chatgpt".into(),
            api_key: "".into(),
            ..Config::default()
        };
        let err = build_provider(&config).unwrap_err();
        assert!(err.to_string().contains("API key is required"));
    }

    #[test]
    fn test_build_openrouter_provider_from_config() {
        let config = Config {
            provider: "openrouter".into(),
            api_key: "sk-or-test".into(),
            ..Config::default()
        };
        let provider = build_provider(&config).unwrap();
        assert_eq!(provider.name(), "openrouter");
    }

    #[test]
    fn test_build_openrouter_without_api_key_returns_error() {
        let config = Config {
            provider: "openrouter".into(),
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

    #[test]
    fn test_build_providers_single() {
        let config = Config {
            provider: "claude".into(),
            api_key: "sk-test".into(),
            model: "claude-sonnet-4-6".into(),
            ..Config::default()
        };
        let providers = build_providers(&config).unwrap();
        assert_eq!(providers.len(), 1);
        assert_eq!(providers[0].provider.name(), "claude");
        assert_eq!(providers[0].model, "claude-sonnet-4-6");
    }

    #[test]
    fn test_build_providers_with_fallback() {
        let config = Config {
            provider: "claude".into(),
            api_key: "sk-test".into(),
            model: "claude-sonnet-4-6".into(),
            fallback_providers: vec![crate::config::ProviderEntry {
                name: "gemini".into(),
                api_key: "AIza".into(),
                model: "gemini-2.5-flash".into(),
            }],
            ..Config::default()
        };
        let providers = build_providers(&config).unwrap();
        assert_eq!(providers.len(), 2);
        assert_eq!(providers[0].provider.name(), "claude");
        assert_eq!(providers[1].provider.name(), "gemini");
        assert_eq!(providers[1].model, "gemini-2.5-flash");
    }

    #[test]
    fn test_build_providers_fallback_default_model() {
        let config = Config {
            provider: "claude".into(),
            api_key: "sk-test".into(),
            fallback_providers: vec![crate::config::ProviderEntry {
                name: "gemini".into(),
                api_key: "AIza".into(),
                model: "".into(), // empty = use provider default
            }],
            ..Config::default()
        };
        let providers = build_providers(&config).unwrap();
        assert_eq!(providers[1].model, "gemini-2.5-flash"); // default model
    }
}
