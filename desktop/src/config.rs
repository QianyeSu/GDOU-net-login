use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::net::IpAddr;
use std::path::PathBuf;
use tempfile::NamedTempFile;

const DEFAULT_PROBE_URL: &str = "http://www.msftconnecttest.com/connecttest.txt";
const LEGACY_GOOGLE_PROBE_URL: &str = "http://connectivitycheck.gstatic.com/generate_204";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub portal_url: String,
    pub probe_url: String,
    pub username: String,
    pub ac_id: Option<u32>,
    pub user_ip: Option<IpAddr>,
    #[serde(default)]
    pub bind_ip: Option<IpAddr>,
    pub retry_seconds: u64,
    #[serde(default = "default_online_check_seconds")]
    pub online_check_seconds: u64,
    pub auto_query_acid: bool,
    #[serde(default)]
    pub auto_reconnect: bool,
    #[serde(default)]
    pub accept_terms: bool,
    pub os_name: String,
    pub device_name: String,
    pub n: u32,
    pub login_type: u32,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            portal_url: String::new(),
            probe_url: DEFAULT_PROBE_URL.to_string(),
            username: String::new(),
            ac_id: None,
            user_ip: None,
            bind_ip: None,
            retry_seconds: 15,
            online_check_seconds: default_online_check_seconds(),
            auto_query_acid: true,
            auto_reconnect: true,
            accept_terms: true,
            os_name: std::env::consts::OS.to_string(),
            device_name: std::env::consts::OS.to_string(),
            n: 200,
            login_type: 1,
        }
    }
}

pub fn default_online_check_seconds() -> u64 {
    60
}

pub fn project_dirs() -> Result<ProjectDirs> {
    ProjectDirs::from("cn", "gdou", "gdou-net-login").context("failed to resolve config directory")
}

pub fn config_path() -> Result<PathBuf> {
    let dirs = project_dirs()?;
    Ok(dirs.config_dir().join("config.json"))
}

pub fn load_config() -> Result<AppConfig> {
    let path = config_path()?;
    let text = fs::read_to_string(&path)
        .with_context(|| format!("failed to read config: {}", path.display()))?;
    let mut cfg: AppConfig = serde_json::from_str(&text).context("failed to parse config json")?;
    migrate_config(&mut cfg);
    Ok(cfg)
}

fn migrate_config(cfg: &mut AppConfig) {
    if cfg.probe_url.trim() == LEGACY_GOOGLE_PROBE_URL {
        cfg.probe_url = DEFAULT_PROBE_URL.to_string();
    }
}

pub fn save_config(cfg: &AppConfig) -> Result<()> {
    let path = config_path()?;
    let parent = path
        .parent()
        .context("config path has no parent directory")?;
    fs::create_dir_all(parent).context("failed to create config directory")?;
    let text = serde_json::to_string_pretty(cfg).context("failed to serialize config")?;
    write_config_atomically(&path, &text)
}

fn write_config_atomically(path: &std::path::Path, text: &str) -> Result<()> {
    let parent = path
        .parent()
        .context("config path has no parent directory")?;
    let mut temp = NamedTempFile::new_in(parent).with_context(|| {
        format!(
            "failed to create temporary config file in {}",
            parent.display()
        )
    })?;
    temp.write_all(text.as_bytes())
        .with_context(|| format!("failed to write temporary config: {}", path.display()))?;
    temp.as_file()
        .sync_all()
        .with_context(|| format!("failed to flush temporary config: {}", path.display()))?;
    temp.persist(path).map_err(|err| {
        anyhow::anyhow!("failed to replace config {}: {}", path.display(), err.error)
    })?;
    Ok(())
}

pub fn store_password(cfg: &AppConfig, password: &str) -> Result<()> {
    if cfg.username.trim().is_empty() {
        return Ok(());
    }
    let entry = keyring::Entry::new(keyring_service(), &cfg.username)?;
    entry
        .set_password(password)
        .context("failed to store password")
}

pub fn load_password(cfg: &AppConfig) -> Result<String> {
    if cfg.username.trim().is_empty() {
        return Ok(String::new());
    }
    let entry = keyring::Entry::new(keyring_service(), &cfg.username)?;
    entry
        .get_password()
        .context("failed to load password from keyring")
}

fn keyring_service() -> &'static str {
    "gdou-net-login"
}

#[cfg(test)]
mod tests {
    use super::write_config_atomically;
    use std::fs;

    #[test]
    fn atomic_write_replaces_existing_config_without_partial_content() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.json");
        fs::write(&path, r#"{"old":true}"#).unwrap();

        write_config_atomically(&path, r#"{"new":true}"#).unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), r#"{"new":true}"#);
    }
}
