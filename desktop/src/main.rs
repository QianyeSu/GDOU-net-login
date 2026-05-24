#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

mod config;
mod gui;
mod srun;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use config::{load_config, load_password, save_config, store_password, AppConfig};
use rpassword::read_password;
use std::future::Future;
use std::io::{self, Write};
use std::thread;
use std::time::Duration;
use tokio::runtime::Runtime;
use tokio::sync::mpsc;
use tokio::sync::watch;
use tracing::{info, warn};

#[derive(Parser, Debug)]
#[command(name = "gdou-net-login")]
#[command(about = "Lightweight Srun campus network login client", version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand, Debug)]
enum Command {
    Init,
    Login {
        #[arg(long)]
        password: Option<String>,
    },
    Logout {
        #[arg(long)]
        password: Option<String>,
    },
    Status,
    Watch {
        #[arg(long, default_value_t = 30)]
        interval: u64,
    },
    Tray,
    Gui,
    ShowConfig,
}

#[derive(Clone, Debug)]
enum TrayCommand {
    Status,
    Login,
    Logout,
    Quit,
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();
    match cli.command.unwrap_or(Command::Gui) {
        Command::Init => run_async(init()),
        Command::Login { password } => run_async(run_login(password)),
        Command::Logout { password } => run_async(run_logout(password)),
        Command::Status => run_async(run_status()),
        Command::Watch { interval } => run_async(run_watch(interval)),
        Command::Tray => run_tray(),
        Command::Gui => gui::run_gui(),
        Command::ShowConfig => run_async(show_config()),
    }
}

fn run_async<F, T>(future: F) -> Result<T>
where
    F: Future<Output = Result<T>>,
{
    Runtime::new()
        .context("failed to create runtime")?
        .block_on(future)
}

async fn init() -> Result<()> {
    let mut cfg = AppConfig::default();
    cfg.portal_url = prompt("portal url", &cfg.portal_url)?;
    cfg.probe_url = prompt("probe url", &cfg.probe_url)?;
    cfg.username = prompt("username", "")?;
    let ac_id = prompt("ac_id (blank for auto)", "")?;
    if !ac_id.trim().is_empty() {
        cfg.ac_id = Some(ac_id.trim().parse().context("invalid ac_id")?);
    }
    let retry = prompt("retry seconds", &cfg.retry_seconds.to_string())?;
    cfg.retry_seconds = retry.trim().parse().context("invalid retry seconds")?;
    cfg.auto_query_acid = prompt("auto query ac_id [Y/n]", "Y")?.trim().to_lowercase() != "n";

    println!("password: ");
    let password = read_password().context("failed to read password")?;
    save_config(&cfg)?;
    store_password(&cfg, &password)?;
    println!("saved config and password");
    Ok(())
}

async fn run_login(password: Option<String>) -> Result<()> {
    let mut cfg = load_config()?;
    let password = match password {
        Some(p) => p,
        None => load_password(&cfg)?,
    };
    if cfg.auto_query_acid && cfg.ac_id.is_none() {
        let probe_client = srun::SrunClient::new(cfg.clone())?;
        if let Some(ac_id) = probe_client.query_acid().await? {
            cfg.ac_id = Some(ac_id);
            save_config(&cfg)?;
            info!("updated ac_id: {}", ac_id);
        }
    }
    let client = srun::SrunClient::new(cfg)?;
    let result = client.login(&password).await?;
    println!("{}", result);
    Ok(())
}

async fn run_logout(password: Option<String>) -> Result<()> {
    let cfg = load_config()?;
    let password = match password {
        Some(p) => p,
        None => load_password(&cfg)?,
    };
    let client = srun::SrunClient::new(cfg)?;
    let result = client.logout(&password).await?;
    println!("{}", result);
    Ok(())
}

async fn run_status() -> Result<()> {
    let cfg = load_config()?;
    let client = srun::SrunClient::new(cfg)?;
    let online = client.probe_online().await?;
    println!("{}", if online { "online" } else { "offline" });
    Ok(())
}

async fn run_watch(interval: u64) -> Result<()> {
    let mut cfg = load_config()?;
    let password = load_password(&cfg)?;
    loop {
        watch_once(&mut cfg, &password).await?;
        tokio::time::sleep(Duration::from_secs(interval.max(5))).await;
    }
}

fn run_tray() -> Result<()> {
    let cfg = load_config()?;
    let password = load_password(&cfg)?;
    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel::<TrayCommand>();
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let icon = default_icon()?;

    let tray_handle = thread::spawn(move || tray_loop(icon, cmd_tx, shutdown_rx));
    let worker_handle = thread::spawn(move || {
        Runtime::new()
            .context("failed to create worker runtime")?
            .block_on(background_worker(cfg, password, cmd_rx, shutdown_tx))
    });

    let _ = tray_handle
        .join()
        .map_err(|_| anyhow::anyhow!("tray thread panicked"))??;
    let _ = worker_handle
        .join()
        .map_err(|_| anyhow::anyhow!("worker thread panicked"))??;
    Ok(())
}

