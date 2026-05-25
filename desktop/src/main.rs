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
use std::net::IpAddr;
use std::process::Command;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::thread;
use std::time::Duration;
use tauri::menu::MenuBuilder;
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Emitter, Manager, State, WindowEvent};
use tokio::runtime::Runtime;

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

const REPOSITORY_URL: &str = "https://github.com/QianyeSu/GDOU-net-login";
const RELEASES_URL: &str = "https://github.com/QianyeSu/GDOU-net-login/releases";
const STARTUP_ENTRY_NAME: &str = "GDOU Net Login";

#[derive(Debug, Clone, serde::Deserialize, Serialize)]
struct UiConfig {
    portal_url: String,
    probe_url: String,
    username: String,
    password: String,
    ac_id: String,
    user_ip: String,
    retry_seconds: u64,
    auto_query_acid: bool,
    auto_reconnect: bool,
    accept_terms: bool,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    startup_enabled: Option<bool>,
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
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            show_main_window(app);
        }))
        .manage(AppState::default())
        .setup(|app| {
            setup_tray(app)?;
            start_saved_auto_reconnect(app);
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == "main" {
                match event {
                    WindowEvent::CloseRequested { api, .. } => {
                        api.prevent_close();
                        let _ = window.hide();
                    }
                    WindowEvent::Resized(_) => {
                        if window.is_minimized().unwrap_or(false) {
                            let _ = window.hide();
                        }
                    }
                    _ => {}
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            load_state_cmd,
            save_config_cmd,
            detect_portal_cmd,
            login_cmd,
            logout_cmd,
            check_status_cmd,
            set_auto_reconnect_cmd,
            set_startup_enabled_cmd,
            open_repository_cmd,
            open_releases_cmd
        ])
        .run(tauri::generate_context!())
        .context("failed to run tauri app")
}

fn setup_tray(app: &mut tauri::App) -> tauri::Result<()> {
    let menu = MenuBuilder::new(app)
        .text("show", "显示主窗口")
        .separator()
        .text("github", "GitHub 仓库")
        .text("updates", "检查更新")
        .separator()
        .text("quit", "退出")
        .build()?;

    let mut tray = TrayIconBuilder::new()
        .menu(&menu)
        .show_menu_on_left_click(false)
        .tooltip("GDOU Net Login")
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show" => show_main_window(app),
            "github" => {
                let _ = open_url(REPOSITORY_URL);
            }
            "updates" => {
                let _ = open_url(RELEASES_URL);
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| match event {
            TrayIconEvent::DoubleClick {
                button: MouseButton::Left,
                ..
            }
            | TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } => show_main_window(tray.app_handle()),
            _ => {}
        });

    if let Some(icon) = app.default_window_icon().cloned() {
        tray = tray.icon(icon);
    }

    tray.build(app)?;
    Ok(())
}

fn start_saved_auto_reconnect(app: &mut tauri::App) {
    let Ok(cfg) = load_config() else {
        return;
    };
    if !cfg.auto_reconnect || cfg.username.trim().is_empty() {
        return;
    }
    if load_password(&cfg).unwrap_or_default().is_empty() {
        return;
    }

    let state = app.state::<AppState>();
    if let Err(err) = start_auto_reconnect_with_config(app.handle(), &state, cfg) {
        tracing::debug!("failed to start saved auto reconnect: {err:#}");
    }
}

fn show_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

#[tauri::command]
fn open_repository_cmd() -> Result<(), String> {
    open_url(REPOSITORY_URL)
}

#[tauri::command]
fn open_releases_cmd() -> Result<(), String> {
    open_url(RELEASES_URL)
}

fn open_url(url: &str) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    let result = Command::new("cmd")
        .args(["/C", "start", "", url])
        .creation_flags(CREATE_NO_WINDOW)
        .spawn();

    #[cfg(target_os = "macos")]
    let result = Command::new("open").arg(url).spawn();

    #[cfg(all(unix, not(target_os = "macos")))]
    let result = Command::new("xdg-open").arg(url).spawn();

    result
        .map(|_| ())
        .map_err(|err| format!("failed to open url: {err}"))
}

