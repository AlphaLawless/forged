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

#[derive(Debug, Clone, PartialEq)]
pub struct ProviderEntry {
    pub name: String,
    pub api_key: String,
    pub model: String,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub provider: String,
    pub api_key: String,
    pub model: String,
    pub fallback_providers: Vec<ProviderEntry>,
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
            fallback_providers: Vec::new(),
            locale: "en".into(),
            commit_type: CommitType::Plain,
            max_length: 72,
            generate: 1,
            timeout: 0, // 0 = use provider's default timeout
        }
    }
}

/// Base config directory: ~/.forged/
pub(crate) fn config_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Could not determine home directory")?;
    Ok(home.join(".forged"))
}

/// Global config file: ~/.forged/global
pub(crate) fn global_config_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("global"))
}

/// Locals directory: ~/.forged/locals/
pub(crate) fn locals_dir() -> Result<PathBuf> {
    Ok(config_dir()?.join("locals"))
}

/// Local config file for a profile: ~/.forged/locals/<profile>
pub(crate) fn local_config_path(profile: &str) -> Result<PathBuf> {
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

/// Parsed INI with support for [section] blocks.
#[derive(Debug, Default)]
struct ParsedIni {
    global: HashMap<String, String>,
    sections: HashMap<String, HashMap<String, String>>,
}

/// Parse INI content with section support.
/// Lines before any [section] go into `global`. Lines after [section.name]
/// go into `sections["name"]`.
fn parse_ini_sections(content: &str) -> ParsedIni {
    let mut result = ParsedIni::default();
    let mut current_section: Option<String> = None;

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            let section = line[1..line.len() - 1].trim();
            // Strip "provider." prefix if present
            let name = section.strip_prefix("provider.").unwrap_or(section);
            current_section = Some(name.to_string());
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            let k = key.trim().to_string();
            let v = value.trim().to_string();
            match &current_section {
                Some(section) => {
                    result
                        .sections
                        .entry(section.clone())
                        .or_default()
                        .insert(k, v);
                }
                None => {
                    result.global.insert(k, v);
                }
            }
        }
    }

    result
}

/// Parse a simple INI-like file (flat, no sections — for backwards compat).
fn parse_ini(content: &str) -> HashMap<String, String> {
    parse_ini_sections(content).global
}

