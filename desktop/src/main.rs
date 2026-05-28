#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

mod config;
mod srun;

use crate::config::{
    default_online_check_seconds, load_config, load_password, save_config, store_password,
    AppConfig,
};
use crate::srun::{validate_request_url, NetworkDiagnostics, RouteInfo, SrunClient, UrlPurpose};
use anyhow::{Context, Result};
use serde::Serialize;
use std::net::IpAddr;
use std::process::Command;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::thread;
use std::time::{Duration, Instant};
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
const AUTH_COOLDOWN: Duration = Duration::from_secs(10);
const COMMAND_COOLDOWN: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, serde::Deserialize, Serialize)]
struct UiConfig {
    portal_url: String,
    probe_url: String,
    username: String,
    password: String,
    ac_id: String,
    user_ip: String,
    retry_seconds: u64,
    online_check_seconds: u64,
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
    auth_busy: AtomicBool,
    last_auth_at: Mutex<Option<Instant>>,
    last_command_at: Mutex<Option<Instant>>,
}

struct WatcherHandle {
    stop: Arc<AtomicBool>,
    join: thread::JoinHandle<()>,
}

struct AuthRunGuard<'a> {
    state: &'a AppState,
}

impl Drop for AuthRunGuard<'_> {
    fn drop(&mut self) {
        self.state.auth_busy.store(false, Ordering::Relaxed);
    }
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
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
                    WindowEvent::Resized(_) if window.is_minimized().unwrap_or(false) => {
                        let _ = window.hide();
                    }
                    _ => {}
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            load_state_cmd,
            save_config_cmd,
            detect_portal_cmd,
            diagnose_cmd,
            reconnect_self_test_cmd,
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
    let password = if cfg.username.trim().is_empty() {
        String::new()
    } else {
        load_password(&cfg).unwrap_or_default()
    };
    Ok(UiResponse {
        status: "Ready".to_string(),
        config: Some(ui_config_from_app_config(&cfg, password)),
        online: None,
        auto_reconnect: Some(cfg.auto_reconnect),
        startup_enabled: Some(is_startup_enabled().unwrap_or(false)),
    })
}

#[tauri::command]
fn save_config_cmd(state: State<'_, AppState>, config: UiConfig) -> Result<UiResponse, String> {
    throttle_command(&state)?;
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
async fn detect_portal_cmd(
    state: State<'_, AppState>,
    config: UiConfig,
) -> Result<UiResponse, String> {
    throttle_command(&state)?;
    let mut probe_config = config.clone();
    probe_config.portal_url.clear();
    probe_config.ac_id.clear();
    let cfg = build_config_without_username(&probe_config).map_err(|err| format!("{err:#}"))?;

    let (cfg, detected_config) = enrich_config_from_probe_inner(cfg, false)
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
            if detected_config.ac_id.is_empty() {
                ""
            } else {
                " / ac_id"
            },
            if detected_config.user_ip.is_empty() {
                ""
            } else {
                " / IP"
            }
        ),
        config: Some(detected_config),
        online: None,
        auto_reconnect: Some(config.auto_reconnect),
        startup_enabled: None,
    })
}