#[tauri::command]
fn load_state_cmd() -> Result<UiResponse, String> {
    let cfg = load_config().unwrap_or_default();
    let password = load_password(&cfg).unwrap_or_default();
    Ok(UiResponse {
        status: "Ready".to_string(),
        config: Some(ui_config_from_app_config(&cfg, password)),
        online: None,
        auto_reconnect: Some(cfg.auto_reconnect),
        startup_enabled: Some(is_startup_enabled().unwrap_or(false)),
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
        startup_enabled: None,
    })
}

#[tauri::command]
async fn detect_portal_cmd(config: UiConfig) -> Result<UiResponse, String> {
    let mut probe_config = config.clone();
    probe_config.portal_url.clear();
    probe_config.ac_id.clear();
    probe_config.user_ip.clear();
    let cfg = build_config_without_username(&probe_config).map_err(|err| format!("{err:#}"))?;

    let (cfg, detected_config) = enrich_config_from_probe(cfg)
        .await
        .map_err(|err| format!("{err:#}"))?;
    let mut detected_config =
        detected_config.unwrap_or_else(|| ui_config_from_app_config(&cfg, String::new()));

    if detected_config.portal_url.trim().is_empty() && !config.portal_url.trim().is_empty() {
        let fallback = build_config_without_username(&config).map_err(|err| format!("{err:#}"))?;
        detected_config = ui_config_from_app_config(&fallback, String::new());
    }

    if detected_config.portal_url.trim().is_empty() {
        return Err(
            "未探测到 Portal 地址；请确认当前连接的是校园网，并处于未登录或认证页可跳转状态"
                .to_string(),
        );
    }

    Ok(UiResponse {
        status: format!(
            "已探测 Portal{}{}",
            detected_config
                .ac_id
                .is_empty()
                .then_some("")
                .unwrap_or(" / ac_id"),
            detected_config
                .user_ip
                .is_empty()
                .then_some("")
                .unwrap_or(" / IP")
        ),
        config: Some(detected_config),
        online: None,
        auto_reconnect: Some(config.auto_reconnect),
        startup_enabled: None,
    })
}

#[tauri::command]
async fn login_cmd(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    config: UiConfig,
) -> Result<UiResponse, String> {
    let (cfg, password) = persist_config(&config).map_err(|err| format!("{err:#}"))?;
    let (cfg, detected_config) = enrich_config_from_probe(cfg)
        .await
        .map_err(|err| format!("{err:#}"))?;
    let result = login_once(cfg.clone(), password)
        .await
        .map_err(|err| format!("{err:#}"))?;
    if cfg.auto_reconnect {
        stop_auto_reconnect(&state);
        start_auto_reconnect_with_config(&app, &state, cfg.clone())
            .map_err(|err| format!("{err:#}"))?;
    }
    Ok(UiResponse {
        status: result.0,
        config: detected_config,
        online: result.1,
        auto_reconnect: Some(cfg.auto_reconnect),
        startup_enabled: None,
    })
}

#[tauri::command]
async fn logout_cmd(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    config: UiConfig,
) -> Result<UiResponse, String> {
    stop_auto_reconnect(&state);
    let (cfg, password) = persist_config(&config).map_err(|err| format!("{err:#}"))?;
    let result = logout_once(cfg.clone(), password)
        .await
        .map_err(|err| format!("{err:#}"))?;
    if cfg.auto_reconnect {
        start_auto_reconnect_with_config(&app, &state, cfg.clone())
            .map_err(|err| format!("{err:#}"))?;
    }
    Ok(UiResponse {
        status: if cfg.auto_reconnect {
            format!("{}; 自动重连已继续守护", result.0)
        } else {
            result.0
        },
        config: None,
        online: result.1,
        auto_reconnect: Some(config.auto_reconnect),
        startup_enabled: None,
    })
}

#[tauri::command]
async fn check_status_cmd(config: UiConfig) -> Result<UiResponse, String> {
    let cfg = build_config(&config).map_err(|err| format!("{err:#}"))?;
    let online = status_once(cfg).await.map_err(|err| format!("{err:#}"))?;
    Ok(UiResponse {
        status: if online { "online" } else { "offline" }.to_string(),
        config: None,
        online: Some(online),
        auto_reconnect: Some(config.auto_reconnect),
        startup_enabled: None,
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
        startup_enabled: None,
    })
}

