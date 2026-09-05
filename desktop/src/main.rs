#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod config;
mod srun;

use crate::config::{
    default_online_check_seconds, load_config, load_password, save_config, store_password,
    AppConfig,
};
use crate::srun::{
    validate_request_url, NetworkDiagnostics, PortalOnlineStatus, RouteInfo, SrunClient, UrlPurpose,
};
use anyhow::{Context, Result};
use encoding_rs::GBK;
use serde::Serialize;
use std::fs;
use std::net::IpAddr;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use sysinfo::Networks;
use tauri::menu::MenuBuilder;
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Emitter, Manager, State, WindowEvent};
use tokio::runtime::Runtime;

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

#[cfg(target_os = "windows")]
use windows_sys::Win32::{
    Foundation::{ERROR_BUFFER_OVERFLOW, ERROR_SUCCESS},
    NetworkManagement::{
        IpHelper::{
            GetAdaptersAddresses, GAA_FLAG_SKIP_ANYCAST, GAA_FLAG_SKIP_MULTICAST,
            IP_ADAPTER_ADDRESSES_LH,
        },
        Ndis::IfOperStatusUp,
    },
    Networking::WinSock::{AF_INET, SOCKADDR_IN},
    System::Registry::{
        RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW,
        RegSetValueExW, HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_EXPAND_SZ,
        REG_OPTION_NON_VOLATILE, REG_SZ,
    },
};

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

const REPOSITORY_URL: &str = "https://github.com/QianyeSu/GDOU-net-login";
const RELEASES_URL: &str = "https://github.com/QianyeSu/GDOU-net-login/releases";
const STARTUP_ENTRY_NAME: &str = "GDOU Net Login";
const STARTUP_SCRIPT_NAME: &str = "GDOU Net Login.vbs";
const AUTH_COOLDOWN: Duration = Duration::from_secs(10);
const COMMAND_COOLDOWN: Duration = Duration::from_secs(2);
const NETWORK_INTERFACES_CACHE_TTL: Duration = Duration::from_secs(8);
const NETWORK_COMMAND_TIMEOUT: Duration = Duration::from_secs(4);

#[derive(Debug, Clone, serde::Deserialize, Serialize)]
struct UiConfig {
    portal_url: String,
    probe_url: String,
    username: String,
    password: String,
    ac_id: String,
    user_ip: String,
    bind_ip: String,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    last_reconnect_at_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
struct NetworkMonitorSnapshot {
    timestamp_ms: u128,
    adapters: Vec<NetworkAdapterSnapshot>,
}

#[derive(Debug, Clone, Serialize)]
struct NetworkAdapterSnapshot {
    name: String,
    kind: String,
    state: String,
    received_per_refresh: u64,
    transmitted_per_refresh: u64,
    received_bytes: u64,
    transmitted_bytes: u64,
    total_bytes: u64,
    is_active: bool,
    is_virtual: bool,
    is_tun: bool,
    is_easy_connect: bool,
    is_clash: bool,
    is_likely_srun_exit: bool,
    recommendation: String,
}

#[derive(Debug, Clone, Serialize)]
struct NetworkInterfaceInfo {
    interface_alias: String,
    interface_description: String,
    ip: String,
    prefix_length: Option<u8>,
    gateway: Option<String>,
    dns: Vec<String>,
    is_up: bool,
    is_virtual: bool,
    is_likely_campus: bool,
    is_likely_vpn: bool,
    is_likely_tun: bool,
    is_likely_lan: bool,
    route_to_portal: bool,
    route_to_internet: bool,
    is_selected: bool,
    recommendation: String,
}

#[derive(Default)]
struct AppState {
    watcher: Mutex<Option<WatcherHandle>>,
    network_monitor: Mutex<Option<Networks>>,
    auth_busy: AtomicBool,
    last_auth_at: Mutex<Option<Instant>>,
    last_command_at: Mutex<Option<Instant>>,
    network_interfaces_cache: Mutex<Option<NetworkInterfaceCache>>,
}

struct NetworkInterfaceCache {
    created_at: Instant,
    items: Vec<NetworkInterfaceInfo>,
}

struct WatcherHandle {
    stop: Arc<AtomicBool>,
    join: thread::JoinHandle<()>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReconnectCycleState {
    Online,
    PausedForEasyConnect,
    WaitingForPortal,
    WaitingForNetwork,
}

struct AuthRunGuard<'a> {
    state: &'a AppState,
}

struct AdapterClassification {
    kind: String,
    is_virtual: bool,
    is_tun: bool,
    is_easy_connect: bool,
    is_clash: bool,
    is_likely_srun_exit: bool,
    recommendation: String,
}

#[derive(Debug, Clone, Default)]
struct ParsedInterface {
    interface_alias: String,
    interface_description: String,
    ip: String,
    gateway: Option<String>,
    dns: Vec<String>,
    is_up: bool,
}

#[derive(Debug, Clone, Default)]
struct NetworkInterfaceSummary {
    campus_candidates: Vec<String>,
    virtual_adapters: Vec<String>,
    selected_bind_ip: Option<String>,
    selected_bind_ip_available: bool,
    has_easy_connect: bool,
    has_clash: bool,
    has_tun: bool,
    has_hypomux_like: bool,
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
            restore_saved_startup_entry();
            start_saved_auto_reconnect(app);
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == "main" {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    hide_main_window(window);
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            load_state_cmd,
            save_config_cmd,
            autosave_config_cmd,
            detect_portal_cmd,
            diagnose_cmd,
            reconnect_self_test_cmd,
            login_cmd,
            logout_cmd,
            check_status_cmd,
            set_auto_reconnect_cmd,
            set_startup_enabled_cmd,
            network_monitor_snapshot_cmd,
            list_network_interfaces_cmd,
            open_repository_cmd,
            open_releases_cmd,
            minimize_window_cmd,
            close_window_cmd,
            toggle_maximize_cmd,
            start_drag_cmd,
            start_resize_cmd
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
        let _ = app.emit("window-visibility", true);
    }
}

fn hide_main_window(window: &tauri::Window) {
    let _ = window.emit("window-visibility", false);
    let _ = window.hide();
}

#[tauri::command]
fn open_repository_cmd() -> Result<(), String> {
    open_url(REPOSITORY_URL)
}

#[tauri::command]
fn open_releases_cmd() -> Result<(), String> {
    open_url(RELEASES_URL)
}

#[tauri::command]
fn minimize_window_cmd(window: tauri::Window) {
    let _ = window.minimize();
}

#[tauri::command]
fn close_window_cmd(window: tauri::Window) {
    hide_main_window(&window);
}

#[tauri::command]
fn toggle_maximize_cmd(window: tauri::Window) {
    if let Ok(is_maximized) = window.is_maximized() {
        if is_maximized {
            let _ = window.unmaximize();
        } else {
            let _ = window.maximize();
        }
    }
}

#[tauri::command]
fn start_drag_cmd(window: tauri::Window) {
    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::Foundation::HWND;
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture;
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            SendMessageW, HTCAPTION, WM_NCLBUTTONDOWN,
        };

        if let Ok(hwnd) = window.hwnd() {
            unsafe {
                ReleaseCapture();
                SendMessageW(hwnd.0 as HWND, WM_NCLBUTTONDOWN, HTCAPTION as usize, 0);
            }
            return;
        }
    }

    let _ = window.start_dragging();
}

#[tauri::command]
fn start_resize_cmd(window: tauri::Window, direction: String) {
    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::Foundation::HWND;
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture;
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            SendMessageW, HTBOTTOM, HTBOTTOMLEFT, HTBOTTOMRIGHT, HTLEFT, HTRIGHT, HTTOP, HTTOPLEFT,
            HTTOPRIGHT, WM_NCLBUTTONDOWN,
        };

        let hit_test = match direction.as_str() {
            "left" => HTLEFT,
            "right" => HTRIGHT,
            "top" => HTTOP,
            "top-left" => HTTOPLEFT,
            "top-right" => HTTOPRIGHT,
            "bottom" => HTBOTTOM,
            "bottom-left" => HTBOTTOMLEFT,
            "bottom-right" => HTBOTTOMRIGHT,
            _ => 0,
        };

        if hit_test != 0 {
            if let Ok(hwnd) = window.hwnd() {
                unsafe {
                    ReleaseCapture();
                    SendMessageW(hwnd.0 as HWND, WM_NCLBUTTONDOWN, hit_test as usize, 0);
                }
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = (&window, &direction);
    }
}

