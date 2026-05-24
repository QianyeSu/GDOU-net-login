#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

mod config;
mod srun;

use crate::config::{load_config, load_password, save_config, store_password, AppConfig};
use crate::srun::SrunClient;
use anyhow::{Context, Result};
use serde::Serialize;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::thread;
use std::time::Duration;
use tauri::{Emitter, State};
use tokio::runtime::Runtime;

#[derive(Debug, Clone, serde::Deserialize, Serialize)]
struct UiConfig {
    portal_url: String,
    probe_url: String,
    username: String,
    password: String,
    ac_id: String,
    retry_seconds: u64,
    auto_query_acid: bool,
    auto_reconnect: bool,
    os_name: String,
    device_name: String,
    n: u32,
    login_type: u32,
}

#[derive(Debug, Clone, Serialize)]
struct UiResponse {
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    config: Option<UiConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    online: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    auto_reconnect: Option<bool>,
}

#[derive(Default)]
struct AppState {
    watcher: Mutex<Option<WatcherHandle>>,
}

struct WatcherHandle {
    stop: Arc<AtomicBool>,
    join: thread::JoinHandle<()>,
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    tauri::Builder::default()
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            load_state_cmd,
            save_config_cmd,
            login_cmd,
            logout_cmd,
            check_status_cmd,
            set_auto_reconnect_cmd
        ])
        .run(tauri::generate_context!())
        .context("failed to run tauri app")
}

#[tauri::command]
fn load_state_cmd() -> Result<UiResponse, String> {
    let cfg = load_config().unwrap_or_default();
    let password = load_password(&cfg).unwrap_or_default();
    Ok(UiResponse {
        status: "Ready".to_string(),
        config: Some(UiConfig {
            portal_url: cfg.portal_url,
            probe_url: cfg.probe_url,
            username: cfg.username,
            password,
            ac_id: cfg.ac_id.map(|v| v.to_string()).unwrap_or_default(),
            retry_seconds: cfg.retry_seconds,
            auto_query_acid: cfg.auto_query_acid,
            auto_reconnect: cfg.auto_reconnect,
            os_name: cfg.os_name,
            device_name: cfg.device_name,
            n: cfg.n,
            login_type: cfg.login_type,
        }),
        online: None,
        auto_reconnect: Some(cfg.auto_reconnect),
    })
}

#[tauri::command]
fn save_config_cmd(config: UiConfig) -> Result<UiResponse, String> {
    persist_config(&config).map_err(|err| format!("{err:#}"))?;
    Ok(UiResponse {
        status: "Saved".to_string(),
        config: None,
        online: None,
        auto_reconnect: Some(config.auto_reconnect),
    })
}

#[tauri::command]
fn login_cmd(config: UiConfig) -> Result<UiResponse, String> {
    let (cfg, password) = persist_config(&config).map_err(|err| format!("{err:#}"))?;
    let result = login_once(cfg.clone(), password).map_err(|err| format!("{err:#}"))?;
    Ok(UiResponse {
        status: result.0,
        config: None,
        online: result.1,
        auto_reconnect: Some(cfg.auto_reconnect),
    })
}

#[tauri::command]
fn logout_cmd(config: UiConfig) -> Result<UiResponse, String> {
    let (cfg, password) = persist_config(&config).map_err(|err| format!("{err:#}"))?;
    let result = logout_once(cfg, password).map_err(|err| format!("{err:#}"))?;
    Ok(UiResponse {
        status: result.0,
        config: None,
        online: result.1,
        auto_reconnect: Some(config.auto_reconnect),
    })
}

#[tauri::command]
fn check_status_cmd(config: UiConfig) -> Result<UiResponse, String> {
    let cfg = build_config(&config).map_err(|err| format!("{err:#}"))?;
    let online = status_once(cfg).map_err(|err| format!("{err:#}"))?;
    Ok(UiResponse {
        status: if online { "online" } else { "offline" }.to_string(),
        config: None,
        online: Some(online),
        auto_reconnect: Some(config.auto_reconnect),
    })
}

#[tauri::command]
fn set_auto_reconnect_cmd(
    app: tauri::AppHandle,
    state: State<AppState>,
    config: UiConfig,
    enabled: bool,
) -> Result<UiResponse, String> {
    if enabled {
        start_auto_reconnect(&app, &state, config.clone()).map_err(|err| format!("{err:#}"))?;
    } else {
        stop_auto_reconnect(&state);
    }
    Ok(UiResponse {
        status: if enabled {
            "Auto reconnect started".to_string()
        } else {
            "Auto reconnect stopped".to_string()
        },
        config: None,
        online: None,
        auto_reconnect: Some(enabled),
    })
}

fn persist_config(config: &UiConfig) -> Result<(AppConfig, String)> {
    let cfg = build_config(config)?;
    save_config(&cfg)?;
    if !config.password.is_empty() {
        store_password(&cfg, &config.password)?;
    }
    Ok((cfg, config.password.clone()))
}