#[tauri::command]
fn set_startup_enabled_cmd(enabled: bool) -> Result<UiResponse, String> {
    set_startup_enabled(enabled).map_err(|err| format!("{err:#}"))?;
    Ok(UiResponse {
        status: if enabled {
            "已开启开机启动".to_string()
        } else {
            "已关闭开机启动".to_string()
        },
        config: None,
        online: None,
        auto_reconnect: None,
        startup_enabled: Some(enabled),
    })
}

fn persist_config(config: &UiConfig) -> Result<(AppConfig, String)> {
    let cfg = build_config(config)?;
    save_config(&cfg)?;
    let password = if config.password.is_empty() {
        load_password(&cfg).unwrap_or_default()
    } else {
        store_password(&cfg, &config.password)?;
        config.password.clone()
    };
    Ok((cfg, password))
}

async fn enrich_config_from_probe(mut cfg: AppConfig) -> Result<(AppConfig, Option<UiConfig>)> {
    if !cfg.portal_url.trim().is_empty() && cfg.ac_id.is_some() && cfg.user_ip.is_some() {
        return Ok((cfg, None));
    }

    let client = SrunClient::new(cfg.clone())?;
    let detected = client.probe_portal_if_needed().await.unwrap_or_default();
    let mut changed = false;

    if cfg.portal_url.trim().is_empty() {
        if let Some(portal_url) = detected.portal_url {
            let (normalized, parsed_ac_id, parsed_user_ip) = normalize_portal_url(&portal_url)?;
            cfg.portal_url = normalized;
            if cfg.ac_id.is_none() {
                cfg.ac_id = parsed_ac_id;
            }
            if cfg.user_ip.is_none() {
                cfg.user_ip = parsed_user_ip;
            }
            changed = true;
        }
    }
    if cfg.ac_id.is_none() {
        if let Some(ac_id) = detected.ac_id {
            cfg.ac_id = Some(ac_id);
            changed = true;
        }
    }
    if cfg.user_ip.is_none() {
        if let Some(user_ip) = detected.user_ip {
            cfg.user_ip = Some(user_ip);
            changed = true;
        }
    }

    if changed {
        save_config(&cfg)?;
        return Ok((
            cfg.clone(),
            Some(ui_config_from_app_config(&cfg, String::new())),
        ));
    }

    Ok((cfg, None))
}

fn ui_config_from_app_config(cfg: &AppConfig, password: String) -> UiConfig {
    UiConfig {
        portal_url: cfg.portal_url.clone(),
        probe_url: cfg.probe_url.clone(),
        username: cfg.username.clone(),
        password,
        ac_id: cfg.ac_id.map(|v| v.to_string()).unwrap_or_default(),
        user_ip: cfg.user_ip.map(|v| v.to_string()).unwrap_or_default(),
        retry_seconds: cfg.retry_seconds,
        auto_query_acid: cfg.auto_query_acid,
        auto_reconnect: cfg.auto_reconnect,
        accept_terms: cfg.accept_terms,
        os_name: cfg.os_name.clone(),
        device_name: cfg.device_name.clone(),
        n: cfg.n,
        login_type: cfg.login_type,
    }
}

fn build_config(config: &UiConfig) -> Result<AppConfig> {
    build_config_inner(config, true)
}

fn build_config_without_username(config: &UiConfig) -> Result<AppConfig> {
    build_config_inner(config, false)
}

fn build_config_inner(config: &UiConfig, require_username: bool) -> Result<AppConfig> {
    let (portal_url, parsed_ac_id, parsed_user_ip) = parse_optional_portal_url(&config.portal_url)?;
    let mut cfg = AppConfig {
        portal_url,
        probe_url: config.probe_url.trim().to_string(),
        username: config.username.trim().to_string(),
        ac_id: parsed_ac_id,
        user_ip: parsed_user_ip,
        retry_seconds: config.retry_seconds.max(5),
        auto_query_acid: config.auto_query_acid,
        auto_reconnect: config.auto_reconnect,
        accept_terms: true,
        os_name: config.os_name.trim().to_string(),
        device_name: config.device_name.trim().to_string(),
        n: config.n,
        login_type: config.login_type,
    };
    if cfg.probe_url.is_empty() {
        anyhow::bail!("probe url is required");
    }
    if require_username && cfg.username.is_empty() {
        anyhow::bail!("username is required");
    }
    let ac_id = config.ac_id.trim();
    if !ac_id.is_empty() {
        cfg.ac_id = Some(ac_id.parse().context("invalid ac_id")?);
    }
    let user_ip = config.user_ip.trim();
    if !user_ip.is_empty() {
        cfg.user_ip = Some(user_ip.parse::<IpAddr>().context("invalid client ip")?);
    }
    Ok(cfg)
}

