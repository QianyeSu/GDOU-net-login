use crate::config::{load_config, load_password, save_config, store_password, AppConfig};
use crate::srun::SrunClient;
use anyhow::{Context, Result};
use eframe::egui;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tokio::runtime::Runtime;

enum UiEvent {
    Message(String),
    Online(bool),
    TaskDone,
}

pub fn run_gui() -> Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([860.0, 560.0])
            .with_min_inner_size([720.0, 500.0]),
        ..Default::default()
    };

    eframe::run_native(
        "GDOU Net Login",
        options,
        Box::new(|cc| Ok(Box::new(LoginApp::new(cc)))),
    )
    .map_err(|err| anyhow::anyhow!("failed to start gui: {err}"))
}

struct LoginApp {
    portal_url: String,
    probe_url: String,
    username: String,
    password: String,
    ac_id: String,
    retry_seconds: u64,
    auto_query_acid: bool,
    auto_reconnect: bool,
    status: String,
    online: Option<bool>,
    task_running: bool,
    tx: Sender<UiEvent>,
    rx: Receiver<UiEvent>,
    watcher_stop: Option<Arc<AtomicBool>>,
}

impl LoginApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        configure_style(&cc.egui_ctx);

        let cfg = load_config().unwrap_or_default();
        let password = load_password(&cfg).unwrap_or_default();
        let (tx, rx) = mpsc::channel();
        Self {
            portal_url: cfg.portal_url,
            probe_url: cfg.probe_url,
            username: cfg.username,
            password,
            ac_id: cfg.ac_id.map(|id| id.to_string()).unwrap_or_default(),
            retry_seconds: cfg.retry_seconds,
            auto_query_acid: cfg.auto_query_acid,
            auto_reconnect: false,
            status: "Ready".to_string(),
            online: None,
            task_running: false,
            tx,
            rx,
            watcher_stop: None,
        }
    }

    fn config_from_form(&self) -> Result<AppConfig> {
        let mut cfg = AppConfig {
            portal_url: self.portal_url.trim().to_string(),
            probe_url: self.probe_url.trim().to_string(),
            username: self.username.trim().to_string(),
            ac_id: None,
            retry_seconds: self.retry_seconds.max(5),
            auto_query_acid: self.auto_query_acid,
            ..AppConfig::default()
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
        let ac_id = self.ac_id.trim();
        if !ac_id.is_empty() {
            cfg.ac_id = Some(ac_id.parse().context("invalid ac_id")?);
        }
        Ok(cfg)
    }

    fn save(&mut self) {
        match self.config_from_form().and_then(|cfg| {
            save_config(&cfg)?;
            if !self.password.is_empty() {
                store_password(&cfg, &self.password)?;
            }
            Ok(())
        }) {
            Ok(()) => self.status = "Saved".to_string(),
            Err(err) => self.status = format!("Save failed: {err:#}"),
        }
    }

    fn spawn_login(&mut self) {
        let cfg = match self.config_from_form() {
            Ok(cfg) => cfg,
            Err(err) => {
                self.status = format!("Login failed: {err:#}");
                return;
            }
        };
        let password = self.password.clone();
        if password.is_empty() {
            self.status = "Login failed: password is required".to_string();
            return;
        }
        self.save();
        self.spawn_task("Logging in...", move || login_once(cfg, password));
    }

    fn spawn_logout(&mut self) {
        let cfg = match self.config_from_form() {
            Ok(cfg) => cfg,
            Err(err) => {
                self.status = format!("Logout failed: {err:#}");
                return;
            }
        };
        let password = self.password.clone();
        self.spawn_task("Logging out...", move || logout_once(cfg, password));
    }

    fn spawn_status(&mut self) {
        let cfg = match self.config_from_form() {
            Ok(cfg) => cfg,
            Err(err) => {
                self.status = format!("Status failed: {err:#}");
                return;
            }
        };
        self.spawn_task("Checking status...", move || status_once(cfg));
    }

    fn spawn_task<F>(&mut self, pending: &str, action: F)
    where
        F: FnOnce() -> Result<(String, Option<bool>)> + Send + 'static,
    {
        if self.task_running {
            return;
        }
        self.task_running = true;
        self.status = pending.to_string();
        let tx = self.tx.clone();
        thread::spawn(move || {
            match action() {
                Ok((message, online)) => {
                    if let Some(online) = online {
                        let _ = tx.send(UiEvent::Online(online));
                    }
                    let _ = tx.send(UiEvent::Message(message));
                }
                Err(err) => {
                    let _ = tx.send(UiEvent::Message(format!("{err:#}")));
                }
            }
            let _ = tx.send(UiEvent::TaskDone);
        });
    }

    fn set_auto_reconnect(&mut self, enabled: bool) {
        if enabled == self.auto_reconnect {
            return;
        }

        if enabled {
            let cfg = match self.config_from_form() {
                Ok(cfg) => cfg,
                Err(err) => {
                    self.status = format!("Auto reconnect failed: {err:#}");
                    return;
                }
            };
            let password = self.password.clone();
            if password.is_empty() {
                self.status = "Auto reconnect failed: password is required".to_string();
                return;
            }
            self.save();
            let stop = Arc::new(AtomicBool::new(false));
            let thread_stop = stop.clone();
            let tx = self.tx.clone();
            thread::spawn(move || auto_reconnect_loop(cfg, password, thread_stop, tx));
            self.watcher_stop = Some(stop);
            self.auto_reconnect = true;
            self.status = "Auto reconnect started".to_string();
        } else {
            if let Some(stop) = self.watcher_stop.take() {
                stop.store(true, Ordering::Relaxed);
            }
            self.auto_reconnect = false;
            self.status = "Auto reconnect stopped".to_string();
        }
    }

    fn drain_events(&mut self, ctx: &egui::Context) {
        while let Ok(event) = self.rx.try_recv() {
            match event {
                UiEvent::Message(message) => self.status = message,
                UiEvent::Online(online) => self.online = Some(online),
                UiEvent::TaskDone => self.task_running = false,
            }
            ctx.request_repaint();
        }
    }
}

