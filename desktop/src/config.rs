use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::net::IpAddr;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub portal_url: String,
    pub probe_url: String,
    pub username: String,
    pub ac_id: Option<u32>,
    pub user_ip: Option<IpAddr>,
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
            probe_url: "http://www.msftconnecttest.com/connecttest.txt".to_string(),
            username: String::new(),
            ac_id: None,
            user_ip: None,
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
    let cfg = serde_json::from_str(&text).context("failed to parse config json")?;
    Ok(cfg)
}

pub fn save_config(cfg: &AppConfig) -> Result<()> {
    let path = config_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).context("failed to create config directory")?;
    }
    let text = serde_json::to_string_pretty(cfg).context("failed to serialize config")?;
    fs::write(&path, text)
        .with_context(|| format!("failed to write config: {}", path.display()))?;
    Ok(())
}

pub fn store_password(cfg: &AppConfig, password: &str) -> Result<()> {
    let entry = keyring::Entry::new(keyring_service(), &cfg.username)?;
    entry
        .set_password(password)
        .context("failed to store password")
}

pub fn load_password(cfg: &AppConfig) -> Result<String> {
    let entry = keyring::Entry::new(keyring_service(), &cfg.username)?;
    entry
        .get_password()
        .context("failed to load password from keyring")
}

fn keyring_service() -> &'static str {
    "gdou-net-login"
}