fn is_startup_enabled() -> Result<bool> {
    #[cfg(target_os = "windows")]
    {
        let output = run_reg_command(&[
            "query",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
            "/v",
            STARTUP_ENTRY_NAME,
        ])?;
        if !output.status.success() {
            return Ok(false);
        }
        let exe = std::env::current_exe()
            .context("failed to resolve current executable")?
            .to_string_lossy()
            .to_string();
        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(stdout
            .to_ascii_lowercase()
            .contains(&exe.to_ascii_lowercase()))
    }

    #[cfg(not(target_os = "windows"))]
    {
        Ok(false)
    }
}

fn set_startup_enabled(enabled: bool) -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        if enabled {
            let exe = std::env::current_exe().context("failed to resolve current executable")?;
            let value = format!("\"{}\"", exe.display());
            let output = run_reg_command(&[
                "add",
                r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
                "/v",
                STARTUP_ENTRY_NAME,
                "/t",
                "REG_SZ",
                "/d",
                &value,
                "/f",
            ])?;
            if !output.status.success() {
                anyhow::bail!(
                    "failed to enable startup: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        } else {
            let output = run_reg_command(&[
                "delete",
                r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
                "/v",
                STARTUP_ENTRY_NAME,
                "/f",
            ])?;
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                let stdout = String::from_utf8_lossy(&output.stdout);
                let text = format!("{stdout}\n{stderr}");
                if !text.contains("找不到") && !text.to_ascii_lowercase().contains("unable to find")
                {
                    anyhow::bail!("failed to disable startup: {text}");
                }
            }
        }
        Ok(())
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = enabled;
        anyhow::bail!("startup is only supported on Windows")
    }
}

#[cfg(target_os = "windows")]
fn run_reg_command(args: &[&str]) -> Result<std::process::Output> {
    Command::new("reg")
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .context("failed to run reg.exe")
}

fn parse_optional_portal_url(input: &str) -> Result<(String, Option<u32>, Option<IpAddr>)> {
    let raw = input.trim();
    if raw.is_empty() {
        return Ok((String::new(), None, None));
    }
    normalize_portal_url(raw)
}

fn normalize_portal_url(input: &str) -> Result<(String, Option<u32>, Option<IpAddr>)> {
    let raw = input.trim();
    if raw.is_empty() {
        anyhow::bail!("portal url is required");
    }

    let parsed = reqwest::Url::parse(raw).context("invalid portal url")?;
    let ac_id = parsed
        .query_pairs()
        .find(|(key, _)| key == "ac_id")
        .and_then(|(_, value)| value.parse::<u32>().ok());
    let user_ip = parsed
        .query_pairs()
        .find(|(key, _)| key == "wlanuserip")
        .and_then(|(_, value)| value.parse::<IpAddr>().ok());

    let host = parsed.host_str().context("portal url missing host")?;
    let mut base = format!("{}://{}", parsed.scheme(), host);
    if let Some(port) = parsed.port() {
        base.push(':');
        base.push_str(&port.to_string());
    }

    Ok((base, ac_id, user_ip))
}

async fn login_once(cfg: AppConfig, password: String) -> Result<(String, Option<bool>)> {
    if password.is_empty() {
        anyhow::bail!("password is required");
    }
    let client = SrunClient::new(cfg)?;
    let message = client.login(&password).await?;
    let online = client.probe_online().await.unwrap_or(false);
    Ok((message, Some(online)))
}

async fn logout_once(cfg: AppConfig, password: String) -> Result<(String, Option<bool>)> {
    let client = SrunClient::new(cfg)?;
    let message = client.logout(&password).await?;
    Ok((message, Some(false)))
}