#[tauri::command]
async fn diagnose_cmd(state: State<'_, AppState>, config: UiConfig) -> Result<UiResponse, String> {
    throttle_command(&state)?;
    let mut cfg = build_config_without_username(&config).map_err(|err| format!("{err:#}"))?;
    merge_saved_login_context(&mut cfg);

    let client = SrunClient::new(cfg.clone()).map_err(|err| format!("{err:#}"))?;
    let (detected, traces) = client
        .probe_portal_detailed()
        .await
        .map_err(|err| format!("{err:#}"))?;
    let mut detected_config = None;

    if cfg.portal_url.trim().is_empty() {
        if let Some(portal_url) = detected.portal_url.clone() {
            let (normalized, parsed_ac_id, parsed_user_ip) =
                normalize_portal_url(&portal_url).map_err(|err| format!("{err:#}"))?;
            cfg.portal_url = normalized;
            if cfg.ac_id.is_none() {
                cfg.ac_id = parsed_ac_id;
            }
            if cfg.user_ip.is_none() {
                cfg.user_ip = parsed_user_ip;
            }
            detected_config = Some(ui_config_from_app_config(&cfg, String::new()));
        }
    }
    if cfg.ac_id.is_none() {
        cfg.ac_id = detected.ac_id;
    }
    if cfg.user_ip.is_none() {
        cfg.user_ip = detected.user_ip;
    }
    if detected_config.is_none()
        && (detected.ac_id.is_some() || detected.user_ip.is_some() || detected.portal_url.is_some())
    {
        detected_config = Some(ui_config_from_app_config(&cfg, String::new()));
    }

    let online = match SrunClient::new(cfg.clone()) {
        Ok(client) => client.probe_online().await.unwrap_or(false),
        Err(_) => false,
    };
    let client = SrunClient::new(cfg.clone()).map_err(|err| format!("{err:#}"))?;
    let network = client.network_diagnostics();
    let local_ip = client
        .local_ip()
        .ok()
        .map(|ip| ip.to_string())
        .unwrap_or_else(|| "未获取".to_string());
    let effective_user_ip = client
        .effective_user_ip()
        .ok()
        .map(|ip| ip.to_string())
        .unwrap_or_else(|| "-".to_string());
    let challenge = if cfg.username.trim().is_empty() {
        "未测试：账号为空".to_string()
    } else if cfg.portal_url.trim().is_empty() || cfg.ac_id.is_none() {
        "未测试：缺少 Portal 或 ac_id".to_string()
    } else {
        match SrunClient::new(cfg.clone()) {
            Ok(client) => match client.diagnose_challenge().await {
                Ok(text) => text,
                Err(err) => format!("失败：{err:#}"),
            },
            Err(err) => format!("失败：{err:#}"),
        }
    };

    let watcher_running = is_auto_reconnect_running(&state);
    let saved_user_ip = cfg
        .user_ip
        .map(|ip| ip.to_string())
        .unwrap_or_else(|| "-".to_string());
    let conclusion = match (online, watcher_running) {
        (true, true) => "已在线，自动重连守护运行中",
        (true, false) => "已在线，自动重连守护未运行",
        (false, true) => "当前未在线或状态接口不可达，自动重连守护运行中",
        (false, false) => "当前未在线或状态接口不可达，自动重连守护未运行",
    };
    let ip_note = if local_ip != "未获取" && saved_user_ip != "-" && local_ip != saved_user_ip {
        format!(
            "\n提示：保存的客户端 IP 与当前校园网 IP 不一致；登录会优先使用当前 IP {}，保存值仅作为兜底。",
            local_ip
        )
    } else {
        String::new()
    };
    let vpn_note = format_network_diagnostics(&network);

    let status = format!(
        "诊断\n结论：{}\nPortal：{}\nac_id：{}\n登录使用 IP：{}\n保存的客户端 IP：{}\n当前校园网 IP：{}{}\nrad_user_info：{}\n自动重连守护：{}\nVPN/代理：{}\nChallenge：{}\n{}",
        conclusion,
        empty_dash(cfg.portal_url.trim()),
        cfg.ac_id
            .map(|id| id.to_string())
            .unwrap_or_else(|| "-".to_string()),
        effective_user_ip,
        saved_user_ip,
        local_ip,
        ip_note,
        if online { "online" } else { "offline 或未能访问" },
        if watcher_running {
            "运行中"
        } else {
            "未运行"
        },
        vpn_note,
        challenge,
        format_probe_traces(&traces),
    );

    Ok(UiResponse {
        status,
        config: detected_config,
        online: Some(online),
        auto_reconnect: Some(config.auto_reconnect),
        startup_enabled: None,
    })
}

#[tauri::command]
async fn reconnect_self_test_cmd(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    config: UiConfig,
) -> Result<UiResponse, String> {
    let _auth_guard = begin_auth_run(&state)?;
    stop_auto_reconnect(&state);
    let (cfg, password) = persist_config(&config).map_err(|err| format!("{err:#}"))?;
    if password.is_empty() {
        return Err("password is required".to_string());
    }

    stop_auto_reconnect(&state);
    let _ = app.emit(
        "status",
        UiResponse {
            status: "重连自测：正在退出当前校园网会话".to_string(),
            config: None,
            online: None,
            auto_reconnect: Some(false),
            startup_enabled: None,
        },
    );

    let logout_status = match logout_once(cfg.clone(), password.clone()).await {
        Ok((message, _)) => message,
        Err(err) => format!("退出阶段返回：{err:#}"),
    };

    let _ = app.emit(
        "status",
        UiResponse {
            status: format!("重连自测：{logout_status}；开始直接登录验证"),
            config: None,
            online: Some(false),
            auto_reconnect: Some(false),
            startup_enabled: None,
        },
    );

    let (cfg, detected_config) = enrich_config_from_probe(cfg)
        .await
        .map_err(|err| format!("{err:#}"))?;
    let login_result = login_once(cfg.clone(), password.clone())
        .await
        .map_err(|err| format!("{err:#}"))?;

    if cfg.auto_reconnect {
        start_auto_reconnect_with_config(&app, &state, cfg.clone())
            .map_err(|err| format!("{err:#}"))?;
    }

    Ok(UiResponse {
        status: format!("重连自测完成：{}", login_result.0),
        config: detected_config,
        online: login_result.1,
        auto_reconnect: Some(cfg.auto_reconnect),
        startup_enabled: None,
    })
}

