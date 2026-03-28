use crate::config::{self, Config, ConfigSource, ConfigWithSources};
use anyhow::Result;
use colored::Colorize;
use inquire::Select;

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

fn mask_api_key(key: &str) -> String {
    if key.is_empty() {
        "(not set)".into()
    } else {
        let visible = &key[..key.len().min(8)];
        format!("{visible}...")
    }
}

fn source_tag(source: &ConfigSource) -> &'static str {
    match source {
        ConfigSource::Default => "default",
        ConfigSource::Global => "global",
        ConfigSource::Local => "local",
    }
}

fn print_config_table(sources: &ConfigWithSources) {
    let config = &sources.config;
    let fs = &sources.field_sources;
    let default_source = ConfigSource::Default;
    let get = |key: &str| -> &ConfigSource { fs.get(key).unwrap_or(&default_source) };

    // provider/api_key/model all share the "providers" source
    let provider_source = get("providers");

    let fields: &[(&str, String, &ConfigSource)] = &[
        ("provider", config.provider.clone(), provider_source),
        ("api_key", mask_api_key(&config.api_key), provider_source),
        ("model", config.model.clone(), provider_source),
        ("locale", config.locale.clone(), get("locale")),
        ("type", config.commit_type.as_str().to_string(), get("type")),
        ("max_length", config.max_length.to_string(), get("max_length")),
        ("generate", config.generate.to_string(), get("generate")),
        ("timeout", config.timeout.to_string(), get("timeout")),
    ];

    for (key, value, source) in fields {
        let tag = format!("[{}]", source_tag(source));
        let line = format!("  {:<12} = {:<30} {}", key, value, tag.dimmed());
        match source {
            ConfigSource::Local => println!("{}", line.green()),
            _ => println!("{line}"),
        }
    }

    // Show fallback providers if any
    if !config.fallback_providers.is_empty() {
        println!();
        println!("  {}", "Fallback providers:".dimmed());
        for (i, entry) in config.fallback_providers.iter().enumerate() {
            let model_display = if entry.model.is_empty() {
                "(default)".to_string()
            } else {
                entry.model.clone()
            };
            println!(
                "    {}. {} (model: {}, key: {})",
                i + 1,
                entry.name,
                model_display,
                mask_api_key(&entry.api_key),
            );
        }
    }
}

const GLOBAL_PREFIX: &str = "Global";

fn clear_screen() {
    print!("\x1b[2J\x1b[H");
}

fn wait_for_enter() {
    use std::io::{self, Read};
    println!();
    println!("  {}", "Press Enter to go back...".dimmed());
    let _ = io::stdin().read(&mut [0u8]);
}

pub fn run_list() -> Result<()> {
    let global_path = config::global_config_path()?;
    let profiles = config::list_profiles()?;

    loop {
        clear_screen();

        // Build options: Global + all local profiles
        let mut options = vec![format!(
            "{} ({})",
            GLOBAL_PREFIX,
            global_path.display()
        )];
        for name in &profiles {
            if let Ok(lp) = config::local_config_path(name) {
                options.push(format!("{name} ({lp})", lp = lp.display()));
            }
        }

        let selected = Select::new("Select a config to view:", options)
            .with_page_size(10)
            .prompt_skippable()?;

        let Some(choice) = selected else {
            break;
        };

        clear_screen();

        if choice.starts_with(GLOBAL_PREFIX) {
            let sources = Config::load_with_sources_at(&global_path, None, None)?;
            println!("  {}", "Global config".bold());
            println!("  {}", global_path.display().to_string().dimmed());
            println!();
            print_config_table(&sources);
        } else {
            let profile = choice.split(" (").next().unwrap_or(&choice);
            let local_path = config::local_config_path(profile)?;
            let sources =
                Config::load_with_sources_at(&global_path, Some(&local_path), Some(profile))?;
            println!(
                "  {} {}",
                "Profile:".bold(),
                profile.green().bold()
            );
            println!("  {}", local_path.display().to_string().dimmed());
            println!();
            print_config_table(&sources);
        }

        wait_for_enter();
    }

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