#[tauri::command]
fn network_monitor_snapshot_cmd(state: State<'_, AppState>) -> NetworkMonitorSnapshot {
    let mut guard = state.network_monitor.lock().unwrap();
    let networks = guard.get_or_insert_with(Networks::new_with_refreshed_list);
    networks.refresh(true);

    let adapters = networks
        .iter()
        .filter_map(|(name, data)| {
            let received_per_refresh = data.received();
            let transmitted_per_refresh = data.transmitted();
            let received_bytes = data.total_received();
            let transmitted_bytes = data.total_transmitted();
            let total_bytes = received_bytes.saturating_add(transmitted_bytes);
            let state = format!("{:?}", data.operational_state());
            let is_active = state.eq_ignore_ascii_case("up") || total_bytes > 0;
            let classification = classify_network_adapter(name);

            if !is_active && total_bytes == 0 {
                return None;
            }

            Some(NetworkAdapterSnapshot {
                name: name.to_string(),
                kind: classification.kind,
                state,
                received_per_refresh,
                transmitted_per_refresh,
                received_bytes,
                transmitted_bytes,
                total_bytes,
                is_active,
                is_virtual: classification.is_virtual,
                is_tun: classification.is_tun,
                is_easy_connect: classification.is_easy_connect,
                is_clash: classification.is_clash,
                is_likely_srun_exit: classification.is_likely_srun_exit,
                recommendation: classification.recommendation,
            })
        })
        .collect();

    NetworkMonitorSnapshot {
        timestamp_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
        adapters,
    }
}

#[tauri::command]
async fn list_network_interfaces_cmd(
    state: State<'_, AppState>,
    force: Option<bool>,
) -> Result<Vec<NetworkInterfaceInfo>, String> {
    let force = force.unwrap_or(false);
    if !force {
        if let Some(cache) = state.network_interfaces_cache.lock().unwrap().as_ref() {
            if cache.created_at.elapsed() <= NETWORK_INTERFACES_CACHE_TTL {
                return Ok(cache.items.clone());
            }
        }
    }

    let items = tokio::task::spawn_blocking(list_network_interfaces)
        .await
        .map_err(|err| format!("网卡刷新任务异常：{err}"))?
        .map_err(|err| format!("{err:#}"))?;
    *state.network_interfaces_cache.lock().unwrap() = Some(NetworkInterfaceCache {
        created_at: Instant::now(),
        items: items.clone(),
    });
    Ok(items)
}

fn list_network_interfaces() -> Result<Vec<NetworkInterfaceInfo>> {
    #[cfg(target_os = "windows")]
    {
        let items = read_ipconfig_interfaces()?;
        let cfg = load_config().unwrap_or_default();
        Ok(build_network_interface_infos(
            items,
            cfg.bind_ip,
            None,
            None,
        ))
    }

    #[cfg(not(target_os = "windows"))]
    {
        Ok(Vec::new())
    }
}

#[cfg(target_os = "windows")]
fn read_ipconfig_interfaces() -> Result<Vec<ParsedInterface>> {
    match read_windows_adapter_interfaces() {
        Ok(items) if !items.is_empty() => return Ok(items),
        Ok(_) => {}
        Err(err) => tracing::debug!("Windows adapter API enumeration failed: {err:#}"),
    }

    let mut command = Command::new("ipconfig");
    command.arg("/all").creation_flags(CREATE_NO_WINDOW);
    let output = command_output_with_timeout(command, NETWORK_COMMAND_TIMEOUT)
        .context("failed to enumerate network interfaces")?;
    if !output.status.success() {
        anyhow::bail!("failed to enumerate network interfaces");
    }

    let text = decode_windows_command_output(&output.stdout);
    Ok(parse_ipconfig_interfaces(&text))
}

#[cfg(target_os = "windows")]
fn read_windows_adapter_interfaces() -> Result<Vec<ParsedInterface>> {
    let mut size = 16 * 1024u32;
    let mut buffer = vec![0u8; size as usize];
    let flags = GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST;

    let mut result = unsafe {
        GetAdaptersAddresses(
            AF_INET as u32,
            flags,
            std::ptr::null(),
            buffer.as_mut_ptr().cast::<IP_ADAPTER_ADDRESSES_LH>(),
            &mut size,
        )
    };

    if result == ERROR_BUFFER_OVERFLOW {
        buffer.resize(size as usize, 0);
        result = unsafe {
            GetAdaptersAddresses(
                AF_INET as u32,
                flags,
                std::ptr::null(),
                buffer.as_mut_ptr().cast::<IP_ADAPTER_ADDRESSES_LH>(),
                &mut size,
            )
        };
    }

    if result != ERROR_SUCCESS {
        anyhow::bail!("GetAdaptersAddresses failed with {result}");
    }

    let mut items = Vec::new();
    let mut adapter = buffer.as_ptr().cast::<IP_ADAPTER_ADDRESSES_LH>();
    while !adapter.is_null() {
        let adapter_ref = unsafe { &*adapter };
        let alias = wide_ptr_to_string(adapter_ref.FriendlyName)
            .or_else(|| ansi_ptr_to_string(adapter_ref.AdapterName))
            .unwrap_or_else(|| "网络接口".to_string());
        let description = wide_ptr_to_string(adapter_ref.Description).unwrap_or_default();
        let is_up = adapter_ref.OperStatus == IfOperStatusUp;
        let gateway = first_ipv4_from_gateway(adapter_ref.FirstGatewayAddress);
        let dns = dns_ipv4_list(adapter_ref.FirstDnsServerAddress);

        let mut address = adapter_ref.FirstUnicastAddress;
        while !address.is_null() {
            let address_ref = unsafe { &*address };
            if let Some(ip) = socket_address_to_ipv4(address_ref.Address.lpSockaddr) {
                items.push(ParsedInterface {
                    interface_alias: alias.clone(),
                    interface_description: description.clone(),
                    ip: ip.to_string(),
                    gateway: gateway.clone(),
                    dns: dns.clone(),
                    is_up,
                });
            }
            address = address_ref.Next;
        }

        adapter = adapter_ref.Next;
    }

    Ok(items)
}

#[cfg(target_os = "windows")]
fn first_ipv4_from_gateway(
    mut gateway: *mut windows_sys::Win32::NetworkManagement::IpHelper::IP_ADAPTER_GATEWAY_ADDRESS_LH,
) -> Option<String> {
    while !gateway.is_null() {
        let gateway_ref = unsafe { &*gateway };
        if let Some(ip) = socket_address_to_ipv4(gateway_ref.Address.lpSockaddr) {
            return Some(ip.to_string());
        }
        gateway = gateway_ref.Next;
    }
    None
}

#[cfg(target_os = "windows")]
fn dns_ipv4_list(
    mut dns: *mut windows_sys::Win32::NetworkManagement::IpHelper::IP_ADAPTER_DNS_SERVER_ADDRESS_XP,
) -> Vec<String> {
    let mut items = Vec::new();
    while !dns.is_null() {
        let dns_ref = unsafe { &*dns };
        if let Some(ip) = socket_address_to_ipv4(dns_ref.Address.lpSockaddr) {
            items.push(ip.to_string());
        }
        dns = dns_ref.Next;
    }
    items
}

#[cfg(target_os = "windows")]
fn socket_address_to_ipv4(
    sockaddr: *mut windows_sys::Win32::Networking::WinSock::SOCKADDR,
) -> Option<std::net::Ipv4Addr> {
    if sockaddr.is_null() {
        return None;
    }
    let sockaddr_in = unsafe { &*(sockaddr.cast::<SOCKADDR_IN>()) };
    if sockaddr_in.sin_family != AF_INET {
        return None;
    }
    let octets = unsafe {
        let bytes = sockaddr_in.sin_addr.S_un.S_un_b;
        [bytes.s_b1, bytes.s_b2, bytes.s_b3, bytes.s_b4]
    };
    Some(std::net::Ipv4Addr::from(octets))
}

#[cfg(target_os = "windows")]
fn wide_ptr_to_string(ptr: windows_sys::core::PWSTR) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    let mut len = 0usize;
    unsafe {
        while *ptr.add(len) != 0 {
            len += 1;
        }
        if len == 0 {
            return None;
        }
        Some(String::from_utf16_lossy(std::slice::from_raw_parts(
            ptr, len,
        )))
    }
}