impl eframe::App for LoginApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain_events(ctx);

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.heading("GDOU Net Login");
                    ui.label("Guangdong Ocean University campus network");
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    status_badge(ui, self.online, self.auto_reconnect);
                });
            });
            ui.add_space(16.0);

            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.set_width(ui.available_width());
                egui::Grid::new("login_form")
                    .num_columns(2)
                    .spacing([18.0, 14.0])
                    .show(ui, |ui| {
                        ui.label("Username");
                        ui.text_edit_singleline(&mut self.username);
                        ui.end_row();

                        ui.label("Password");
                        ui.add(egui::TextEdit::singleline(&mut self.password).password(true));
                        ui.end_row();

                        ui.label("Portal URL");
                        ui.text_edit_singleline(&mut self.portal_url);
                        ui.end_row();

                        ui.label("Probe URL");
                        ui.text_edit_singleline(&mut self.probe_url);
                        ui.end_row();

                        ui.label("ac_id");
                        ui.horizontal(|ui| {
                            ui.text_edit_singleline(&mut self.ac_id);
                            ui.checkbox(&mut self.auto_query_acid, "Auto");
                        });
                        ui.end_row();

                        ui.label("Retry seconds");
                        ui.add(egui::DragValue::new(&mut self.retry_seconds).range(5..=3600));
                        ui.end_row();
                    });
            });

            ui.add_space(16.0);
            ui.horizontal_wrapped(|ui| {
                if ui.button("Save").clicked() {
                    self.save();
                }
                if ui
                    .add_enabled(!self.task_running, egui::Button::new("Login Now"))
                    .clicked()
                {
                    self.spawn_login();
                }
                if ui
                    .add_enabled(!self.task_running, egui::Button::new("Logout"))
                    .clicked()
                {
                    self.spawn_logout();
                }
                if ui
                    .add_enabled(!self.task_running, egui::Button::new("Check Status"))
                    .clicked()
                {
                    self.spawn_status();
                }

                let mut auto = self.auto_reconnect;
                if ui.checkbox(&mut auto, "Auto reconnect").changed() {
                    self.set_auto_reconnect(auto);
                }
            });

            ui.add_space(16.0);
            ui.separator();
            ui.add_space(12.0);
            ui.label(egui::RichText::new("Status").strong());
            ui.label(&self.status);
            ui.add_space(8.0);
            ui.label("Keep this window open to maintain automatic reconnect.");
        });
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if let Some(stop) = self.watcher_stop.take() {
            stop.store(true, Ordering::Relaxed);
        }
    }
}

fn configure_style(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(10.0, 10.0);
    style.visuals.widgets.active.rounding = egui::Rounding::same(6.0);
    style.visuals.widgets.hovered.rounding = egui::Rounding::same(6.0);
    style.visuals.widgets.inactive.rounding = egui::Rounding::same(6.0);
    ctx.set_style(style);
}

fn status_badge(ui: &mut egui::Ui, online: Option<bool>, watching: bool) {
    let text = match (online, watching) {
        (Some(true), true) => "Online - watching",
        (Some(true), false) => "Online",
        (Some(false), true) => "Offline - watching",
        (Some(false), false) => "Offline",
        (None, true) => "Watching",
        (None, false) => "Idle",
    };
    ui.label(egui::RichText::new(text).strong());
}

fn login_once(mut cfg: AppConfig, password: String) -> Result<(String, Option<bool>)> {
    Runtime::new()?.block_on(async move {
        if cfg.auto_query_acid && cfg.ac_id.is_none() {
            let client = SrunClient::new(cfg.clone())?;
            if let Some(ac_id) = client.query_acid().await? {
                cfg.ac_id = Some(ac_id);
                save_config(&cfg)?;
            }
        }
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

fn status_once(cfg: AppConfig) -> Result<(String, Option<bool>)> {
    Runtime::new()?.block_on(async move {
        let client = SrunClient::new(cfg)?;
        let online = client.probe_online().await?;
        let message = if online { "online" } else { "offline" }.to_string();
        Ok((message, Some(online)))
    })
}

fn auto_reconnect_loop(
    mut cfg: AppConfig,
    password: String,
    stop: Arc<AtomicBool>,
    tx: Sender<UiEvent>,
) {
    let rt = match Runtime::new() {
        Ok(rt) => rt,
        Err(err) => {
            let _ = tx.send(UiEvent::Message(format!("Runtime failed: {err:#}")));
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
                let _ = tx.send(UiEvent::Online(online));
                let _ = tx.send(UiEvent::Message(format!("Auto reconnect: {message}")));
            }
            Err(err) => {
                let _ = tx.send(UiEvent::Online(false));
                let _ = tx.send(UiEvent::Message(format!("Auto reconnect failed: {err:#}")));
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
