use anyhow::{Context, Result, bail};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq)]
pub enum CommitType {
    Plain,
    Conventional,
    Gitmoji,
    SubjectBody,
}

impl CommitType {
    pub fn from_str_loose(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "plain" => Ok(Self::Plain),
            "conventional" => Ok(Self::Conventional),
            "gitmoji" => Ok(Self::Gitmoji),
            "subject+body" | "subjectbody" | "subject_body" => Ok(Self::SubjectBody),
            other => bail!(
                "Invalid commit type: '{other}'. Valid: plain, conventional, gitmoji, subject+body"
            ),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::Plain => "plain",
            Self::Conventional => "conventional",
            Self::Gitmoji => "gitmoji",
            Self::SubjectBody => "subject+body",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Config {
    pub provider: String,
    pub api_key: String,
    pub model: String,
    pub locale: String,
    pub commit_type: CommitType,
    pub max_length: u32,
    pub generate: u8,
    pub timeout: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            provider: String::new(),
            api_key: String::new(),
            model: String::new(),
            locale: "en".into(),
            commit_type: CommitType::Plain,
            max_length: 72,
            generate: 1,
            timeout: 0, // 0 = use provider's default timeout
        }
    }
}

fn config_path() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Could not determine home directory")?;
    Ok(home.join(".forged"))
}

/// Parse a simple INI-like file (key=value per line, no sections).
fn parse_ini(content: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            map.insert(key.trim().to_string(), value.trim().to_string());
        }
    }
    map
}

/// Serialize a HashMap back to INI format.
fn serialize_ini(map: &HashMap<String, String>) -> String {
    let mut keys: Vec<&String> = map.keys().collect();
    keys.sort();
    keys.iter()
        .map(|k| format!("{}={}", k, map[*k]))
        .collect::<Vec<_>>()
        .join("\n")
}

impl Config {
    /// Load config from the default path (~/.forged).
    pub fn load() -> Result<Self> {
        let path = config_path()?;
        Self::load_from(&path)
    }

    /// Load config from a specific path.
    pub fn load_from(path: &PathBuf) -> Result<Self> {
        let mut config = Config::default();

        if !path.exists() {
            return Ok(config);
        }

        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {}", path.display()))?;
        let map = parse_ini(&content);

        if let Some(v) = map.get("provider") {
            config.provider = v.clone();
        }
        if let Some(v) = map.get("api_key") {
            config.api_key = v.clone();
        }
        if let Some(v) = map.get("model") {
            config.model = v.clone();
        }
        if let Some(v) = map.get("locale") {
            if v.is_empty() {
                bail!("Config 'locale' cannot be empty");
            }
            config.locale = v.clone();
        }
        if let Some(v) = map.get("type") {
            config.commit_type = CommitType::from_str_loose(v)?;
        }
        if let Some(v) = map.get("max_length") {
            let n: u32 = v
                .parse()
                .context("Config 'max_length' must be an integer")?;
            if n < 20 {
                bail!("Config 'max_length' must be at least 20");
            }
            config.max_length = n;
        }
        if let Some(v) = map.get("generate") {
            let n: u8 = v.parse().context("Config 'generate' must be an integer")?;
            if n == 0 || n > 5 {
                bail!("Config 'generate' must be between 1 and 5");
            }
            config.generate = n;
        }
        if let Some(v) = map.get("timeout") {
            let n: u64 = v.parse().context("Config 'timeout' must be an integer")?;
            config.timeout = n;
        }

        Ok(config)
    }

    /// Save config to the default path.
    pub fn save(&self) -> Result<()> {
        let path = config_path()?;
        self.save_to(&path)
    }

    /// Save config to a specific path.
    pub fn save_to(&self, path: &PathBuf) -> Result<()> {
        let mut map = HashMap::new();
        if !self.provider.is_empty() {
            map.insert("provider".into(), self.provider.clone());
        }
        if !self.api_key.is_empty() {
            map.insert("api_key".into(), self.api_key.clone());
        }
        if !self.model.is_empty() {
            map.insert("model".into(), self.model.clone());
        }
        map.insert("locale".into(), self.locale.clone());
        map.insert("type".into(), self.commit_type.as_str().into());
        map.insert("max_length".into(), self.max_length.to_string());
        map.insert("generate".into(), self.generate.to_string());
        map.insert("timeout".into(), self.timeout.to_string());

        let content = serialize_ini(&map);
        fs::write(path, content)
            .with_context(|| format!("Failed to write config file: {}", path.display()))?;
        Ok(())
    }