#[cfg(target_os = "windows")]
fn ansi_ptr_to_string(ptr: windows_sys::core::PSTR) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    let mut len = 0usize;
    unsafe {
        while *ptr.add(len) != 0 {
            len += 1;
        }
        if len == 0 {
            return None;
        }
        Some(String::from_utf8_lossy(std::slice::from_raw_parts(ptr.cast::<u8>(), len)).into())
    }
}

#[cfg(target_os = "windows")]
pub(crate) fn command_output_with_timeout(
    mut command: Command,
    timeout: Duration,
) -> Result<std::process::Output> {
    let mut child = command
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .context("failed to start command")?;
    let started_at = Instant::now();

    loop {
        if child
            .try_wait()
            .context("failed to poll command")?
            .is_some()
        {
            return child
                .wait_with_output()
                .context("failed to read command output");
        }
        if started_at.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            anyhow::bail!("command timed out after {} seconds", timeout.as_secs());
        }
        thread::sleep(Duration::from_millis(40));
    }
}

#[cfg(target_os = "windows")]
pub(crate) fn decode_windows_command_output(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(text) if !text.contains('\u{fffd}') => text.to_string(),
        _ => {
            let (text, _, _) = GBK.decode(bytes);
            text.into_owned()
        }
    }
}

fn build_network_interface_infos(
    items: Vec<ParsedInterface>,
    selected_ip: Option<IpAddr>,
    portal_route: Option<&RouteInfo>,
    default_route: Option<&RouteInfo>,
) -> Vec<NetworkInterfaceInfo> {
    let mut interfaces = Vec::new();
    for item in items {
        let alias = item.interface_alias;
        let description = item.interface_description;
        let ip_text = item.ip;
        let Ok(ip) = ip_text.parse::<IpAddr>() else {
            continue;
        };
        if matches!(ip, IpAddr::V6(_)) {
            continue;
        }

        let prefix_length = None;
        let gateway = item.gateway;
        let dns = item.dns;
        let is_up = item.is_up;
        let classification = classify_network_interface(&alias, &description, ip);
        let route_to_portal = route_matches_interface(portal_route, &alias, ip);
        let route_to_internet = route_matches_interface(default_route, &alias, ip)
            || is_gateway_present(gateway.as_deref());
        let is_selected = selected_ip == Some(ip);
        let is_likely_lan = is_private_ip(ip);
        let is_likely_campus = is_up
            && !classification.is_virtual
            && is_gateway_present(gateway.as_deref())
            && matches!(ip, IpAddr::V4(ipv4) if ipv4.octets()[0] == 10);
        let recommendation = if is_selected {
            "当前选择的登录出口".to_string()
        } else if is_likely_campus {
            "可能是校园网".to_string()
        } else {
            classification.recommendation
        };

        interfaces.push(NetworkInterfaceInfo {
            interface_alias: alias,
            interface_description: description,
            ip: ip.to_string(),
            prefix_length,
            gateway,
            dns,
            is_up,
            is_virtual: classification.is_virtual,
            is_likely_campus,
            is_likely_vpn: classification.is_easy_connect || classification.is_clash,
            is_likely_tun: classification.is_tun,
            is_likely_lan,
            route_to_portal,
            route_to_internet,
            is_selected,
            recommendation,
        });
    }

    interfaces.sort_by_key(|iface| {
        (
            !iface.is_selected,
            !iface.is_likely_campus,
            iface.is_virtual,
            !iface.is_up,
            iface.interface_alias.clone(),
        )
    });
    interfaces
}

fn summarize_network_interfaces(selected_ip: Option<IpAddr>) -> NetworkInterfaceSummary {
    let items = read_network_interfaces_for_summary();
    let infos = build_network_interface_infos(items, selected_ip, None, None);
    let mut summary = NetworkInterfaceSummary {
        selected_bind_ip: selected_ip.map(|ip| ip.to_string()),
        ..NetworkInterfaceSummary::default()
    };

    for item in infos {
        let label = format!("{} / {}", item.interface_alias, item.ip);
        if item.is_likely_campus {
            summary.campus_candidates.push(label.clone());
        }
        if item.is_virtual {
            summary.virtual_adapters.push(format!(
                "{} / {} / {}",
                item.interface_alias, item.ip, item.recommendation
            ));
        }
        if item.is_selected {
            summary.selected_bind_ip_available = true;
        }

        let name =
            format!("{} {}", item.interface_alias, item.interface_description).to_ascii_lowercase();
        summary.has_easy_connect |= name.contains("easyconnect") || name.contains("sangfor");
        summary.has_clash |= name.contains("clash") || name.contains("mihomo");
        summary.has_tun |= item.is_likely_tun;
        summary.has_hypomux_like |= contains_any(&name, &["hypomux", "mux", "dispatch", "teaming"]);
    }

    summary.campus_candidates.truncate(4);
    summary.virtual_adapters.truncate(5);
    summary
}

fn route_matches_interface(route: Option<&RouteInfo>, alias: &str, ip: IpAddr) -> bool {
    let Some(route) = route else {
        return false;
    };
    route.source == Some(ip) || route.interface.eq_ignore_ascii_case(alias)
}

#[cfg(target_os = "windows")]
fn read_network_interfaces_for_summary() -> Vec<ParsedInterface> {
    read_ipconfig_interfaces().unwrap_or_default()
}

#[cfg(not(target_os = "windows"))]
fn read_network_interfaces_for_summary() -> Vec<ParsedInterface> {
    Vec::new()
}

#[cfg(target_os = "windows")]
fn easyconnect_is_active() -> bool {
    read_ipconfig_interfaces()
        .map(|items| has_active_easyconnect_adapter(&items))
        .unwrap_or(false)
}

#[cfg(not(target_os = "windows"))]
fn easyconnect_is_active() -> bool {
    false
}

fn has_active_easyconnect_adapter(items: &[ParsedInterface]) -> bool {
    items.iter().any(|item| {
        if !item.is_up {
            return false;
        }
        let Ok(ip) = item.ip.parse::<IpAddr>() else {
            return false;
        };
        classify_network_interface(&item.interface_alias, &item.interface_description, ip)
            .is_easy_connect
    })
}

fn classify_network_adapter(name: &str) -> AdapterClassification {
    classify_network_interface(name, "", IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED))
}

fn classify_network_interface(name: &str, description: &str, ip: IpAddr) -> AdapterClassification {
    let lower = format!("{name} {description}").to_ascii_lowercase();
    let is_proxy_ip = is_proxy_reserved_ip(ip) || is_easyconnect_ip(ip);
    let is_loopback_ip = matches!(ip, IpAddr::V4(ipv4) if ipv4.is_loopback());
    let is_link_local_ip =
        matches!(ip, IpAddr::V4(ipv4) if ipv4.octets()[0] == 169 && ipv4.octets()[1] == 254);
    let is_easy_connect = contains_any(&lower, &["easyconnect", "sangfor", "ssl vpn"]);
    let is_clash = contains_any(&lower, &["clash", "mihomo"]);
    let is_tun = contains_any(
        &lower,
        &[
            "tun",
            "tap",
            "wintun",
            "openvpn",
            "wireguard",
            "sing",
            "v2ray",
            "nekoray",
            "tailscale",
            "zerotier",
        ],
    );
    let is_loopback = is_loopback_ip || contains_any(&lower, &["loopback", "pseudo-interface"]);
    let is_virtual = is_easy_connect
        || is_clash
        || is_tun
        || is_proxy_ip
        || is_link_local_ip
        || is_loopback
        || contains_any(
            &lower,
            &[
                "virtual",
                "vmware",
                "hyper-v",
                "wsl",
                "oray",
                "npcap",
                "hypomux",
                "dispatch",
                "teaming",
                "bluetooth",
                "wi-fi direct",
            ],
        );
    let is_wlan = contains_any(&lower, &["wlan", "wi-fi", "wifi", "wireless"]);
    let is_ethernet = contains_any(&lower, &["ethernet", "以太网", "realtek", "gbe", "gigabit"]);
    let is_likely_srun_exit = !is_virtual && (is_wlan || is_ethernet);

    let kind = if is_easy_connect {
        "EasyConnect".to_string()
    } else if is_clash {
        "网络工具/虚拟网卡".to_string()
    } else if is_tun {
        "TUN/虚拟网卡".to_string()
    } else if is_loopback {
        "本机回环".to_string()
    } else if is_link_local_ip {
        "未分配有效地址".to_string()
    } else if is_wlan {
        "WLAN".to_string()
    } else if is_ethernet {
        "有线网卡".to_string()
    } else if is_virtual {
        "虚拟网卡".to_string()
    } else {
        "网络接口".to_string()
    };

    let recommendation = if is_likely_srun_exit {
        "可作为校园网登录出口候选".to_string()
    } else if is_easy_connect {
        "虚拟网卡，不建议用于 SRUN 登录".to_string()
    } else if is_loopback || is_link_local_ip {
        "无效登录出口，不建议用于校园网登录".to_string()
    } else if is_clash || is_tun || is_virtual {
        "网络工具/虚拟网卡，不建议用于校园网登录".to_string()
    } else {
        "仅用于流量展示".to_string()
    };

    AdapterClassification {
        kind,
        is_virtual,
        is_tun,
        is_easy_connect,
        is_clash,
        is_likely_srun_exit,
        recommendation,
    }
}