/// Serialize global keys to flat INI format.
fn serialize_ini(map: &HashMap<String, String>) -> String {
    let mut keys: Vec<&String> = map.keys().collect();
    keys.sort();
    keys.iter()
        .map(|k| format!("{}={}", k, map[*k]))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Serialize a ParsedIni (global keys + sections) to INI format.
fn serialize_parsed(parsed: &ParsedIni) -> String {
    let mut output = serialize_ini(&parsed.global);

    // Sort section names for deterministic output
    let mut section_names: Vec<&String> = parsed.sections.keys().collect();
    section_names.sort();

    for name in section_names {
        let section = &parsed.sections[name];
        if section.is_empty() {
            continue;
        }
        output.push_str(&format!("\n\n[provider.{name}]\n"));
        output.push_str(&serialize_ini(section));
    }

    output
}

const MAX_PROVIDERS: usize = 4;
const VALID_PROVIDER_NAMES: &[&str] = &["claude", "gemini", "chatgpt", "openrouter"];

/// Collect source-trackable keys from INI content.
/// Maps legacy `provider`/`api_key`/`model` to `providers` for source tracking.
fn collect_source_keys(content: &str) -> Vec<String> {
    let parsed = parse_ini_sections(content);
    let mut keys: Vec<String> = Vec::new();

    for k in parsed.global.keys() {
        match k.as_str() {
            // Legacy keys map to "providers" for source tracking
            "provider" | "api_key" | "model" => {
                if !keys.contains(&"providers".to_string()) {
                    keys.push("providers".to_string());
                }
            }
            _ => keys.push(k.clone()),
        }
    }

    keys
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConfigSource {
    Default,
    Global,
    Local,
}

#[derive(Debug)]
pub struct ConfigWithSources {
    pub config: Config,
    pub profile: Option<String>,
    pub global_path: PathBuf,
    pub local_path: Option<PathBuf>,
    pub field_sources: HashMap<String, ConfigSource>,
}

const ALL_FIELD_KEYS: &[&str] = &[
    "providers",
    "locale",
    "type",
    "max_length",
    "generate",
    "timeout",
];

/// List all available local profile names from ~/.forged/locals/.
pub fn list_profiles() -> Result<Vec<String>> {
    ensure_config_dir()?;
    let dir = locals_dir()?;
    let mut names = Vec::new();
    if dir.is_dir() {
        for entry in fs::read_dir(&dir).context("Failed to read locals directory")? {
            let entry = entry?;
            if entry.file_type()?.is_file()
                && let Some(name) = entry.file_name().to_str()
            {
                names.push(name.to_string());
            }
        }
        names.sort();
    }
    Ok(names)
}

/// List profiles under a custom base directory (for testing).
pub fn list_profiles_at(base: &Path) -> Result<Vec<String>> {
    let dir = base.join("locals");
    let mut names = Vec::new();
    if dir.is_dir() {
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            if entry.file_type()?.is_file()
                && let Some(name) = entry.file_name().to_str()
            {
                names.push(name.to_string());
            }
        }
        names.sort();
    }
    Ok(names)
}

/// Remove a local profile file. Returns true if the file existed.
pub fn remove_local_profile(profile: &str) -> Result<bool> {
    ensure_config_dir()?;
    let path = local_config_path(profile)?;
    if path.exists() {
        fs::remove_file(&path)
            .with_context(|| format!("Failed to remove profile: {}", path.display()))?;
        Ok(true)
    } else {
        Ok(false)
    }
}

/// Remove a local profile file under a custom base (for testing).
pub fn remove_local_profile_at(base: &Path, profile: &str) -> Result<bool> {
    let path = base.join("locals").join(profile);
    if path.exists() {
        fs::remove_file(&path)?;
        Ok(true)
    } else {
        Ok(false)
    }
}

/// Check if a profile exists in ~/.forged/locals/.
pub fn profile_exists(profile: &str) -> Result<bool> {
    ensure_config_dir()?;
    Ok(local_config_path(profile)?.exists())
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

    /// Load config with per-field source tracking.
    pub fn load_with_sources() -> Result<ConfigWithSources> {
        ensure_config_dir()?;

        let gpath = global_config_path()?;
        let config_result = Self::load_from(&gpath)?;

        // Parse global INI to find which keys are explicitly set
        let global_keys = if gpath.exists() {
            let content = fs::read_to_string(&gpath)?;
            collect_source_keys(&content)
        } else {
            Vec::new()
        };

        let mut field_sources: HashMap<String, ConfigSource> = HashMap::new();
        for key in ALL_FIELD_KEYS {
            let k = key.to_string();
            if global_keys.contains(&k) {
                field_sources.insert(k, ConfigSource::Global);
            } else {
                field_sources.insert(k, ConfigSource::Default);
            }
        }

        let mut config = config_result;
        let mut profile_name = None;
        let mut lpath = None;

        if let Some(repo_root) = crate::git::try_repo_root() {
            let dot_forged = Path::new(&repo_root).join(".forged");
            if dot_forged.is_file()
                && let Ok(content) = fs::read_to_string(&dot_forged)
            {
                let profile = content.trim().to_string();
                if !profile.is_empty() {
                    let local_path = local_config_path(&profile)?;
                    if local_path.exists() {
                        let local_content = fs::read_to_string(&local_path)?;
                        let local_keys = collect_source_keys(&local_content);
                        for key in &local_keys {
                            field_sources.insert(key.clone(), ConfigSource::Local);
                        }

                        config.apply_overrides_from(&local_path)?;
                        lpath = Some(local_path);
                        profile_name = Some(profile);
                    }
                }
            }
        }

        Ok(ConfigWithSources {
            config,
            profile: profile_name,
            global_path: gpath,
            local_path: lpath,
            field_sources,
        })
    }

    /// Load config with per-field sources from custom paths (for testing).
    pub fn load_with_sources_at(
        global_path: &Path,
        local_path: Option<&Path>,
        profile: Option<&str>,
    ) -> Result<ConfigWithSources> {
        let config_result = Self::load_from(&global_path.to_path_buf())?;

        let global_keys = if global_path.exists() {
            let content = fs::read_to_string(global_path)?;
            collect_source_keys(&content)
        } else {
            Vec::new()
        };

        let mut field_sources: HashMap<String, ConfigSource> = HashMap::new();
        for key in ALL_FIELD_KEYS {
            let k = key.to_string();
            if global_keys.contains(&k) {
                field_sources.insert(k, ConfigSource::Global);
            } else {
                field_sources.insert(k, ConfigSource::Default);
            }
        }

        let mut config = config_result;
        let mut resolved_local = None;
        let mut resolved_profile = None;

        if let Some(lp) = local_path
            && lp.exists()
        {
            let local_content = fs::read_to_string(lp)?;
            let local_keys = collect_source_keys(&local_content);
            for key in &local_keys {
                field_sources.insert(key.clone(), ConfigSource::Local);
            }
            config.apply_overrides_from(&lp.to_path_buf())?;
            resolved_local = Some(lp.to_path_buf());
            resolved_profile = profile.map(|s| s.to_string());
        }

        Ok(ConfigWithSources {
            config,
            profile: resolved_profile,
            global_path: global_path.to_path_buf(),
            local_path: resolved_local,
            field_sources,
        })
    }

    /// Load config from a specific path.
    pub fn load_from(path: &PathBuf) -> Result<Self> {
        let mut config = Config::default();

        if !path.exists() {
            return Ok(config);
        }

        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {}", path.display()))?;
        let parsed = parse_ini_sections(&content);
        config.apply_parsed(&parsed)?;

        Ok(config)
    }

    /// Apply key-value pairs from a flat INI map (no sections). Only keys present are touched.
    fn apply_map(&mut self, map: &HashMap<String, String>) -> Result<()> {
        self.apply_common_fields(map)?;

        // Legacy single-provider fields
        if let Some(v) = map.get("provider") {
            self.provider = v.clone();
        }
        if let Some(v) = map.get("api_key") {
            self.api_key = v.clone();
        }
        if let Some(v) = map.get("model") {
            self.model = v.clone();
        }
        Ok(())
    }

    /// Apply parsed INI with sections support.
    fn apply_parsed(&mut self, parsed: &ParsedIni) -> Result<()> {
        self.apply_common_fields(&parsed.global)?;

        if let Some(providers_str) = parsed.global.get("providers") {
            // New multi-provider format
            let names: Vec<&str> = providers_str
                .split(',')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .collect();
            if names.is_empty() {
                bail!("'providers' cannot be empty");
            }
            if names.len() > MAX_PROVIDERS {
                bail!(
                    "Maximum of {MAX_PROVIDERS} providers allowed, got {}",
                    names.len()
                );
            }
            for name in &names {
                if !VALID_PROVIDER_NAMES.contains(name) {
                    bail!(
                        "Unknown provider: '{name}'. Available: {}",
                        VALID_PROVIDER_NAMES.join(", ")
                    );
                }
            }

            // First provider = primary
            let primary_section = parsed.sections.get(names[0]).cloned().unwrap_or_default();
            self.provider = names[0].to_string();
            self.api_key = primary_section.get("api_key").cloned().unwrap_or_default();
            self.model = primary_section.get("model").cloned().unwrap_or_default();

            // Remaining = fallbacks
            self.fallback_providers.clear();
            for name in &names[1..] {
                let section = parsed.sections.get(*name).cloned().unwrap_or_default();
                self.fallback_providers.push(ProviderEntry {
                    name: name.to_string(),
                    api_key: section.get("api_key").cloned().unwrap_or_default(),
                    model: section.get("model").cloned().unwrap_or_default(),
                });
            }
        } else {
            // Legacy single-provider fields (no providers= key)
            if let Some(v) = parsed.global.get("provider") {
                self.provider = v.clone();
            }
            if let Some(v) = parsed.global.get("api_key") {
                self.api_key = v.clone();
            }
            if let Some(v) = parsed.global.get("model") {
                self.model = v.clone();
            }
        }
        Ok(())
    }

    /// Apply common (non-provider) fields from a map.
    fn apply_common_fields(&mut self, map: &HashMap<String, String>) -> Result<()> {
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
        let parsed = parse_ini_sections(&content);
        self.apply_parsed(&parsed)
    }

    /// Save config to the global path (~/.forged/global).
    pub fn save_global(&self) -> Result<()> {
        ensure_config_dir()?;
        let path = global_config_path()?;
        self.save_to(&path)
    }

    /// Save only the fields that differ from a base config (for local overrides).
    pub fn save_diff_to(&self, path: &PathBuf, base: &Config) -> Result<()> {
        let mut parsed = ParsedIni::default();

        // Common fields diff
        if self.locale != base.locale {
            parsed.global.insert("locale".into(), self.locale.clone());
        }
        if self.commit_type != base.commit_type {
            parsed
                .global
                .insert("type".into(), self.commit_type.as_str().into());
        }
        if self.max_length != base.max_length {
            parsed
                .global
                .insert("max_length".into(), self.max_length.to_string());
        }
        if self.generate != base.generate {
            parsed
                .global
                .insert("generate".into(), self.generate.to_string());
        }
        if self.timeout != base.timeout {
            parsed
                .global
                .insert("timeout".into(), self.timeout.to_string());
        }

        // Provider diff: if providers differ at all, emit full provider block
        let providers_differ = self.provider != base.provider
            || self.api_key != base.api_key
            || self.model != base.model
            || self.fallback_providers != base.fallback_providers;

        if providers_differ && !self.provider.is_empty() {
            let provider_parsed = self.to_parsed_ini();
            // Copy providers= key and all sections
            if let Some(providers_val) = provider_parsed.global.get("providers") {
                parsed
                    .global
                    .insert("providers".into(), providers_val.clone());
            }
            for (name, section) in &provider_parsed.sections {
                parsed.sections.insert(name.clone(), section.clone());
            }
        }

        let content = serialize_parsed(&parsed);
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

    /// Save config to a specific path (always uses section format).
    pub fn save_to(&self, path: &PathBuf) -> Result<()> {
        let parsed = self.to_parsed_ini();
        let content = serialize_parsed(&parsed);
        fs::write(path, content)
            .with_context(|| format!("Failed to write config file: {}", path.display()))?;
        Ok(())
    }

    /// Build a ParsedIni representation of this config.
    fn to_parsed_ini(&self) -> ParsedIni {
        let mut parsed = ParsedIni::default();

        // Common fields in global section
        parsed.global.insert("locale".into(), self.locale.clone());
        parsed
            .global
            .insert("type".into(), self.commit_type.as_str().into());
        parsed
            .global
            .insert("max_length".into(), self.max_length.to_string());
        parsed
            .global
            .insert("generate".into(), self.generate.to_string());
        parsed
            .global
            .insert("timeout".into(), self.timeout.to_string());

        if !self.provider.is_empty() {
            // Build providers list
            let mut all_names = vec![self.provider.clone()];
            for entry in &self.fallback_providers {
                all_names.push(entry.name.clone());
            }
            parsed
                .global
                .insert("providers".into(), all_names.join(","));

            // Primary provider section
            let mut primary = HashMap::new();
            if !self.api_key.is_empty() {
                primary.insert("api_key".into(), self.api_key.clone());
            }
            if !self.model.is_empty() {
                primary.insert("model".into(), self.model.clone());
            }
            if !primary.is_empty() {
                parsed.sections.insert(self.provider.clone(), primary);
            }

            // Fallback provider sections
            for entry in &self.fallback_providers {
                let mut section = HashMap::new();
                if !entry.api_key.is_empty() {
                    section.insert("api_key".into(), entry.api_key.clone());
                }
                if !entry.model.is_empty() {
                    section.insert("model".into(), entry.model.clone());
                }
                if !section.is_empty() {
                    parsed.sections.insert(entry.name.clone(), section);
                }
            }
        }

        parsed
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
            ..Config::default()
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

    #[test]
    fn test_list_profiles_empty() {
        let tmp = TempDir::new().unwrap();
        let base = tmp.path().join(".forged");
        ensure_config_dir_at(&base).unwrap();

        let profiles = list_profiles_at(&base).unwrap();
        assert!(profiles.is_empty());
    }

    #[test]
    fn test_list_profiles_returns_names() {
        let tmp = TempDir::new().unwrap();
        let base = tmp.path().join(".forged");
        ensure_config_dir_at(&base).unwrap();

        fs::write(base.join("locals").join("repo-a"), "locale=ja\n").unwrap();
        fs::write(base.join("locals").join("repo-b"), "locale=pt-br\n").unwrap();

        let profiles = list_profiles_at(&base).unwrap();
        assert_eq!(profiles, vec!["repo-a", "repo-b"]);
    }

    #[test]
    fn test_remove_local_profile_deletes() {
        let tmp = TempDir::new().unwrap();
        let base = tmp.path().join(".forged");
        ensure_config_dir_at(&base).unwrap();

        let profile_path = base.join("locals").join("myrepo");
        fs::write(&profile_path, "locale=ja\n").unwrap();
        assert!(profile_path.exists());

        let removed = remove_local_profile_at(&base, "myrepo").unwrap();
        assert!(removed);
        assert!(!profile_path.exists());
    }

    #[test]
    fn test_remove_local_profile_nonexistent_ok() {
        let tmp = TempDir::new().unwrap();
        let base = tmp.path().join(".forged");
        ensure_config_dir_at(&base).unwrap();

        let removed = remove_local_profile_at(&base, "nope").unwrap();
        assert!(!removed);
    }

    #[test]
    fn test_load_with_sources_global_only() {
        let tmp = TempDir::new().unwrap();
        let base = tmp.path().join(".forged");
        ensure_config_dir_at(&base).unwrap();

        let global_path = base.join("global");
        fs::write(
            &global_path,
            "provider=claude\napi_key=sk-test\nmodel=claude-sonnet-4-6\n",
        )
        .unwrap();

        let result = Config::load_with_sources_at(&global_path, None, None).unwrap();

        assert_eq!(result.config.provider, "claude");
        assert!(result.profile.is_none());
        assert!(result.local_path.is_none());
        assert_eq!(result.field_sources["providers"], ConfigSource::Global);
        assert_eq!(result.field_sources["locale"], ConfigSource::Default);
        assert_eq!(result.field_sources["type"], ConfigSource::Default);
        assert_eq!(result.field_sources["max_length"], ConfigSource::Default);
    }

    #[test]
    fn test_load_with_sources_with_local_overrides() {
        let tmp = TempDir::new().unwrap();
        let base = tmp.path().join(".forged");
        ensure_config_dir_at(&base).unwrap();

        let global_path = base.join("global");
        fs::write(
            &global_path,
            "provider=claude\napi_key=sk-test\nlocale=en\n",
        )
        .unwrap();

        let local_path = base.join("locals").join("myrepo");
        fs::write(&local_path, "locale=pt-br\ntype=conventional\n").unwrap();

        let result =
            Config::load_with_sources_at(&global_path, Some(&local_path), Some("myrepo")).unwrap();

        assert_eq!(result.config.locale, "pt-br");
        assert_eq!(result.config.commit_type, CommitType::Conventional);
        assert_eq!(result.config.provider, "claude");
        assert_eq!(result.profile, Some("myrepo".to_string()));
        assert!(result.local_path.is_some());

        // Sources
        assert_eq!(result.field_sources["providers"], ConfigSource::Global);
        assert_eq!(result.field_sources["locale"], ConfigSource::Local);
        assert_eq!(result.field_sources["type"], ConfigSource::Local);
        assert_eq!(result.field_sources["max_length"], ConfigSource::Default);
    }

    #[test]
    fn test_parse_ini_with_sections() {
        let content = "locale=en\nproviders=claude,gemini\n\n[provider.claude]\napi_key=sk-test\nmodel=claude-sonnet-4-6\n\n[provider.gemini]\napi_key=AIza\nmodel=gemini-2.5-flash\n";
        let parsed = parse_ini_sections(content);

        assert_eq!(parsed.global["locale"], "en");
        assert_eq!(parsed.global["providers"], "claude,gemini");
        assert_eq!(parsed.sections["claude"]["api_key"], "sk-test");
        assert_eq!(parsed.sections["claude"]["model"], "claude-sonnet-4-6");
        assert_eq!(parsed.sections["gemini"]["api_key"], "AIza");
        assert_eq!(parsed.sections["gemini"]["model"], "gemini-2.5-flash");
    }

    #[test]
    fn test_apply_multi_provider() {
        let content = "providers=claude,gemini\nlocale=pt-br\n\n[provider.claude]\napi_key=sk-test\nmodel=claude-sonnet-4-6\n\n[provider.gemini]\napi_key=AIza\nmodel=gemini-2.5-flash\n";
        let mut config = Config::default();
        let parsed = parse_ini_sections(content);
        config.apply_parsed(&parsed).unwrap();

        assert_eq!(config.provider, "claude");
        assert_eq!(config.api_key, "sk-test");
        assert_eq!(config.model, "claude-sonnet-4-6");
        assert_eq!(config.locale, "pt-br");
        assert_eq!(config.fallback_providers.len(), 1);
        assert_eq!(config.fallback_providers[0].name, "gemini");
        assert_eq!(config.fallback_providers[0].api_key, "AIza");
        assert_eq!(config.fallback_providers[0].model, "gemini-2.5-flash");
    }

    #[test]
    fn test_apply_single_provider_backwards_compat() {
        let content = "provider=claude\napi_key=sk-test\nmodel=claude-sonnet-4-6\nlocale=en\n";
        let mut config = Config::default();
        let parsed = parse_ini_sections(content);
        config.apply_parsed(&parsed).unwrap();

        assert_eq!(config.provider, "claude");
        assert_eq!(config.api_key, "sk-test");
        assert_eq!(config.model, "claude-sonnet-4-6");
        assert!(config.fallback_providers.is_empty());
    }

    #[test]
    fn test_save_roundtrip_multi_provider() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("config");

        let config = Config {
            provider: "claude".into(),
            api_key: "sk-test".into(),
            model: "claude-sonnet-4-6".into(),
            fallback_providers: vec![ProviderEntry {
                name: "gemini".into(),
                api_key: "AIza".into(),
                model: "gemini-2.5-flash".into(),
            }],
            locale: "pt-br".into(),
            ..Config::default()
        };
        config.save_to(&path).unwrap();

        let loaded = Config::load_from(&path).unwrap();
        assert_eq!(loaded.provider, "claude");
        assert_eq!(loaded.api_key, "sk-test");
        assert_eq!(loaded.model, "claude-sonnet-4-6");
        assert_eq!(loaded.locale, "pt-br");
        assert_eq!(loaded.fallback_providers.len(), 1);
        assert_eq!(loaded.fallback_providers[0].name, "gemini");
        assert_eq!(loaded.fallback_providers[0].api_key, "AIza");
    }

    #[test]
    fn test_save_single_provider_uses_sections() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("config");

        let config = Config {
            provider: "claude".into(),
            api_key: "sk-test".into(),
            model: "claude-sonnet-4-6".into(),
            ..Config::default()
        };
        config.save_to(&path).unwrap();

        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("providers=claude"));
        assert!(content.contains("[provider.claude]"));
        assert!(content.contains("api_key=sk-test"));
        // Should NOT have legacy flat keys
        assert!(!content.contains("provider=claude"));
    }

    #[test]
    fn test_save_diff_multi_provider() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("local");

        let base = Config {
            provider: "claude".into(),
            api_key: "sk-base".into(),
            ..Config::default()
        };
        let modified = Config {
            provider: "gemini".into(),
            api_key: "AIza".into(),
            model: "gemini-2.5-flash".into(),
            fallback_providers: vec![ProviderEntry {
                name: "claude".into(),
                api_key: "sk-fallback".into(),
                model: "claude-sonnet-4-6".into(),
            }],
            ..Config::default()
        };

        modified.save_diff_to(&path, &base).unwrap();

        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("providers=gemini,claude"));
        assert!(content.contains("[provider.gemini]"));
        assert!(content.contains("[provider.claude]"));
    }

    #[test]
    fn test_max_four_providers_validation() {
        let content = "providers=claude,gemini,chatgpt,openrouter,claude\n";
        let mut config = Config::default();
        let parsed = parse_ini_sections(content);
        let err = config.apply_parsed(&parsed).unwrap_err();
        assert!(err.to_string().contains("Maximum of 4"));
    }

    #[test]
    fn test_invalid_provider_name_validation() {
        let content = "providers=claude,foobar\n";
        let mut config = Config::default();
        let parsed = parse_ini_sections(content);
        let err = config.apply_parsed(&parsed).unwrap_err();
        assert!(err.to_string().contains("Unknown provider"));
    }
}