#[tauri::command]
async fn login_cmd(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    config: UiConfig,
) -> Result<UiResponse, String> {
    let _auth_guard = begin_auth_run(&state)?;
    let (cfg, password) = persist_config(&config).map_err(|err| format!("{err:#}"))?;
    let (cfg, detected_config) = enrich_config_from_probe(cfg)
        .await
        .map_err(|err| format!("{err:#}"))?;
    let result = login_once(cfg.clone(), password)
        .await
        .map_err(|err| format!("{err:#}"))?;
    if cfg.auto_reconnect {
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
    let _auth_guard = begin_auth_run(&state)?;
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
        online: if cfg.auto_reconnect { None } else { result.1 },
        auto_reconnect: Some(config.auto_reconnect),
        startup_enabled: None,
    })
}

#[tauri::command]
async fn check_status_cmd(
    state: State<'_, AppState>,
    config: UiConfig,
) -> Result<UiResponse, String> {
    throttle_command(&state)?;
    let cfg = build_config(&config).map_err(|err| format!("{err:#}"))?;
    let (cfg, detected_config) = enrich_config_from_probe(cfg)
        .await
        .map_err(|err| format!("{err:#}"))?;
    let online = status_once(cfg).await.map_err(|err| format!("{err:#}"))?;
    Ok(UiResponse {
        status: if online { "online" } else { "offline" }.to_string(),
        config: detected_config,
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
    let mut cfg = build_config(config)?;
    merge_saved_login_context(&mut cfg);
    save_config(&cfg)?;
    let password = if config.password.is_empty() {
        load_password(&cfg).unwrap_or_default()
    } else {
        store_password(&cfg, &config.password)?;
        config.password.clone()
    };
    Ok((cfg, password))
}

async fn enrich_config_from_probe(cfg: AppConfig) -> Result<(AppConfig, Option<UiConfig>)> {
    enrich_config_from_probe_inner(cfg, true).await
}

async fn enrich_config_from_probe_inner(
    mut cfg: AppConfig,
    use_saved_context: bool,
) -> Result<(AppConfig, Option<UiConfig>)> {
    if use_saved_context {
        merge_saved_login_context(&mut cfg);
    }

    let mut changed = refresh_current_user_ip(&mut cfg)?;

    if !cfg.portal_url.trim().is_empty() && cfg.ac_id.is_some() && cfg.user_ip.is_some() {
        if changed {
            save_config(&cfg)?;
            return Ok((
                cfg.clone(),
                Some(ui_config_from_app_config(&cfg, String::new())),
            ));
        }
        return Ok((cfg, None));
    }

    let client = SrunClient::new(cfg.clone())?;
    let detected = client.probe_portal_if_needed().await.unwrap_or_default();

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
    if cfg.ac_id.is_none() && cfg.auto_query_acid && !cfg.portal_url.trim().is_empty() {
        if let Some(ac_id) = SrunClient::new(cfg.clone())?.query_acid().await? {
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

fn merge_saved_login_context(cfg: &mut AppConfig) {
    let Ok(saved) = load_config() else {
        return;
    };
    if cfg.portal_url.trim().is_empty() && !saved.portal_url.trim().is_empty() {
        cfg.portal_url = saved.portal_url;
    }
    if cfg.ac_id.is_none() {
        cfg.ac_id = saved.ac_id;
    }
    if cfg.user_ip.is_none() {
        cfg.user_ip = saved.user_ip;
    }
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
        online_check_seconds: cfg.online_check_seconds,
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
    if !portal_url.trim().is_empty() {
        validate_request_url(&portal_url, UrlPurpose::Portal)?;
    }
    let mut cfg = AppConfig {
        portal_url,
        probe_url: config.probe_url.trim().to_string(),
        username: config.username.trim().to_string(),
        ac_id: parsed_ac_id,
        user_ip: parsed_user_ip,
        retry_seconds: config.retry_seconds.max(10),
        online_check_seconds: config
            .online_check_seconds
            .max(default_online_check_seconds()),
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
    validate_request_url(&cfg.probe_url, UrlPurpose::Probe)?;
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
    tokio::time::sleep(Duration::from_secs(2)).await;
    let online = client.probe_online().await.unwrap_or(false);
    let status = if online {
        format!("{message}; 断开请求已返回，但二次检测仍显示在线")
    } else {
        format!("{message}; 二次检测已离线")
    };
    Ok((status, Some(online)))
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

fn begin_auth_run(state: &AppState) -> Result<AuthRunGuard<'_>, String> {
    if state
        .auth_busy
        .compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed)
        .is_err()
    {
        return Err("上一轮登录或断开还在执行，请稍等几秒再试".to_string());
    }

    let now = Instant::now();
    let mut last = state.last_auth_at.lock().unwrap();
    if let Some(last_at) = *last {
        let elapsed = now.saturating_duration_since(last_at);
        if elapsed < AUTH_COOLDOWN {
            state.auth_busy.store(false, Ordering::Relaxed);
            let wait = AUTH_COOLDOWN.saturating_sub(elapsed).as_secs().max(1);
            return Err(format!("认证请求过于频繁，请 {wait} 秒后再试"));
        }
    }
    *last = Some(now);
    drop(last);

    Ok(AuthRunGuard { state })
}

fn throttle_command(state: &AppState) -> Result<(), String> {
    let now = Instant::now();
    let mut last = state.last_command_at.lock().unwrap();
    if let Some(last_at) = *last {
        let elapsed = now.saturating_duration_since(last_at);
        if elapsed < COMMAND_COOLDOWN {
            let wait = COMMAND_COOLDOWN.saturating_sub(elapsed).as_secs().max(1);
            return Err(format!("操作过于频繁，请 {wait} 秒后再试"));
        }
    }
    *last = Some(now);
    Ok(())
}

fn refresh_current_user_ip(cfg: &mut AppConfig) -> Result<bool> {
    let client = SrunClient::new(cfg.clone())?;
    let Ok(current_ip) = client.local_ip() else {
        return Ok(false);
    };
    if cfg.user_ip != Some(current_ip) {
        cfg.user_ip = Some(current_ip);
        return Ok(true);
    }
    Ok(false)
}

fn format_network_diagnostics(info: &NetworkDiagnostics) -> String {
    let mut parts = Vec::new();
    if let Some(proxy) = &info.system_proxy {
        parts.push(format!("系统代理已开启({proxy})，SRUN 请求仍按程序直连"));
    } else {
        parts.push("系统代理未开启".to_string());
    }

    if info.tun_detected {
        parts.push("检测到可能的 TUN/虚拟网卡".to_string());
    }

    if let Some(route) = &info.default_route {
        parts.push(format!("默认出口：{}", format_route(route)));
    }
    if let Some(route) = &info.portal_route {
        if route.virtual_route {
            parts.push(format!(
                "Portal 路由可能经过 VPN/TUN：{}；建议将校园网网段设置为 DIRECT",
                format_route(route)
            ));
        } else {
            parts.push(format!("Portal 路由：{}", format_route(route)));
        }
    }

    parts.join("；")
}

fn format_route(route: &RouteInfo) -> String {
    let source = route
        .source
        .map(|ip| ip.to_string())
        .unwrap_or_else(|| "-".to_string());
    let next_hop = route.next_hop.as_deref().unwrap_or("-");
    format!(
        "{} source={} next_hop={}",
        route.interface, source, next_hop
    )
}

fn is_auto_reconnect_running(state: &State<AppState>) -> bool {
    state.watcher.lock().unwrap().is_some()
}

fn empty_dash(value: &str) -> &str {
    if value.is_empty() {
        "-"
    } else {
        value
    }
}

fn format_probe_traces(traces: &[crate::srun::PortalProbeTrace]) -> String {
    let mut lines = vec!["探测明细：".to_string()];
    if traces.is_empty() {
        lines.push("- 无探测记录".to_string());
        return lines.join("\n");
    }

    for trace in traces {
        let status = trace
            .status
            .map(|status| status.to_string())
            .unwrap_or_else(|| "-".to_string());
        let found =
            if trace.portal_url.is_some() || trace.ac_id.is_some() || trace.user_ip.is_some() {
                format!(
                    "命中 Portal={} ac_id={} IP={}",
                    trace.portal_url.as_deref().unwrap_or("-"),
                    trace
                        .ac_id
                        .map(|id| id.to_string())
                        .unwrap_or_else(|| "-".to_string()),
                    trace
                        .user_ip
                        .map(|ip| ip.to_string())
                        .unwrap_or_else(|| "-".to_string())
                )
            } else if let Some(err) = &trace.error {
                format!("失败 {err}")
            } else if let Some(location) = &trace.location {
                format!("重定向但未解析：{}", shorten(location, 90))
            } else {
                "未发现认证信息".to_string()
            };
        lines.push(format!("- {} [{}] {}", trace.target, status, found));
    }

    lines.join("\n")
}

fn shorten(text: &str, max_chars: usize) -> String {
    let mut chars = text.chars();
    let shortened: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{shortened}...")
    } else {
        shortened
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
                    cfg.ac_id = Some(ac_id);
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

        let interval = if last_online == Some(true) {
            Duration::from_secs(cfg.online_check_seconds.max(default_online_check_seconds()))
        } else {
            Duration::from_secs(cfg.retry_seconds.max(10))
        };
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
