use crate::config::Config;
use anyhow::Result;

pub fn run_set(key: &str, value: &str) -> Result<()> {
    let mut config = Config::load_global()?;
    config.set(key, value)?;
    config.save_global()?;
    println!("Set {key}={value}");
    Ok(())
}

pub fn run_get(key: &str) -> Result<()> {
    let config = Config::load()?;
    let value = match key {
        "provider" => config.provider.clone(),
        "api_key" => {
            if config.api_key.is_empty() {
                "(not set)".into()
            } else {
                // Mask the key for security
                let visible = &config.api_key[..config.api_key.len().min(8)];
                format!("{visible}...")
            }
        }
        "model" => config.model.clone(),
        "locale" => config.locale.clone(),
        "type" => config.commit_type.as_str().into(),
        "max_length" => config.max_length.to_string(),
        "generate" => config.generate.to_string(),
        "timeout" => config.timeout.to_string(),
        _ => anyhow::bail!("Unknown config key: '{key}'"),
    };
    println!("{value}");
    Ok(())
}

pub fn run_list() -> Result<()> {
    let (config, profile) = Config::load_with_source()?;

    if let Some(ref name) = profile {
        println!("# profile: {name}");
    }

    println!("provider={}", config.provider);
    println!(
        "api_key={}",
        if config.api_key.is_empty() {
            "(not set)".into()
        } else {
            let visible = &config.api_key[..config.api_key.len().min(8)];
            format!("{visible}...")
        }
    );
    println!("model={}", config.model);
    println!("locale={}", config.locale);
    println!("type={}", config.commit_type.as_str());
    println!("max_length={}", config.max_length);
    println!("generate={}", config.generate);
    println!("timeout={}", config.timeout);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // These tests need to write to real files so we test the command module
    // through Config directly rather than through run_set/run_get which use home dir.

    #[test]
    fn test_config_set_persists_to_file() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join(".forged");

        let mut config = Config::default();
        config.set("provider", "claude").unwrap();
        config.save_to(&path).unwrap();

        let loaded = Config::load_from(&path).unwrap();
        assert_eq!(loaded.provider, "claude");
    }

    #[test]
    fn test_config_get_reads_persisted_value() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join(".forged");

        let mut config = Config::default();
        config.set("locale", "pt-br").unwrap();
        config.save_to(&path).unwrap();

        let loaded = Config::load_from(&path).unwrap();
        assert_eq!(loaded.locale, "pt-br");
    }

    #[test]
    fn test_config_set_invalid_key_prints_error() {
        let mut config = Config::default();
        let err = config.set("foobar", "value").unwrap_err();
        assert!(err.to_string().contains("Unknown config key"));
    }

    #[test]
    fn test_config_set_provider_updates_correctly() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join(".forged");

        let mut config = Config::default();
        config.set("provider", "claude").unwrap();
        config.save_to(&path).unwrap();

        let mut config = Config::load_from(&path).unwrap();
        config.set("provider", "claude").unwrap();
        config.save_to(&path).unwrap();

        let loaded = Config::load_from(&path).unwrap();
        assert_eq!(loaded.provider, "claude");
    }
}