async fn status_once(cfg: AppConfig) -> Result<bool> {
    let client = SrunClient::new(cfg)?;
    client.probe_online().await
}

fn start_auto_reconnect(
    app: &tauri::AppHandle,
    state: &State<AppState>,
    config: UiConfig,
) -> Result<()> {
    let (cfg, _) = persist_config(&config)?;
    start_auto_reconnect_with_config(app, state, cfg)
}

fn start_auto_reconnect_with_config(
    app: &tauri::AppHandle,
    state: &State<AppState>,
    cfg: AppConfig,
) -> Result<()> {
    let mut guard = state.watcher.lock().unwrap();
    if guard.is_some() {
        return Ok(());
    }

    let password = load_password(&cfg).unwrap_or_default();
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
        drop(watcher.join);
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
                    startup_enabled: None,
                },
            );
            return;
        }
    };

    let mut last_online: Option<bool> = None;
    let mut last_error: Option<String> = None;

    while !stop.load(Ordering::Relaxed) {
        let result = rt.block_on(async {
            let (next_cfg, detected_config) = enrich_config_from_probe(cfg.clone()).await?;
            cfg = next_cfg;
            if detected_config.is_some() {
                let _ = app.emit(
                    "status",
                    UiResponse {
                        status: "Auto reconnect: 已更新 Portal 配置".to_string(),
                        config: detected_config,
                        online: None,
                        auto_reconnect: Some(true),
                        startup_enabled: None,
                    },
                );
            }

            let client = SrunClient::new(cfg.clone())?;
            let online = client.probe_online().await?;
            if online {
                return Ok::<_, anyhow::Error>((true, "online".to_string()));
            }
            if cfg.ac_id.is_none() && cfg.auto_query_acid {
                if let Some(ac_id) = client.query_acid().await? {
                    if cfg.ac_id != Some(ac_id) {
                        cfg.ac_id = Some(ac_id);
                        save_config(&cfg)?;
                    }
                }
            }
            let login_client = SrunClient::new(cfg.clone())?;
            let message = login_client.login(&password).await?;
            Ok((true, message))
        });

        match result {
            Ok((online, message)) => {
                let should_emit =
                    last_online != Some(online) || message != "online" || last_error.is_some();
                last_online = Some(online);
                last_error = None;
                if should_emit {
                    let _ = app.emit(
                        "status",
                        UiResponse {
                            status: format!("Auto reconnect: {message}"),
                            config: None,
                            online: Some(online),
                            auto_reconnect: Some(true),
                            startup_enabled: None,
                        },
                    );
                }
            }
            Err(err) => {
                let message = format!("{err:#}");
                let should_emit =
                    last_online != Some(false) || last_error.as_deref() != Some(&message);
                last_online = Some(false);
                last_error = Some(message.clone());
                if should_emit {
                    let _ = app.emit(
                        "status",
                        UiResponse {
                            status: format!("Auto reconnect failed: {message}"),
                            config: None,
                            online: Some(false),
                            auto_reconnect: Some(true),
                            startup_enabled: None,
                        },
                    );
                }
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

#[cfg(test)]
mod tests {
    use super::normalize_portal_url;

    #[test]
    fn normalizes_full_portal_success_url_and_extracts_acid() {
        let (portal, ac_id, user_ip) =
            normalize_portal_url("http://portal.example/srun_portal_success?ac_id=5&theme=pro")
                .unwrap();

        assert_eq!(portal, "http://portal.example");
        assert_eq!(ac_id, Some(5));
        assert_eq!(user_ip, None);
    }

    #[test]
    fn extracts_wlan_user_ip_from_portal_url() {
        let (_, _, user_ip) = normalize_portal_url(
            "http://portal.example/srun_portal_success?ac_id=5&wlanuserip=10.0.0.23",
        )
        .unwrap();

        assert_eq!(user_ip.unwrap().to_string(), "10.0.0.23");
    }

    #[test]
    fn preserves_explicit_port_without_query() {
        let (portal, ac_id, user_ip) = normalize_portal_url("http://portal.example:8080").unwrap();

        assert_eq!(portal, "http://portal.example:8080");
        assert_eq!(ac_id, None);
        assert_eq!(user_ip, None);
    }
}