fn build_config(config: &UiConfig) -> Result<AppConfig> {
    let mut cfg = AppConfig {
        portal_url: config.portal_url.trim().to_string(),
        probe_url: config.probe_url.trim().to_string(),
        username: config.username.trim().to_string(),
        ac_id: None,
        retry_seconds: config.retry_seconds.max(5),
        auto_query_acid: config.auto_query_acid,
        auto_reconnect: config.auto_reconnect,
        os_name: config.os_name.trim().to_string(),
        device_name: config.device_name.trim().to_string(),
        n: config.n,
        login_type: config.login_type,
    };
    if cfg.portal_url.is_empty() {
        anyhow::bail!("portal url is required");
    }
    if cfg.probe_url.is_empty() {
        anyhow::bail!("probe url is required");
    }
    if cfg.username.is_empty() {
        anyhow::bail!("username is required");
    }
    let ac_id = config.ac_id.trim();
    if !ac_id.is_empty() {
        cfg.ac_id = Some(ac_id.parse().context("invalid ac_id")?);
    }
    Ok(cfg)
}

fn login_once(cfg: AppConfig, password: String) -> Result<(String, Option<bool>)> {
    Runtime::new()?.block_on(async move {
        let client = SrunClient::new(cfg)?;
        let message = client.login(&password).await?;
        Ok((message, Some(true)))
    })
}

fn logout_once(cfg: AppConfig, password: String) -> Result<(String, Option<bool>)> {
    Runtime::new()?.block_on(async move {
        let client = SrunClient::new(cfg)?;
        let message = client.logout(&password).await?;
        Ok((message, Some(false)))
    })
}

fn status_once(cfg: AppConfig) -> Result<bool> {
    Runtime::new()?.block_on(async move {
        let client = SrunClient::new(cfg)?;
        client.probe_online().await
    })
}

fn start_auto_reconnect(
    app: &tauri::AppHandle,
    state: &State<AppState>,
    config: UiConfig,
) -> Result<()> {
    let mut guard = state.watcher.lock().unwrap();
    if guard.is_some() {
        return Ok(());
    }

    let (cfg, password) = persist_config(&config)?;
    if password.is_empty() {
        anyhow::bail!("password is required");
    }

    let stop = Arc::new(AtomicBool::new(false));
    let thread_stop = stop.clone();
    let handle = app.clone();
    let join = thread::spawn(move || auto_reconnect_loop(handle, cfg, password, thread_stop));
    *guard = Some(WatcherHandle { stop, join });
    Ok(())
}

fn stop_auto_reconnect(state: &State<AppState>) {
    let mut guard = state.watcher.lock().unwrap();
    if let Some(watcher) = guard.take() {
        watcher.stop.store(true, Ordering::Relaxed);
        let _ = watcher.join.join();
    }
}

fn auto_reconnect_loop(
    app: tauri::AppHandle,
    mut cfg: AppConfig,
    password: String,
    stop: Arc<AtomicBool>,
) {
    let rt = match Runtime::new() {
        Ok(rt) => rt,
        Err(err) => {
            let _ = app.emit(
                "status",
                UiResponse {
                    status: format!("Runtime failed: {err:#}"),
                    config: None,
                    online: Some(false),
                    auto_reconnect: Some(false),
                },
            );
            return;
        }
    };

    while !stop.load(Ordering::Relaxed) {
        let result = rt.block_on(async {
            let client = SrunClient::new(cfg.clone())?;
            let online = client.probe_online().await?;
            if online {
                return Ok::<_, anyhow::Error>((true, "online".to_string()));
            }
            if cfg.auto_query_acid {
                if let Some(ac_id) = client.query_acid().await? {
                    cfg.ac_id = Some(ac_id);
                    save_config(&cfg)?;
                }
            }
            let login_client = SrunClient::new(cfg.clone())?;
            let message = login_client.login(&password).await?;
            Ok((true, message))
        });

        match result {
            Ok((online, message)) => {
                let _ = app.emit(
                    "status",
                    UiResponse {
                        status: format!("Auto reconnect: {message}"),
                        config: None,
                        online: Some(online),
                        auto_reconnect: Some(true),
                    },
                );
            }
            Err(err) => {
                let _ = app.emit(
                    "status",
                    UiResponse {
                        status: format!("Auto reconnect failed: {err:#}"),
                        config: None,
                        online: Some(false),
                        auto_reconnect: Some(true),
                    },
                );
            }
        }

        let interval = Duration::from_secs(cfg.retry_seconds.max(5));
        let mut slept = Duration::ZERO;
        while slept < interval {
            if stop.load(Ordering::Relaxed) {
                return;
            }
            thread::sleep(Duration::from_secs(1));
            slept += Duration::from_secs(1);
        }
    }
}
