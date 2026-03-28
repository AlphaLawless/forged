use anyhow::{Context, Result, bail};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

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

/// Base config directory: ~/.forged/
fn config_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Could not determine home directory")?;
    Ok(home.join(".forged"))
}

/// Global config file: ~/.forged/global
fn global_config_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("global"))
}

/// Locals directory: ~/.forged/locals/
fn locals_dir() -> Result<PathBuf> {
    Ok(config_dir()?.join("locals"))
}

/// Local config file for a profile: ~/.forged/locals/<profile>
fn local_config_path(profile: &str) -> Result<PathBuf> {
    Ok(locals_dir()?.join(profile))
}

/// Ensure ~/.forged/ directory structure exists, migrating from old file format if needed.
fn ensure_config_dir() -> Result<()> {
    let dir = config_dir()?;

    if dir.is_file() {
        // Old format: ~/.forged is a plain file. Migrate silently.
        let backup = dir.with_extension("bak");
        fs::rename(&dir, &backup).context("Failed to rename ~/.forged during migration")?;
        fs::create_dir_all(locals_dir()?).context("Failed to create ~/.forged/locals/")?;
        let global = global_config_path()?;
        fs::rename(&backup, &global).context("Failed to move config to ~/.forged/global")?;
    } else if !dir.exists() {
        fs::create_dir_all(locals_dir()?).context("Failed to create ~/.forged/locals/")?;
    } else if dir.is_dir() {
        let locals = locals_dir()?;
        if !locals.exists() {
            fs::create_dir_all(&locals)?;
        }
    }

    Ok(())
}

