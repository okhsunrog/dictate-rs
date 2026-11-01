use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::{io::Write, os::unix::fs::PermissionsExt, path::PathBuf};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputMode {
    #[default]
    Preview,
    Clipboard,
    Paste,
}
impl OutputMode {
    /// Parse a per-invocation output-mode override from the command line, e.g.
    /// `clipboard`, `--paste`, `copy`. Returns `None` for anything else.
    pub fn from_cli(s: &str) -> Option<Self> {
        match s.trim_start_matches("--") {
            "preview" => Some(Self::Preview),
            "clipboard" | "copy" => Some(Self::Clipboard),
            "paste" | "type" => Some(Self::Paste),
            _ => None,
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub language: String,
    pub output: OutputMode,
    pub paste_shift: bool,
    pub microphone: String,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            base_url: String::new(),
            api_key: String::new(),
            model: String::new(),
            language: String::new(),
            output: OutputMode::Preview,
            paste_shift: false,
            microphone: String::new(),
        }
    }
}
impl Config {
    pub fn path() -> Result<PathBuf> {
        Ok(directories::ProjectDirs::from("", "", "dictate")
            .context("Не найден каталог настроек")?
            .config_dir()
            .join("config.toml"))
    }
    /// Load the on-disk config only. Environment overrides are applied
    /// separately by [`Config::effective`] so that a key provided via the
    /// environment is never written back to the file on save.
    pub fn load() -> Result<Self> {
        let path = Self::path()?;
        match std::fs::read_to_string(path) {
            Ok(text) => toml::from_str(&text).context("Некорректный config.toml"),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e.into()),
        }
    }
    /// The config actually used at runtime: the saved values with
    /// `DICTATE_API_KEY` / `DICTATE_BASE_URL` overlaid. Use this for the proxy
    /// and for validation before recording; never persist it.
    pub fn effective(&self) -> Self {
        self.with_overrides(
            std::env::var("DICTATE_API_KEY").ok(),
            std::env::var("DICTATE_BASE_URL").ok(),
        )
    }
    fn with_overrides(&self, key: Option<String>, url: Option<String>) -> Self {
        let mut config = self.clone();
        if let Some(key) = key {
            config.api_key = key;
        }
        if let Some(url) = url {
            config.base_url = url;
        }
        config
    }
    pub fn validate(&self) -> Result<()> {
        let url = reqwest::Url::parse(&self.base_url).context("Некорректный адрес прокси")?;
        if !["http", "https"].contains(&url.scheme())
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            bail!("Нужен HTTP(S) адрес прокси без пароля, query и fragment");
        }
        if self.api_key.trim().is_empty() {
            bail!("Укажи ключ API в настройках");
        }
        if self.model.trim().is_empty() {
            bail!("Укажи модель редактирования");
        }
        if !self.language.is_empty()
            && !(self.language.len() >= 2
                && self.language.len() <= 12
                && self
                    .language
                    .bytes()
                    .all(|c| c.is_ascii_alphabetic() || c == b'-'))
        {
            bail!("Язык: ru, en или пустое поле для автоопределения");
        }
        Ok(())
    }
    pub fn save(&self) -> Result<()> {
        // Validate the effective config (so an env-provided key is accepted),
        // but persist only `self` — the env override is never written to disk.
        self.effective().validate()?;
        let path = Self::path()?;
        let dir = path.parent().unwrap();
        std::fs::create_dir_all(dir)?;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
        let mut file = tempfile::NamedTempFile::new_in(dir)?;
        file.as_file()
            .set_permissions(std::fs::Permissions::from_mode(0o600))?;
        file.write_all(toml::to_string_pretty(self)?.as_bytes())?;
        file.as_file().sync_all()?;
        file.persist(path)?;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn base() -> Config {
        Config {
            base_url: "https://proxy.example.com/v1".into(),
            api_key: "secret".into(),
            model: "test-model".into(),
            ..Config::default()
        }
    }
    #[test]
    fn accepts_reasonable_config() {
        assert!(base().validate().is_ok());
        let mut c = base();
        c.language = "ru".into();
        assert!(c.validate().is_ok());
        c.language = "en-US".into();
        assert!(c.validate().is_ok());
    }
    #[test]
    fn rejects_missing_key_and_bad_url() {
        assert!(Config::default().validate().is_err(), "empty key");
        let mut c = base();
        c.base_url = "ftp://example.com".into();
        assert!(c.validate().is_err(), "non-http scheme");
        c = base();
        c.base_url = "https://u:p@example.com/v1".into();
        assert!(c.validate().is_err(), "url with credentials");
        c = base();
        c.base_url = "https://example.com/v1?x=1".into();
        assert!(c.validate().is_err(), "url with query");
        c = base();
        c.language = "русский".into();
        assert!(c.validate().is_err(), "non-ascii language");
    }
    #[test]
    fn toml_round_trip_preserves_fields() {
        let mut c = base();
        c.output = OutputMode::Paste;
        c.paste_shift = true;
        c.language = "ru".into();
        c.microphone = "alsa_input.pci".into();
        let text = toml::to_string_pretty(&c).unwrap();
        let back: Config = toml::from_str(&text).unwrap();
        assert_eq!(back.output, OutputMode::Paste);
        assert!(back.paste_shift);
        assert_eq!(back.language, "ru");
        assert_eq!(back.microphone, "alsa_input.pci");
        assert_eq!(back.api_key, "secret");
    }
    #[test]
    fn partial_toml_uses_defaults() {
        // Missing fields fall back to Default thanks to #[serde(default)].
        let back: Config = toml::from_str("api_key = \"k\"\n").unwrap();
        assert!(back.model.is_empty());
        assert!(back.base_url.is_empty());
        assert_eq!(back.output, OutputMode::Preview);
    }
    #[test]
    fn env_override_never_reaches_the_persisted_config() {
        // The env key overlays the effective config for the run, but must not
        // mutate the value that gets serialised to disk.
        let file_cfg = Config::default(); // empty api_key on disk
        let effective = file_cfg.with_overrides(Some("env-secret".into()), None);
        assert_eq!(effective.api_key, "env-secret", "effective picks up env");
        assert_eq!(file_cfg.api_key, "", "the saved config is untouched");
        let serialised = toml::to_string_pretty(&file_cfg).unwrap();
        assert!(
            !serialised.contains("env-secret"),
            "env key must not be written to disk: {serialised}"
        );
    }
}