fn contains_any(value: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| value.contains(needle))
}

fn is_gateway_present(value: Option<&str>) -> bool {
    value
        .map(str::trim)
        .is_some_and(|value| !value.is_empty() && value != "0.0.0.0")
}

fn parse_ipconfig_interfaces(text: &str) -> Vec<ParsedInterface> {
    let mut interfaces = Vec::new();
    let mut current: Option<ParsedInterface> = None;
    let mut reading_dns = false;

    for raw_line in text.lines() {
        let line = raw_line.trim_end();
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if !line.starts_with(' ') && trimmed.ends_with(':') {
            if let Some(item) = current.take() {
                if !item.ip.is_empty() {
                    interfaces.push(item);
                }
            }
            let alias = trimmed.trim_end_matches(':').to_string();
            current = Some(ParsedInterface {
                interface_alias: alias,
                is_up: true,
                ..ParsedInterface::default()
            });
            reading_dns = false;
            continue;
        }

        let Some(item) = current.as_mut() else {
            continue;
        };

        if let Some((key, value)) = split_ipconfig_field(trimmed) {
            reading_dns = false;
            let key_lower = key.to_ascii_lowercase();
            if key_lower.contains("description") || key.contains("描述") {
                item.interface_description = value.to_string();
            } else if key_lower.contains("ipv4") || key.contains("IPv4") {
                item.ip = clean_ipconfig_value(value);
            } else if key_lower.contains("default gateway") || key.contains("默认网关") {
                item.gateway = Some(clean_ipconfig_value(value)).filter(|value| !value.is_empty());
            } else if key_lower.contains("dns servers") || key.contains("DNS 服务器") {
                let dns = clean_ipconfig_value(value);
                if !dns.is_empty() {
                    item.dns.push(dns);
                }
                reading_dns = true;
            } else if key_lower.contains("media state")
                && (value.to_ascii_lowercase().contains("disconnected")
                    || value.contains("已断开")
                    || value.contains("断开"))
            {
                item.is_up = false;
            }
        } else if reading_dns && looks_like_ipv4(trimmed) {
            item.dns.push(clean_ipconfig_value(trimmed));
        }
    }

    if let Some(item) = current {
        if !item.ip.is_empty() {
            interfaces.push(item);
        }
    }

    interfaces
}

fn split_ipconfig_field(line: &str) -> Option<(&str, &str)> {
    let (key, value) = line.split_once(':')?;
    Some((key.trim(), value.trim()))
}

fn clean_ipconfig_value(value: &str) -> String {
    value
        .split('(')
        .next()
        .unwrap_or(value)
        .trim()
        .trim_end_matches('.')
        .to_string()
}

fn looks_like_ipv4(value: &str) -> bool {
    value
        .split_whitespace()
        .next()
        .and_then(|candidate| candidate.parse::<std::net::Ipv4Addr>().ok())
        .is_some()
}

fn is_private_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let [a, b, _, _] = ip.octets();
            a == 10 || a == 172 && (16..=31).contains(&b) || a == 192 && b == 168
        }
        IpAddr::V6(_) => false,
    }
}

fn is_proxy_reserved_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let [a, b, _, _] = ip.octets();
            a == 198 && (b == 18 || b == 19)
        }
        IpAddr::V6(_) => false,
    }
}

fn is_easyconnect_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let [a, b, c, _] = ip.octets();
            a == 2 && b == 0 && c == 1
        }
        IpAddr::V6(_) => false,
    }
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
        last_reconnect_at_ms: cfg.last_reconnect_at_ms,
    })
}

#[tauri::command]
fn save_config_cmd(state: State<'_, AppState>, config: UiConfig) -> Result<UiResponse, String> {
    let _ = state;
    persist_config(&config).map_err(|err| format!("{err:#}"))?;
    Ok(UiResponse {
        status: "Saved".to_string(),
        config: None,
        online: None,
        auto_reconnect: Some(config.auto_reconnect),
        startup_enabled: None,
        last_reconnect_at_ms: None,
    })
}

#[tauri::command]
fn autosave_config_cmd(config: UiConfig) -> Result<UiResponse, String> {
    persist_config_allowing_empty_username(&config).map_err(|err| format!("{err:#}"))?;
    Ok(UiResponse {
        status: "Saved".to_string(),
        config: None,
        online: None,
        auto_reconnect: Some(config.auto_reconnect),
        startup_enabled: None,
        last_reconnect_at_ms: None,
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
        last_reconnect_at_ms: None,
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

    let online_status = match SrunClient::new(cfg.clone()) {
        Ok(client) => client.probe_online_status().await,
        Err(err) => Err(err),
    };
    let client = SrunClient::new(cfg.clone()).map_err(|err| format!("{err:#}"))?;
    let network = client.network_diagnostics();
    let interface_summary = summarize_network_interfaces(cfg.bind_ip);
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
    let selected_bind_ip = cfg
        .bind_ip
        .map(|ip| ip.to_string())
        .unwrap_or_else(|| "自动选择".to_string());
    let conclusion = match (online_status.as_ref(), watcher_running) {
        (Ok(PortalOnlineStatus::Online), true) => "已在线，自动重连守护运行中",
        (Ok(PortalOnlineStatus::Online), false) => "已在线，自动重连守护未运行",
        (Ok(PortalOnlineStatus::Offline), true) => "已确认离线，自动重连守护运行中",
        (Ok(PortalOnlineStatus::Offline), false) => "已确认离线，自动重连守护未运行",
        (_, true) => "Portal 状态暂时无法确认，自动重连守护运行中",
        (_, false) => "Portal 状态暂时无法确认，自动重连守护未运行",
    };
    let status_detail = match &online_status {
        Ok(PortalOnlineStatus::Online) => "online".to_string(),
        Ok(PortalOnlineStatus::Offline) => "offline".to_string(),
        Ok(PortalOnlineStatus::Unknown) => "unknown：Portal 返回了无法判定的状态".to_string(),
        Err(err) => format!("接口失败：{err:#}"),
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
    let interface_note = format_interface_summary(&interface_summary);

    let status = format!(
        "诊断\n结论：{}\nPortal：{}\nac_id：{}\n登录出口选择：{}\n登录使用 IP：{}\n保存的客户端 IP：{}\n当前校园网 IP：{}{}\nrad_user_info：{}\n自动重连守护：{}\n网络路径：{}\n网卡摘要：{}\nChallenge：{}\n{}",
        conclusion,
        empty_dash(cfg.portal_url.trim()),
        cfg.ac_id
            .map(|id| id.to_string())
            .unwrap_or_else(|| "-".to_string()),
        selected_bind_ip,
        effective_user_ip,
        saved_user_ip,
        local_ip,
        ip_note,
        status_detail,
        if watcher_running {
            "运行中"
        } else {
            "未运行"
        },
        vpn_note,
        interface_note,
        challenge,
        format_probe_traces(&traces),
    );

    Ok(UiResponse {
        status,
        config: detected_config,
        online: online_status_to_option(&online_status),
        auto_reconnect: Some(config.auto_reconnect),
        startup_enabled: None,
        last_reconnect_at_ms: None,
    })
}

#[tauri::command]
async fn reconnect_self_test_cmd(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    config: UiConfig,
) -> Result<UiResponse, String> {
    let _auth_guard = begin_auth_run(&state)?;
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
            auto_reconnect: None,
            startup_enabled: None,
            last_reconnect_at_ms: None,
        },
    );

    let result = async {
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
                auto_reconnect: None,
                startup_enabled: None,
                last_reconnect_at_ms: None,
            },
        );

        let (next_cfg, detected_config) = enrich_config_from_probe(cfg.clone())
            .await
            .map_err(|err| format!("{err:#}"))?;
        let login_result = login_once(next_cfg.clone(), password.clone())
            .await
            .map_err(|err| format!("{err:#}"))?;
        Ok::<_, String>((next_cfg, detected_config, login_result))
    }
    .await;

    match result {
        Ok((mut next_cfg, detected_config, login_result)) => {
            let reconnect_at_ms = record_last_reconnect(&mut next_cfg);
            if next_cfg.auto_reconnect {
                start_auto_reconnect_with_config(&app, &state, next_cfg.clone())
                    .map_err(|err| format!("{err:#}"))?;
            }

            Ok(UiResponse {
                status: format!("重连自测完成：{}", login_result.0),
                config: detected_config,
                online: login_result.1,
                auto_reconnect: Some(next_cfg.auto_reconnect),
                startup_enabled: None,
                last_reconnect_at_ms: Some(reconnect_at_ms),
            })
        }
        Err(err) => {
            if cfg.auto_reconnect {
                match start_auto_reconnect_with_config(&app, &state, cfg.clone()) {
                    Ok(()) => {
                        let _ = app.emit(
                            "status",
                            UiResponse {
                                status: format!("重连自测失败，自动重连已恢复：{err}"),
                                config: None,
                                online: None,
                                auto_reconnect: Some(true),
                                startup_enabled: None,
                                last_reconnect_at_ms: None,
                            },
                        );
                    }
                    Err(start_err) => {
                        return Err(format!("{err}; 自动重连恢复失败：{start_err:#}"));
                    }
                }
            }
            Err(err)
        }
    }
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
        last_reconnect_at_ms: None,
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
        last_reconnect_at_ms: None,
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
    let status_result = status_once(cfg).await;
    let (status, online) = match status_result {
        Ok(PortalOnlineStatus::Online) => ("online".to_string(), Some(true)),
        Ok(PortalOnlineStatus::Offline) => ("offline".to_string(), Some(false)),
        Ok(PortalOnlineStatus::Unknown) => (
            "unknown：Portal 返回了无法判定的状态，未执行重连".to_string(),
            None,
        ),
        Err(err) => (format!("Portal 状态接口失败：{err:#}"), None),
    };
    Ok(UiResponse {
        status,
        config: detected_config,
        online,
        auto_reconnect: Some(config.auto_reconnect),
        startup_enabled: None,
        last_reconnect_at_ms: None,
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
        last_reconnect_at_ms: None,
    })
}