/// Ensure config dir structure under a custom base (for testing).
fn ensure_config_dir_at(base: &Path) -> Result<()> {
    let locals = base.join("locals");

    if base.is_file() {
        let backup = base.with_extension("bak");
        fs::rename(base, &backup)?;
        fs::create_dir_all(&locals)?;
        fs::rename(&backup, base.join("global"))?;
    } else if !base.exists() || (base.is_dir() && !locals.exists()) {
        fs::create_dir_all(&locals)?;
    }

    Ok(())
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
    /// Load config with full resolution: global + optional local overlay.
    pub fn load() -> Result<Self> {
        let (config, _) = Self::load_with_source()?;
        Ok(config)
    }

    /// Load global config only (for setup/config set that write to global).
    pub fn load_global() -> Result<Self> {
        ensure_config_dir()?;
        let path = global_config_path()?;
        Self::load_from(&path)
    }

    /// Load config with source info (profile name if active).
    pub fn load_with_source() -> Result<(Self, Option<String>)> {
        ensure_config_dir()?;

        let global_path = global_config_path()?;
        let mut config = Self::load_from(&global_path)?;
        let mut profile_name = None;

        if let Some(repo_root) = crate::git::try_repo_root() {
            let dot_forged = Path::new(&repo_root).join(".forged");
            if dot_forged.is_file()
                && let Ok(content) = fs::read_to_string(&dot_forged)
            {
                let profile = content.trim().to_string();
                if !profile.is_empty() {
                    let local_path = local_config_path(&profile)?;
                    if local_path.exists() {
                        config.apply_overrides_from(&local_path)?;
                        profile_name = Some(profile);
                    }
                }
            }
        }

        Ok((config, profile_name))
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
        config.apply_map(&map)?;

        Ok(config)
    }

    /// Apply key-value pairs from a parsed INI map. Only keys present are touched.
    fn apply_map(&mut self, map: &HashMap<String, String>) -> Result<()> {
        if let Some(v) = map.get("provider") {
            self.provider = v.clone();
        }
        if let Some(v) = map.get("api_key") {
            self.api_key = v.clone();
        }
        if let Some(v) = map.get("model") {
            self.model = v.clone();
        }
        if let Some(v) = map.get("locale") {
            if v.is_empty() {
                bail!("Config 'locale' cannot be empty");
            }
            self.locale = v.clone();
        }
        if let Some(v) = map.get("type") {
            self.commit_type = CommitType::from_str_loose(v)?;
        }
        if let Some(v) = map.get("max_length") {
            let n: u32 = v
                .parse()
                .context("Config 'max_length' must be an integer")?;
            if n < 20 {
                bail!("Config 'max_length' must be at least 20");
            }
            self.max_length = n;
        }
        if let Some(v) = map.get("generate") {
            let n: u8 = v.parse().context("Config 'generate' must be an integer")?;
            if n == 0 || n > 5 {
                bail!("Config 'generate' must be between 1 and 5");
            }
            self.generate = n;
        }
        if let Some(v) = map.get("timeout") {
            let n: u64 = v.parse().context("Config 'timeout' must be an integer")?;
            self.timeout = n;
        }
        Ok(())
    }

    /// Apply overrides from a local config file. Only fields present in the file are changed.
    pub fn apply_overrides_from(&mut self, path: &PathBuf) -> Result<()> {
        if !path.exists() {
            return Ok(());
        }
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {}", path.display()))?;
        let map = parse_ini(&content);
        self.apply_map(&map)
    }

    /// Save config to the global path (~/.forged/global).
    pub fn save_global(&self) -> Result<()> {
        ensure_config_dir()?;
        let path = global_config_path()?;
        self.save_to(&path)
    }

    /// Save only the fields that differ from a base config (for local overrides).
    pub fn save_diff_to(&self, path: &PathBuf, base: &Config) -> Result<()> {
        let mut map = HashMap::new();
        if self.provider != base.provider && !self.provider.is_empty() {
            map.insert("provider".into(), self.provider.clone());
        }
        if self.api_key != base.api_key && !self.api_key.is_empty() {
            map.insert("api_key".into(), self.api_key.clone());
        }
        if self.model != base.model && !self.model.is_empty() {
            map.insert("model".into(), self.model.clone());
        }
        if self.locale != base.locale {
            map.insert("locale".into(), self.locale.clone());
        }
        if self.commit_type != base.commit_type {
            map.insert("type".into(), self.commit_type.as_str().into());
        }
        if self.max_length != base.max_length {
            map.insert("max_length".into(), self.max_length.to_string());
        }
        if self.generate != base.generate {
            map.insert("generate".into(), self.generate.to_string());
        }
        if self.timeout != base.timeout {
            map.insert("timeout".into(), self.timeout.to_string());
        }

        let content = serialize_ini(&map);
        fs::write(path, content)
            .with_context(|| format!("Failed to write config file: {}", path.display()))?;
        Ok(())
    }

    /// Save local profile config (only fields that differ from global).
    pub fn save_local(&self, profile: &str) -> Result<()> {
        ensure_config_dir()?;
        let global = Self::load_from(&global_config_path()?)?;
        let path = local_config_path(profile)?;
        self.save_diff_to(&path, &global)
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
    use tempfile::{NamedTempFile, TempDir};

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

        let config = Config {
            provider: "claude".into(),
            api_key: "sk-test".into(),
            model: "claude-sonnet-4-6".into(),
            locale: "pt-br".into(),
            commit_type: CommitType::Gitmoji,
            max_length: 50,
            generate: 3,
            timeout: 20,
        };
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

    #[test]
    fn test_apply_map_partial_override() {
        let mut config = Config {
            provider: "claude".into(),
            api_key: "sk-test".into(),
            locale: "en".into(),
            commit_type: CommitType::Plain,
            max_length: 72,
            ..Config::default()
        };

        let mut map = HashMap::new();
        map.insert("locale".into(), "pt-br".into());
        map.insert("type".into(), "gitmoji".into());
        config.apply_map(&map).unwrap();

        // Changed
        assert_eq!(config.locale, "pt-br");
        assert_eq!(config.commit_type, CommitType::Gitmoji);
        // Unchanged
        assert_eq!(config.provider, "claude");
        assert_eq!(config.api_key, "sk-test");
        assert_eq!(config.max_length, 72);
    }

    #[test]
    fn test_apply_map_validation_still_works() {
        let mut config = Config::default();
        let mut map = HashMap::new();
        map.insert("max_length".into(), "5".into());
        let err = config.apply_map(&map).unwrap_err();
        assert!(err.to_string().contains("at least 20"));
    }

    #[test]
    fn test_apply_overrides_from_empty_file() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("empty");
        fs::write(&path, "").unwrap();

        let mut config = Config {
            provider: "claude".into(),
            locale: "en".into(),
            ..Config::default()
        };
        config.apply_overrides_from(&path).unwrap();
        assert_eq!(config.provider, "claude");
        assert_eq!(config.locale, "en");
    }

    #[test]
    fn test_apply_overrides_from_nonexistent_is_noop() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("nope");

        let mut config = Config::default();
        config.apply_overrides_from(&path).unwrap();
        assert_eq!(config.locale, "en"); // default unchanged
    }

    #[test]
    fn test_save_diff_to_only_includes_changed_fields() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("local");

        let base = Config::default();
        let modified = Config {
            locale: "ja".into(),
            commit_type: CommitType::Conventional,
            ..Config::default()
        };

        modified.save_diff_to(&path, &base).unwrap();

        let content = fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(content.contains("locale=ja"));
        assert!(content.contains("type=conventional"));
        // Should NOT contain provider, api_key, etc.
        assert!(!content.contains("provider"));
        assert!(!content.contains("max_length"));
    }

    #[test]
    fn test_save_diff_to_empty_when_identical() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("local");

        let base = Config::default();
        let same = Config::default();

        same.save_diff_to(&path, &base).unwrap();

        let content = fs::read_to_string(&path).unwrap();
        assert!(content.trim().is_empty());
    }

    #[test]
    fn test_migration_file_to_dir() {
        let tmp = TempDir::new().unwrap();
        let base = tmp.path().join(".forged");

        // Write old-format file
        fs::write(&base, "provider=claude\napi_key=sk-test\n").unwrap();
        assert!(base.is_file());

        ensure_config_dir_at(&base).unwrap();

        // Now it should be a directory
        assert!(base.is_dir());
        assert!(base.join("locals").is_dir());
        assert!(base.join("global").is_file());

        // Content preserved
        let content = fs::read_to_string(base.join("global")).unwrap();
        assert!(content.contains("provider=claude"));
        assert!(content.contains("api_key=sk-test"));
    }

    #[test]
    fn test_migration_dir_already_exists() {
        let tmp = TempDir::new().unwrap();
        let base = tmp.path().join(".forged");
        fs::create_dir_all(base.join("locals")).unwrap();
        fs::write(base.join("global"), "provider=gemini\n").unwrap();

        ensure_config_dir_at(&base).unwrap();

        // Nothing changed
        assert!(base.is_dir());
        let content = fs::read_to_string(base.join("global")).unwrap();
        assert!(content.contains("provider=gemini"));
    }

    #[test]
    fn test_migration_creates_dir_when_nothing_exists() {
        let tmp = TempDir::new().unwrap();
        let base = tmp.path().join(".forged");
        assert!(!base.exists());

        ensure_config_dir_at(&base).unwrap();

        assert!(base.is_dir());
        assert!(base.join("locals").is_dir());
    }

    #[test]
    fn test_load_merges_global_and_local() {
        let tmp = TempDir::new().unwrap();
        let base = tmp.path().join(".forged");
        fs::create_dir_all(base.join("locals")).unwrap();

        // Write global
        let global = Config {
            provider: "claude".into(),
            api_key: "sk-global".into(),
            locale: "en".into(),
            commit_type: CommitType::Plain,
            max_length: 72,
            ..Config::default()
        };
        global.save_to(&base.join("global")).unwrap();

        // Write local override (only locale + type)
        fs::write(
            base.join("locals").join("myrepo"),
            "locale=pt-br\ntype=conventional\n",
        )
        .unwrap();

        // Load global then apply local
        let mut config = Config::load_from(&base.join("global")).unwrap();
        config
            .apply_overrides_from(&base.join("locals").join("myrepo"))
            .unwrap();

        // Overridden
        assert_eq!(config.locale, "pt-br");
        assert_eq!(config.commit_type, CommitType::Conventional);
        // Inherited from global
        assert_eq!(config.provider, "claude");
        assert_eq!(config.api_key, "sk-global");
        assert_eq!(config.max_length, 72);
    }
}