fn tray_loop(
    icon: tray_menu::Icon,
    cmd_tx: mpsc::UnboundedSender<TrayCommand>,
    shutdown_rx: watch::Receiver<bool>,
) -> Result<()> {
    let tray = tray_menu::TrayIconBuilder::new()
        .with_tooltip("GDOU net login")
        .with_icon(icon)
        .build()
        .context("failed to create tray icon")?;

    let tray_id = tray.id().clone();
    let receiver = tray_menu::TrayIconEvent::receiver();

    loop {
        if *shutdown_rx.borrow() {
            break;
        }

        match receiver.try_recv() {
            Ok(tray_menu::TrayIconEvent::Click {
                id,
                button: tray_menu::MouseButton::Right,
                button_state: tray_menu::MouseButtonState::Up,
                position,
                ..
            }) if id == tray_id => {
                let mut menu = tray_menu::PopupMenu::new();
                menu.add(&tray_menu::TextEntry::of("status", "Status"));
                menu.add(&tray_menu::TextEntry::of("login", "Login"));
                menu.add(&tray_menu::TextEntry::of("logout", "Logout"));
                menu.add(&tray_menu::Divider);
                menu.add(&tray_menu::TextEntry::of("quit", "Quit"));
                if let Some(id) = menu.popup(position) {
                    match id.0.as_str() {
                        "status" => {
                            let _ = cmd_tx.send(TrayCommand::Status);
                        }
                        "login" => {
                            let _ = cmd_tx.send(TrayCommand::Login);
                        }
                        "logout" => {
                            let _ = cmd_tx.send(TrayCommand::Logout);
                        }
                        "quit" => {
                            let _ = cmd_tx.send(TrayCommand::Quit);
                            break;
                        }
                        _ => {}
                    }
                }
            }
            Ok(_) => {}
            Err(_) => {
                thread::sleep(Duration::from_millis(16));
            }
        }
    }

    drop(tray);
    Ok(())
}

async fn background_worker(
    mut cfg: AppConfig,
    password: String,
    mut cmd_rx: mpsc::UnboundedReceiver<TrayCommand>,
    shutdown_tx: watch::Sender<bool>,
) -> Result<()> {
    let mut interval = tokio::time::interval(Duration::from_secs(cfg.retry_seconds.max(5)));
    loop {
        tokio::select! {
            _ = interval.tick() => {
                if let Err(err) = watch_once(&mut cfg, &password).await {
                    warn!("watch failed: {:#}", err);
                }
            }
            cmd = cmd_rx.recv() => {
                match cmd {
                    Some(TrayCommand::Status) => {
                        if let Err(err) = run_status().await {
                            warn!("status failed: {:#}", err);
                        }
                    }
                    Some(TrayCommand::Login) => {
                        if let Err(err) = run_login(None).await {
                            warn!("login failed: {:#}", err);
                        }
                    }
                    Some(TrayCommand::Logout) => {
                        if let Err(err) = run_logout(None).await {
                            warn!("logout failed: {:#}", err);
                        }
                    }
                    Some(TrayCommand::Quit) => {
                        let _ = shutdown_tx.send(true);
                        break;
                    }
                    None => {
                        let _ = shutdown_tx.send(true);
                        break;
                    }
                }
            }
        }
    }
    Ok(())
}

async fn watch_once(cfg: &mut AppConfig, password: &str) -> Result<()> {
    let client = srun::SrunClient::new(cfg.clone())?;
    match client.probe_online().await {
        Ok(true) => info!("online"),
        Ok(false) => {
            warn!("offline, trying login");
            if cfg.auto_query_acid {
                if let Some(ac_id) = client.query_acid().await? {
                    cfg.ac_id = Some(ac_id);
                    save_config(cfg)?;
                    info!("refreshed ac_id: {}", ac_id);
                }
            }
            let login_client = srun::SrunClient::new(cfg.clone())?;
            match login_client.login(password).await {
                Ok(msg) => info!("{}", msg),
                Err(err) => warn!("login failed: {:#}", err),
            }
        }
        Err(err) => warn!("probe failed: {:#}", err),
    }
    Ok(())
}

fn default_icon() -> Result<tray_menu::Icon> {
    let rgba = [
        0x2d, 0x6b, 0xff, 0xff, 0x2d, 0x6b, 0xff, 0xff, 0x2d, 0x6b, 0xff, 0xff, 0x2d, 0x6b, 0xff,
        0xff,
    ];
    tray_menu::Icon::from_rgba(rgba.to_vec(), 2, 2).context("failed to build tray icon")
}

async fn show_config() -> Result<()> {
    let cfg = load_config()?;
    println!("{}", serde_json::to_string_pretty(&cfg)?);
    Ok(())
}

fn prompt(label: &str, default: &str) -> Result<String> {
    print!("{} [{}]: ", label, default);
    io::stdout().flush().ok();
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let value = input.trim();
    Ok(if value.is_empty() {
        default.to_string()
    } else {
        value.to_string()
    })
}