    /// Set a single key-value pair, validating the key and value.
    pub fn set(&mut self, key: &str, value: &str) -> Result<()> {
        match key {
            "provider" => self.provider = value.into(),
            "api_key" => self.api_key = value.into(),
            "model" => self.model = value.into(),
            "locale" => {
                if value.is_empty() {
                    bail!("'locale' cannot be empty");
                }
                self.locale = value.into();
            }
            "type" => self.commit_type = CommitType::from_str_loose(value)?,
            "max_length" => {
                let n: u32 = value.parse().context("'max_length' must be an integer")?;
                if n < 20 {
                    bail!("'max_length' must be at least 20");
                }
                self.max_length = n;
            }
            "generate" => {
                let n: u8 = value.parse().context("'generate' must be an integer")?;
                if n == 0 || n > 5 {
                    bail!("'generate' must be between 1 and 5");
                }
                self.generate = n;
            }
            "timeout" => {
                let n: u64 = value.parse().context("'timeout' must be an integer")?;
                self.timeout = n;
            }
            other => bail!("Unknown config key: '{other}'"),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_config_defaults_when_file_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join(".forged");
        let config = Config::load_from(&path).unwrap();
        assert_eq!(config.locale, "en");
        assert_eq!(config.max_length, 72);
        assert_eq!(config.generate, 1);
        assert_eq!(config.commit_type, CommitType::Plain);
        assert!(config.provider.is_empty());
    }

    #[test]
    fn test_config_load_from_ini_string() {
        let mut f = NamedTempFile::new().unwrap();
        writeln!(f, "provider=claude").unwrap();
        writeln!(f, "api_key=sk-ant-test123").unwrap();
        writeln!(f, "model=claude-sonnet-4-6").unwrap();
        writeln!(f, "locale=pt-br").unwrap();
        writeln!(f, "type=conventional").unwrap();
        writeln!(f, "max_length=50").unwrap();
        writeln!(f, "generate=3").unwrap();
        writeln!(f, "timeout=30").unwrap();

        let path = f.path().to_path_buf();
        let config = Config::load_from(&path).unwrap();
        assert_eq!(config.provider, "claude");
        assert_eq!(config.api_key, "sk-ant-test123");
        assert_eq!(config.model, "claude-sonnet-4-6");
        assert_eq!(config.locale, "pt-br");
        assert_eq!(config.commit_type, CommitType::Conventional);
        assert_eq!(config.max_length, 50);
        assert_eq!(config.generate, 3);
        assert_eq!(config.timeout, 30);
    }

    #[test]
    fn test_config_invalid_max_length_below_20() {
        let mut f = NamedTempFile::new().unwrap();
        writeln!(f, "max_length=10").unwrap();
        let path = f.path().to_path_buf();
        let err = Config::load_from(&path).unwrap_err();
        assert!(err.to_string().contains("at least 20"));
    }

    #[test]
    fn test_config_generate_above_5_returns_error() {
        let mut f = NamedTempFile::new().unwrap();
        writeln!(f, "generate=10").unwrap();
        let path = f.path().to_path_buf();
        let err = Config::load_from(&path).unwrap_err();
        assert!(err.to_string().contains("between 1 and 5"));
    }

    #[test]
    fn test_config_generate_zero_returns_error() {
        let mut f = NamedTempFile::new().unwrap();
        writeln!(f, "generate=0").unwrap();
        let path = f.path().to_path_buf();
        let err = Config::load_from(&path).unwrap_err();
        assert!(err.to_string().contains("between 1 and 5"));
    }

    #[test]
    fn test_config_set_unknown_key_returns_error() {
        let mut config = Config::default();
        let err = config.set("nonexistent", "value").unwrap_err();
        assert!(err.to_string().contains("Unknown config key"));
    }

    #[test]
    fn test_config_invalid_commit_type() {
        let mut f = NamedTempFile::new().unwrap();
        writeln!(f, "type=foobar").unwrap();
        let path = f.path().to_path_buf();
        let err = Config::load_from(&path).unwrap_err();
        assert!(err.to_string().contains("Invalid commit type"));
    }

    #[test]
    fn test_config_save_and_reload_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join(".forged");

        let mut config = Config::default();
        config.provider = "claude".into();
        config.api_key = "sk-test".into();
        config.model = "claude-sonnet-4-6".into();
        config.locale = "pt-br".into();
        config.commit_type = CommitType::Gitmoji;
        config.max_length = 50;
        config.generate = 3;
        config.timeout = 20;
        config.save_to(&path).unwrap();

        let loaded = Config::load_from(&path).unwrap();
        assert_eq!(loaded.provider, "claude");
        assert_eq!(loaded.api_key, "sk-test");
        assert_eq!(loaded.model, "claude-sonnet-4-6");
        assert_eq!(loaded.locale, "pt-br");
        assert_eq!(loaded.commit_type, CommitType::Gitmoji);
        assert_eq!(loaded.max_length, 50);
        assert_eq!(loaded.generate, 3);
        assert_eq!(loaded.timeout, 20);
    }
}