#[tauri::command]
fn set_startup_enabled_cmd(enabled: bool) -> Result<UiResponse, String> {
    set_startup_enabled(enabled).map_err(|err| format!("{err:#}"))?;
    save_startup_preference(enabled).map_err(|err| format!("{err:#}"))?;
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
        last_reconnect_at_ms: None,
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

fn persist_config_allowing_empty_username(config: &UiConfig) -> Result<AppConfig> {
    let mut cfg = build_config_without_username(config)?;
    merge_saved_login_context(&mut cfg);
    save_config(&cfg)?;
    if !cfg.username.is_empty() && !config.password.is_empty() {
        store_password(&cfg, &config.password)?;
    }
    Ok(cfg)
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
    merge_saved_login_context_from(cfg, &saved);
}

fn merge_saved_login_context_from(cfg: &mut AppConfig, saved: &AppConfig) {
    if cfg.portal_url.trim().is_empty() && !saved.portal_url.trim().is_empty() {
        cfg.portal_url = saved.portal_url.clone();
    }
    if cfg.ac_id.is_none() {
        cfg.ac_id = saved.ac_id;
    }
    if cfg.user_ip.is_none() {
        cfg.user_ip = saved.user_ip;
    }
    if cfg.last_reconnect_at_ms.is_none() {
        cfg.last_reconnect_at_ms = saved.last_reconnect_at_ms;
    }
    // Do not restore saved bind_ip here. An empty bind_ip from the UI is an
    // explicit request to switch the login outlet back to automatic selection.
}

fn ui_config_from_app_config(cfg: &AppConfig, password: String) -> UiConfig {
    UiConfig {
        portal_url: cfg.portal_url.clone(),
        probe_url: cfg.probe_url.clone(),
        username: cfg.username.clone(),
        password,
        ac_id: cfg.ac_id.map(|v| v.to_string()).unwrap_or_default(),
        user_ip: cfg.user_ip.map(|v| v.to_string()).unwrap_or_default(),
        bind_ip: cfg.bind_ip.map(|v| v.to_string()).unwrap_or_default(),
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
    let startup_enabled = load_config()
        .map(|saved| saved.startup_enabled)
        .unwrap_or(false);
    let mut cfg = AppConfig {
        portal_url,
        probe_url: config.probe_url.trim().to_string(),
        username: config.username.trim().to_string(),
        ac_id: parsed_ac_id,
        user_ip: parsed_user_ip,
        bind_ip: None,
        retry_seconds: config.retry_seconds.max(15),
        online_check_seconds: config
            .online_check_seconds
            .max(default_online_check_seconds()),
        auto_query_acid: config.auto_query_acid,
        auto_reconnect: config.auto_reconnect,
        startup_enabled,
        last_reconnect_at_ms: None,
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
    let bind_ip = config.bind_ip.trim();
    if !bind_ip.is_empty() {
        cfg.bind_ip = Some(
            bind_ip
                .parse::<IpAddr>()
                .context("invalid login outlet ip")?,
        );
    }
    Ok(cfg)
}

fn is_startup_enabled() -> Result<bool> {
    #[cfg(target_os = "windows")]
    {
        let exe = std::env::current_exe().context("failed to resolve current executable")?;
        if let Some(value) = read_startup_value()? {
            if startup_value_matches_executable(&value, &exe) {
                return Ok(true);
            }
        }
        if let Some(script) = read_startup_script()? {
            return Ok(startup_script_matches_executable(&script, &exe));
        }
        Ok(false)
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
            let registry_result = write_startup_value(&value);
            let script_result = write_startup_script(&exe);
            if is_startup_enabled()? {
                if let Err(err) = registry_result {
                    tracing::debug!(
                        "registry startup entry unavailable; script startup is active: {err:#}"
                    );
                }
                if let Err(err) = script_result {
                    tracing::debug!(
                        "startup script unavailable; registry startup is active: {err:#}"
                    );
                }
                return Ok(());
            }
            let registry_error = registry_result
                .err()
                .map(|err| format!("注册表：{err:#}"))
                .unwrap_or_else(|| "注册表校验失败".to_string());
            let script_error = script_result
                .err()
                .map(|err| format!("启动文件夹：{err:#}"))
                .unwrap_or_else(|| "启动文件夹校验失败".to_string());
            anyhow::bail!("startup entry could not be verified ({registry_error}; {script_error})");
        } else {
            delete_startup_value()?;
            delete_disabled_startup_value()?;
            delete_startup_script()?;
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
const STARTUP_RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
#[cfg(target_os = "windows")]
const STARTUP_DISABLED_RUN_KEY: &str =
    r"Software\Microsoft\Windows\CurrentVersion\Run\LenovoDisabled";

#[cfg(target_os = "windows")]
fn startup_value_matches_executable(value: &str, exe: &std::path::Path) -> bool {
    let command = value.trim();
    let executable = if let Some(quoted) = command.strip_prefix('"') {
        quoted
            .split_once('"')
            .map(|(path, _)| path)
            .unwrap_or(quoted)
    } else {
        command.split_whitespace().next().unwrap_or_default()
    };
    let normalized_value = executable.to_ascii_lowercase();
    let normalized_exe = exe.to_string_lossy().trim().to_ascii_lowercase();
    normalized_value == normalized_exe
}

#[cfg(target_os = "windows")]
fn startup_script_matches_executable(script: &str, exe: &std::path::Path) -> bool {
    script
        .to_ascii_lowercase()
        .contains(&exe.to_string_lossy().trim().to_ascii_lowercase())
}

#[cfg(target_os = "windows")]
fn startup_folder_path() -> Result<PathBuf> {
    let appdata = std::env::var_os("APPDATA").context("APPDATA is not available")?;
    Ok(PathBuf::from(appdata)
        .join("Microsoft")
        .join("Windows")
        .join("Start Menu")
        .join("Programs")
        .join("Startup"))
}

#[cfg(target_os = "windows")]
fn startup_script_path() -> Result<PathBuf> {
    Ok(startup_folder_path()?.join(STARTUP_SCRIPT_NAME))
}

#[cfg(target_os = "windows")]
fn read_startup_script() -> Result<Option<String>> {
    let path = startup_script_path()?;
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => {
            return Err(err)
                .with_context(|| format!("failed to read startup script: {}", path.display()))
        }
    };
    if bytes.starts_with(&[0xFF, 0xFE]) {
        let utf16 = bytes[2..]
            .chunks_exact(2)
            .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
            .collect::<Vec<_>>();
        Ok(Some(String::from_utf16_lossy(&utf16)))
    } else {
        Ok(Some(String::from_utf8_lossy(&bytes).into_owned()))
    }
}

#[cfg(target_os = "windows")]
fn write_startup_script(exe: &std::path::Path) -> Result<()> {
    let folder = startup_folder_path()?;
    fs::create_dir_all(&folder)
        .with_context(|| format!("failed to create startup folder: {}", folder.display()))?;
    let escaped_exe = exe.to_string_lossy().replace('"', "\"\"");
    let script = format!(
        "Dim shell\r\nSet shell = CreateObject(\"WScript.Shell\")\r\n\
         shell.Run \"{escaped_exe}\", 0, False\r\n"
    );
    let mut bytes = vec![0xFF, 0xFE];
    for unit in script.encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    let path = folder.join(STARTUP_SCRIPT_NAME);
    fs::write(&path, bytes)
        .with_context(|| format!("failed to write startup script: {}", path.display()))
}

#[cfg(target_os = "windows")]
fn delete_startup_script() -> Result<()> {
    let path = startup_script_path()?;
    match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => {
            Err(err).with_context(|| format!("failed to remove startup script: {}", path.display()))
        }
    }
}

#[cfg(target_os = "windows")]
fn startup_registry_key_at(path: &str, access: u32, create: bool) -> Result<Option<HKEY>> {
    use std::os::windows::ffi::OsStrExt;

    let key_name: Vec<u16> = std::ffi::OsStr::new(path)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut key: HKEY = std::ptr::null_mut();
    let status = unsafe {
        if create {
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                key_name.as_ptr(),
                0,
                std::ptr::null_mut(),
                REG_OPTION_NON_VOLATILE,
                access,
                std::ptr::null(),
                &mut key,
                std::ptr::null_mut(),
            )
        } else {
            RegOpenKeyExW(HKEY_CURRENT_USER, key_name.as_ptr(), 0, access, &mut key)
        }
    };
    if status == ERROR_SUCCESS {
        Ok(Some(key))
    } else if !create && status == windows_sys::Win32::Foundation::ERROR_FILE_NOT_FOUND {
        Ok(None)
    } else {
        anyhow::bail!("failed to access startup registry key (Windows error {status})")
    }
}

#[cfg(target_os = "windows")]
fn startup_registry_key(access: u32, create: bool) -> Result<Option<HKEY>> {
    startup_registry_key_at(STARTUP_RUN_KEY, access, create)
}

#[cfg(target_os = "windows")]
fn startup_value_name() -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;

    std::ffi::OsStr::new(STARTUP_ENTRY_NAME)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

#[cfg(target_os = "windows")]
fn read_startup_value() -> Result<Option<String>> {
    let Some(key) = startup_registry_key(KEY_QUERY_VALUE, false)? else {
        return Ok(None);
    };
    let value_name = startup_value_name();
    let mut value_type = 0;
    let mut byte_len = 0;
    let status = unsafe {
        RegQueryValueExW(
            key,
            value_name.as_ptr(),
            std::ptr::null_mut(),
            &mut value_type,
            std::ptr::null_mut(),
            &mut byte_len,
        )
    };
    if status == windows_sys::Win32::Foundation::ERROR_FILE_NOT_FOUND {
        unsafe {
            RegCloseKey(key);
        }
        return Ok(None);
    }
    if status != ERROR_SUCCESS {
        unsafe {
            RegCloseKey(key);
        }
        anyhow::bail!("failed to query startup registry value (Windows error {status})");
    }
    if value_type != REG_SZ && value_type != REG_EXPAND_SZ {
        unsafe {
            RegCloseKey(key);
        }
        anyhow::bail!("startup registry value has unsupported type {value_type}");
    }

    let mut bytes = vec![0u8; byte_len as usize];
    let status = unsafe {
        RegQueryValueExW(
            key,
            value_name.as_ptr(),
            std::ptr::null_mut(),
            &mut value_type,
            bytes.as_mut_ptr(),
            &mut byte_len,
        )
    };
    unsafe {
        RegCloseKey(key);
    }
    if status != ERROR_SUCCESS {
        anyhow::bail!("failed to read startup registry value (Windows error {status})");
    }
    bytes.truncate(byte_len as usize);
    if !bytes.len().is_multiple_of(2) {
        anyhow::bail!("startup registry value has invalid UTF-16 data");
    }
    let utf16 = bytes
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
        .take_while(|unit| *unit != 0)
        .collect::<Vec<_>>();
    Ok(Some(String::from_utf16_lossy(&utf16)))
}

#[cfg(target_os = "windows")]
fn write_startup_value(value: &str) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;

    let Some(key) = startup_registry_key(KEY_SET_VALUE | KEY_QUERY_VALUE, true)? else {
        anyhow::bail!("failed to create startup registry key");
    };
    let value_name = startup_value_name();
    let value_data: Vec<u16> = std::ffi::OsStr::new(value)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let status = unsafe {
        RegSetValueExW(
            key,
            value_name.as_ptr(),
            0,
            REG_SZ,
            value_data.as_ptr().cast::<u8>(),
            (value_data.len() * std::mem::size_of::<u16>()) as u32,
        )
    };
    unsafe {
        RegCloseKey(key);
    }
    if status != ERROR_SUCCESS {
        anyhow::bail!("failed to write startup registry value (Windows error {status})");
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn delete_startup_value() -> Result<()> {
    delete_startup_value_at(STARTUP_RUN_KEY)
}

#[cfg(target_os = "windows")]
fn delete_disabled_startup_value() -> Result<()> {
    if let Err(err) = delete_startup_value_at(STARTUP_DISABLED_RUN_KEY) {
        tracing::debug!("ignored disabled startup cleanup failure: {err:#}");
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn delete_startup_value_at(path: &str) -> Result<()> {
    let Some(key) = startup_registry_key_at(path, KEY_SET_VALUE, false)? else {
        return Ok(());
    };
    let value_name = startup_value_name();
    let status = unsafe { RegDeleteValueW(key, value_name.as_ptr()) };
    unsafe {
        RegCloseKey(key);
    }
    if status == ERROR_SUCCESS || status == windows_sys::Win32::Foundation::ERROR_FILE_NOT_FOUND {
        return Ok(());
    }

    // Some OEM startup managers move the value to a private subkey and make
    // the direct delete call return ERROR_BAD_COMMAND. Keep reg.exe as a
    // narrow compatibility fallback for that case.
    let registry_path = format!(r"HKCU\{path}");
    let output = Command::new("reg")
        .args(["delete", &registry_path, "/v", STARTUP_ENTRY_NAME, "/f"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .context("failed to run reg.exe for startup cleanup")?;
    if output.status.success() {
        return Ok(());
    }

    anyhow::bail!(
        "failed to delete startup registry value (Windows error {status}): {}",
        decode_windows_command_output(&output.stderr)
    )
}

fn save_startup_preference(enabled: bool) -> Result<()> {
    let mut cfg = load_config().unwrap_or_default();
    cfg.startup_enabled = enabled;
    save_config(&cfg)
}

fn restore_saved_startup_entry() {
    let Ok(mut cfg) = load_config() else {
        return;
    };
    if cfg.startup_enabled {
        if let Err(err) = set_startup_enabled(true) {
            tracing::warn!("failed to restore startup entry: {err:#}");
        }
    } else if is_startup_enabled().unwrap_or(false) {
        cfg.startup_enabled = true;
        if let Err(err) = save_config(&cfg) {
            tracing::warn!("failed to save detected startup preference: {err:#}");
        }
    }
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
    validate_request_url(&base, UrlPurpose::Portal)?;

    Ok((base, ac_id, user_ip))
}

async fn login_once(cfg: AppConfig, password: String) -> Result<(String, Option<bool>)> {
    if password.is_empty() {
        anyhow::bail!("password is required");
    }
    let client = SrunClient::new(cfg)?;
    let message = client.login(&password).await?;
    let online_status = client.probe_online_status().await;
    let online = online_status_to_option(&online_status);
    Ok((message, online))
}

async fn logout_once(cfg: AppConfig, password: String) -> Result<(String, Option<bool>)> {
    let client = SrunClient::new(cfg)?;
    let message = client.logout(&password).await?;
    tokio::time::sleep(Duration::from_secs(2)).await;
    let online_status = client.probe_online_status().await;
    let online = online_status_to_option(&online_status);
    let status = match online_status {
        Ok(PortalOnlineStatus::Online) => {
            format!("{message}; 断开请求已返回，但二次检测仍显示在线")
        }
        Ok(PortalOnlineStatus::Offline) => format!("{message}; 二次检测已确认离线"),
        Ok(PortalOnlineStatus::Unknown) => {
            format!("{message}; Portal 返回了无法判定的状态")
        }
        Err(err) => format!("{message}; 二次状态检测失败：{err:#}"),
    };
    Ok((status, online))
}

async fn status_once(cfg: AppConfig) -> Result<PortalOnlineStatus> {
    let client = SrunClient::new(cfg)?;
    client.probe_online_status().await
}

fn online_status_to_option(status: &Result<PortalOnlineStatus>) -> Option<bool> {
    match status {
        Ok(PortalOnlineStatus::Online) => Some(true),
        Ok(PortalOnlineStatus::Offline) => Some(false),
        Ok(PortalOnlineStatus::Unknown) | Err(_) => None,
    }
}

fn current_timestamp_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}

fn record_last_reconnect(cfg: &mut AppConfig) -> u64 {
    let timestamp_ms = current_timestamp_ms();
    cfg.last_reconnect_at_ms = Some(timestamp_ms);
    if let Err(err) = save_config(cfg) {
        tracing::warn!("failed to persist last reconnect time: {err:#}");
    }
    timestamp_ms
}

fn is_bind_ip_fallback_error(message: &str) -> bool {
    message.contains("[bind_ip_fallback_unavailable]")
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
        let _ = watcher.join.join();
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
                "Portal 路由可能经过网络工具/TUN：{}；建议将校园网网段设置为 DIRECT",
                format_route(route)
            ));
        } else {
            parts.push(format!("Portal 路由：{}", format_route(route)));
        }
    }

    parts.join("；")
}

fn format_interface_summary(summary: &NetworkInterfaceSummary) -> String {
    let mut parts = Vec::new();
    match &summary.selected_bind_ip {
        Some(ip) if summary.selected_bind_ip_available => {
            parts.push(format!("已指定登录出口 {ip}"));
        }
        Some(ip) => {
            parts.push(format!("已指定登录出口 {ip}，但当前网卡列表未发现该 IP"));
        }
        None => parts.push("登录出口为自动选择".to_string()),
    }

    if summary.campus_candidates.is_empty() {
        parts.push("未发现明确校园网候选".to_string());
    } else {
        parts.push(format!(
            "校园网候选 {} 个：{}",
            summary.campus_candidates.len(),
            summary.campus_candidates.join("、")
        ));
    }

    if !summary.virtual_adapters.is_empty() {
        parts.push(format!(
            "检测到网络工具/虚拟网卡 {} 个：{}",
            summary.virtual_adapters.len(),
            summary.virtual_adapters.join("、")
        ));
    }

    let mut tools = Vec::new();
    if summary.has_easy_connect {
        tools.push("EasyConnect/Sangfor");
    }
    if summary.has_clash {
        tools.push("Clash/mihomo");
    }
    if summary.has_tun {
        tools.push("TUN/TAP");
    }
    if summary.has_hypomux_like {
        tools.push("多网卡工具");
    }
    if !tools.is_empty() {
        parts.push(format!("网络工具特征：{}", tools.join("、")));
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
                    last_reconnect_at_ms: None,
                },
            );
            return;
        }
    };

    let mut last_state: Option<ReconnectCycleState> = None;
    let mut last_message: Option<String> = None;
    let mut last_error: Option<String> = None;

    while !stop.load(Ordering::Relaxed) {
        let result = rt.block_on(async {
            if easyconnect_is_active() {
                return Ok::<_, anyhow::Error>((
                    ReconnectCycleState::PausedForEasyConnect,
                    "检测到 EasyConnect 已连接，校园网自动重连已暂停".to_string(),
                    None,
                ));
            }

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
                        last_reconnect_at_ms: None,
                    },
                );
            }

            let client = SrunClient::new(cfg.clone())?;
            match client.probe_online_status().await {
                Ok(PortalOnlineStatus::Online) => Ok::<_, anyhow::Error>((
                    ReconnectCycleState::Online,
                    "online".to_string(),
                    None,
                )),
                Ok(PortalOnlineStatus::Offline) => {
                    if cfg.ac_id.is_none() && cfg.auto_query_acid {
                        if let Some(ac_id) = client.query_acid().await? {
                            cfg.ac_id = Some(ac_id);
                        }
                    }
                    let login_client = SrunClient::new(cfg.clone())?;
                    let message = login_client.login(&password).await?;
                    let reconnect_at_ms = record_last_reconnect(&mut cfg);
                    Ok((ReconnectCycleState::Online, message, Some(reconnect_at_ms)))
                }
                Ok(PortalOnlineStatus::Unknown) => Ok((
                    ReconnectCycleState::WaitingForPortal,
                    "Portal 返回了无法判定的状态，自动重连已暂停本轮".to_string(),
                    None,
                )),
                Err(err) => Ok((
                    ReconnectCycleState::WaitingForPortal,
                    format!("Portal 状态接口失败，自动重连已暂停本轮：{err:#}"),
                    None,
                )),
            }
        });

        match result {
            Ok((state, message, reconnect_at_ms)) => {
                let should_emit = last_state != Some(state)
                    || last_message.as_deref() != Some(&message)
                    || last_error.is_some();
                last_state = Some(state);
                last_message = Some(message.clone());
                last_error = None;
                if should_emit {
                    let _ = app.emit(
                        "status",
                        UiResponse {
                            status: format!("Auto reconnect: {message}"),
                            config: None,
                            online: (state == ReconnectCycleState::Online).then_some(true),
                            auto_reconnect: Some(true),
                            startup_enabled: None,
                            last_reconnect_at_ms: reconnect_at_ms,
                        },
                    );
                }
            }
            Err(err) => {
                let message = format!("{err:#}");
                let paused_for_network = is_bind_ip_fallback_error(&message);
                let state = if paused_for_network {
                    ReconnectCycleState::WaitingForNetwork
                } else {
                    ReconnectCycleState::WaitingForPortal
                };
                let should_emit = last_state != Some(ReconnectCycleState::Online)
                    || last_error.as_deref() != Some(&message);
                last_state = Some(state);
                last_message = None;
                last_error = Some(message.clone());
                if should_emit {
                    let _ = app.emit(
                        "status",
                        UiResponse {
                            status: if paused_for_network {
                                format!("登录出口不可用，自动重连已暂停：{message}")
                            } else {
                                format!("Auto reconnect failed: {message}")
                            },
                            config: None,
                            online: None,
                            auto_reconnect: Some(true),
                            startup_enabled: None,
                            last_reconnect_at_ms: None,
                        },
                    );
                }
            }
        }

        let interval = if last_state == Some(ReconnectCycleState::Online) {
            Duration::from_secs(cfg.online_check_seconds.max(default_online_check_seconds()))
        } else {
            Duration::from_secs(cfg.retry_seconds.max(15))
        };
        let mut slept = Duration::ZERO;
        let check_interval = Duration::from_secs(5);
        while slept < interval {
            if stop.load(Ordering::Relaxed) {
                return;
            }
            thread::sleep(check_interval);
            slept += check_interval;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_network_interface_infos, classify_network_interface, format_interface_summary,
        has_active_easyconnect_adapter, login_once, merge_saved_login_context_from,
        normalize_portal_url, AppConfig, NetworkInterfaceSummary, ParsedInterface, RouteInfo,
    };
    use std::net::IpAddr;

    #[cfg(target_os = "windows")]
    use super::{
        decode_windows_command_output, startup_script_matches_executable,
        startup_value_matches_executable,
    };

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

    #[test]
    fn normalize_portal_url_rejects_unsafe_hosts() {
        for target in [
            "http://localhost/srun_portal_success?ac_id=1&wlanuserip=10.1.2.3",
            "http://127.0.0.1/srun_portal_success?ac_id=1&wlanuserip=10.1.2.3",
            "http://198.18.0.1/srun_portal_success?ac_id=1&wlanuserip=10.1.2.3",
        ] {
            assert!(
                normalize_portal_url(target).is_err(),
                "{target} should be rejected"
            );
        }
    }

    #[test]
    fn saved_context_does_not_restore_cleared_bind_ip() {
        let mut cfg = AppConfig {
            portal_url: String::new(),
            ac_id: None,
            user_ip: None,
            bind_ip: None,
            ..AppConfig::default()
        };
        let saved = AppConfig {
            portal_url: "http://10.129.1.1".to_string(),
            ac_id: Some(1),
            user_ip: Some("10.1.2.3".parse().unwrap()),
            bind_ip: Some("10.1.2.4".parse().unwrap()),
            last_reconnect_at_ms: Some(1_757_000_000_000),
            ..AppConfig::default()
        };

        merge_saved_login_context_from(&mut cfg, &saved);

        assert_eq!(cfg.portal_url, "http://10.129.1.1");
        assert_eq!(cfg.ac_id, Some(1));
        assert_eq!(cfg.user_ip.unwrap().to_string(), "10.1.2.3");
        assert_eq!(cfg.bind_ip, None);
        assert_eq!(cfg.last_reconnect_at_ms, Some(1_757_000_000_000));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn decodes_gbk_ipconfig_output() {
        let bytes = b"\xd2\xd4\xcc\xab\xcd\xf8\xca\xca\xc5\xe4\xc6\xf7 WLAN:";

        assert_eq!(decode_windows_command_output(bytes), "以太网适配器 WLAN:");
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn matches_quoted_startup_path_with_optional_arguments() {
        let exe = std::path::Path::new(r"C:\Program Files\GDOU Net Login\gdou-net-login.exe");

        assert!(startup_value_matches_executable(
            r#""C:\Program Files\GDOU Net Login\gdou-net-login.exe""#,
            exe
        ));
        assert!(startup_value_matches_executable(
            r#""C:\Program Files\GDOU Net Login\gdou-net-login.exe" --hidden"#,
            exe
        ));
        assert!(!startup_value_matches_executable(
            r#""C:\Other\gdou-net-login.exe""#,
            exe
        ));
        assert!(startup_script_matches_executable(
            r#"shell.Run "C:\Program Files\GDOU Net Login\gdou-net-login.exe", 0, False"#,
            exe
        ));
        assert!(!startup_script_matches_executable(
            r#"shell.Run "C:\Other\gdou-net-login.exe", 0, False"#,
            exe
        ));
    }

    #[test]
    fn ranks_real_campus_interface_before_virtual_adapters() {
        let items = vec![
            ParsedInterface {
                interface_alias: "EasyConnect".to_string(),
                interface_description: "Sangfor SSL VPN".to_string(),
                ip: "2.0.1.7".to_string(),
                gateway: Some("2.0.1.1".to_string()),
                is_up: true,
                ..ParsedInterface::default()
            },
            ParsedInterface {
                interface_alias: "WLAN".to_string(),
                interface_description: "Intel Wi-Fi".to_string(),
                ip: "10.0.2.20".to_string(),
                gateway: Some("10.0.2.1".to_string()),
                is_up: true,
                ..ParsedInterface::default()
            },
        ];

        let infos = build_network_interface_infos(items, None, None, None);

        assert_eq!(infos[0].interface_alias, "WLAN");
        assert!(infos[0].is_likely_campus);
        assert!(!infos[0].is_virtual);
        assert!(infos[1].is_virtual);
        assert!(!infos[1].is_likely_campus);
    }

    #[test]
    fn selected_bind_ip_is_marked_and_sorted_first() {
        let selected: IpAddr = "10.0.2.20".parse().unwrap();
        let items = vec![
            ParsedInterface {
                interface_alias: "以太网".to_string(),
                interface_description: "Realtek PCIe".to_string(),
                ip: "10.0.3.20".to_string(),
                gateway: Some("10.0.3.1".to_string()),
                is_up: true,
                ..ParsedInterface::default()
            },
            ParsedInterface {
                interface_alias: "WLAN".to_string(),
                interface_description: "Intel Wi-Fi".to_string(),
                ip: selected.to_string(),
                gateway: Some("10.0.2.1".to_string()),
                is_up: true,
                ..ParsedInterface::default()
            },
        ];

        let infos = build_network_interface_infos(items, Some(selected), None, None);

        assert_eq!(infos[0].ip, selected.to_string());
        assert!(infos[0].is_selected);
        assert_eq!(infos[0].recommendation, "当前选择的登录出口");
    }

    #[test]
    fn marks_interface_used_by_portal_route() {
        let portal_ip: IpAddr = "10.0.2.20".parse().unwrap();
        let items = vec![
            ParsedInterface {
                interface_alias: "WLAN".to_string(),
                interface_description: "Intel Wi-Fi".to_string(),
                ip: portal_ip.to_string(),
                gateway: Some("10.0.2.1".to_string()),
                is_up: true,
                ..ParsedInterface::default()
            },
            ParsedInterface {
                interface_alias: "EasyConnect".to_string(),
                interface_description: "Sangfor SSL VPN".to_string(),
                ip: "2.0.1.8".to_string(),
                gateway: Some("2.0.1.1".to_string()),
                is_up: true,
                ..ParsedInterface::default()
            },
        ];
        let route = RouteInfo {
            interface: "WLAN".to_string(),
            source: Some(portal_ip),
            next_hop: Some("10.0.2.1".to_string()),
            virtual_route: false,
        };

        let infos = build_network_interface_infos(items, None, Some(&route), None);

        assert!(infos
            .iter()
            .any(|item| item.ip == portal_ip.to_string() && item.route_to_portal));
        assert!(infos
            .iter()
            .any(|item| item.interface_alias == "EasyConnect" && !item.route_to_portal));
    }

    #[test]
    fn pauses_reconnect_only_for_an_active_easyconnect_adapter() {
        let disconnected = ParsedInterface {
            interface_alias: "本地连接 2".to_string(),
            interface_description: "Sangfor SSL VPN CS Support System VNIC".to_string(),
            ip: "2.0.1.12".to_string(),
            is_up: false,
            ..ParsedInterface::default()
        };
        let active = ParsedInterface {
            is_up: true,
            ..disconnected.clone()
        };
        let wlan = ParsedInterface {
            interface_alias: "WLAN".to_string(),
            interface_description: "Intel Wi-Fi".to_string(),
            ip: "10.138.26.168".to_string(),
            is_up: true,
            ..ParsedInterface::default()
        };

        assert!(!has_active_easyconnect_adapter(&[
            disconnected,
            wlan.clone()
        ]));
        assert!(has_active_easyconnect_adapter(&[wlan, active]));
    }

    #[test]
    fn classifies_common_network_tools_as_virtual_not_srun_exits() {
        let cases = [
            ("EasyConnect", "Sangfor SSL VPN", "2.0.1.8", "EasyConnect"),
            (
                "Clash Verge TUN",
                "Wintun Userspace Tunnel",
                "198.18.0.2",
                "网络工具/虚拟网卡",
            ),
            (
                "mihomo",
                "Meta TUN Adapter",
                "198.19.0.9",
                "网络工具/虚拟网卡",
            ),
            (
                "HypoMuxPlus",
                "Network Dispatch Adapter",
                "10.20.30.40",
                "虚拟网卡",
            ),
        ];

        for (name, description, ip, expected_kind) in cases {
            let classification =
                classify_network_interface(name, description, ip.parse::<IpAddr>().unwrap());

            assert_eq!(classification.kind, expected_kind);
            assert!(classification.is_virtual, "{name} should be virtual");
            assert!(
                !classification.is_likely_srun_exit,
                "{name} should not be an SRUN login exit"
            );
        }
    }

    #[test]
    fn interface_summary_mentions_network_tool_features() {
        let summary = NetworkInterfaceSummary {
            selected_bind_ip: Some("10.1.2.3".to_string()),
            selected_bind_ip_available: false,
            campus_candidates: vec!["WLAN / 10.1.2.3".to_string()],
            virtual_adapters: vec![
                "EasyConnect / 2.0.1.8 / 虚拟网卡，不建议用于 SRUN 登录".to_string()
            ],
            has_easy_connect: true,
            has_clash: true,
            has_tun: true,
            has_hypomux_like: true,
        };

        let text = format_interface_summary(&summary);

        assert!(text.contains("当前网卡列表未发现该 IP"));
        assert!(text.contains("EasyConnect/Sangfor"));
        assert!(text.contains("Clash/mihomo"));
        assert!(text.contains("TUN/TAP"));
        assert!(text.contains("多网卡工具"));
    }

    #[tokio::test]
    async fn login_rejects_empty_password_without_network() {
        let result = login_once(AppConfig::default(), String::new()).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().to_string(), "password is required");
    }
}
