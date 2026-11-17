pub mod theme;

use crate::client::connection::{connect_to_host, ConnectionHandle, DecodedFrameMessage};
use crate::common::net_info::{get_local_ips, InterfaceIp};
use crate::common::protocol::{InputEvent, KeyCode, MouseBtn};
use crate::host::server::{HostServer, HostServerConfig};
use eframe::egui::{self, Color32, RichText, Rounding, Stroke, Vec2};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Instant;
use theme::THEME;
use tokio::sync::{broadcast, mpsc, oneshot};

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum NavTab {
    Host,
    Client,
    Recent,
    Settings,
    Diagnostics,
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum DisplayScaling {
    FitAspect,
    Stretch,
}

#[derive(Clone, Debug)]
pub struct RecentHostEntry {
    pub address: String,
    pub timestamp_desc: String,
}

struct Toast {
    message: String,
    created_at: Instant,
    duration_secs: f32,
}

struct PendingConnection {
    rx: oneshot::Receiver<anyhow::Result<ConnectionHandle>>,
    target_addr: String,
}

pub struct UnifiedRemoteApp {
    // Current Navigation Tab
    current_tab: NavTab,

    // Host Tab State
    host_bind_ip: String,
    host_port: String,
    host_password: String,
    show_host_password: bool,
    host_fps: u32,
    host_quality: u8,
    is_host_running: bool,
    host_status_text: String,
    host_shutdown_tx: Option<broadcast::Sender<()>>,
    discovered_ips: Vec<InterfaceIp>,

    // Client Tab State
    client_host_addr: String,
    client_password: String,
    show_client_password: bool,
    client_status_message: String,
    client_status_is_error: bool,
    is_client_connecting: bool,
    pending_connection: Option<PendingConnection>,
    recent_hosts: Vec<RecentHostEntry>,

    // Remote Session State (when client is connected)
    connection: Option<ConnectionHandle>,
    frame_rx: Option<mpsc::Receiver<DecodedFrameMessage>>,
    video_texture: Option<egui::TextureHandle>,
    last_frame_size: (u32, u32),
    display_scaling: DisplayScaling,
    session_start_time: Option<Instant>,

    // Performance Stats
    fps_counter: u32,
    current_fps: u32,
    last_fps_update: Instant,

    // Notification toast
    toast: Option<Toast>,

    // Tokio Runtime
    tokio_runtime: Arc<tokio::runtime::Runtime>,
}

impl UnifiedRemoteApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        initial_host: Option<String>,
        initial_password: Option<String>,
        tokio_runtime: Arc<tokio::runtime::Runtime>,
    ) -> Self {
        // Apply modern dark theme immediately
        theme::apply_modern_theme(&cc.egui_ctx);

        let discovered_ips = get_local_ips();
        let default_client_host = if let Some(h) = initial_host {
            h
        } else if let Some(ts_ip) = discovered_ips.iter().find(|i| i.is_tailscale) {
            format!("{}:5885", ts_ip.ip)
        } else {
            "100.x.y.z:5885".to_string()
        };

        let mut recent_hosts = Vec::new();
        if let Some(ts_ip) = discovered_ips.iter().find(|i| i.is_tailscale) {
            recent_hosts.push(RecentHostEntry {
                address: format!("{}:5885", ts_ip.ip),
                timestamp_desc: "Current Machine (Tailscale)".to_string(),
            });
        }

        Self {
            current_tab: NavTab::Host,

            // Host
            host_bind_ip: "0.0.0.0".to_string(),
            host_port: "5885".to_string(),
            host_password: initial_password.clone().unwrap_or_else(|| "remote88".to_string()),
            show_host_password: false,
            host_fps: 60,
            host_quality: 75,
            is_host_running: false,
            host_status_text: "Host server is idle. Click 'Start Host Server' to accept connections.".to_string(),
            host_shutdown_tx: None,
            discovered_ips,

            // Client
            client_host_addr: default_client_host,
            client_password: initial_password.unwrap_or_default(),
            show_client_password: false,
            client_status_message: "Ready to connect to remote host.".to_string(),
            client_status_is_error: false,
            is_client_connecting: false,
            pending_connection: None,
            recent_hosts,

            // Session
            connection: None,
            frame_rx: None,
            video_texture: None,
            last_frame_size: (0, 0),
            display_scaling: DisplayScaling::FitAspect,
            session_start_time: None,

            // Stats
            fps_counter: 0,
            current_fps: 0,
            last_fps_update: Instant::now(),

            // Toast
            toast: None,

            tokio_runtime,
        }
    }

    pub fn show_toast(&mut self, message: impl Into<String>) {
        self.toast = Some(Toast {
            message: message.into(),
            created_at: Instant::now(),
            duration_secs: 2.5f32,
        });
    }

    pub fn add_recent_host(&mut self, addr: &str) {
        if addr.trim().is_empty() {
            return;
        }
        let clean_addr = addr.trim().to_string();
        self.recent_hosts.retain(|item| item.address != clean_addr);
        self.recent_hosts.insert(
            0,
            RecentHostEntry {
                address: clean_addr,
                timestamp_desc: "Recently Connected".to_string(),
            },
        );
        if self.recent_hosts.len() > 6 {
            self.recent_hosts.truncate(6);
        }
    }

    fn copy_text(&mut self, ctx: &egui::Context, text: &str, toast_msg: &str) {
        ctx.output_mut(|o| o.copied_text = text.to_string());
        if let Ok(mut cb) = arboard::Clipboard::new() {
            let _ = cb.set_text(text);
        }
        self.show_toast(toast_msg);
    }

    fn refresh_ips(&mut self) {
        self.discovered_ips = get_local_ips();
        self.show_toast("Refreshed network interfaces!");
    }

    fn start_host_server(&mut self) {
        if self.is_host_running {
            return;
        }

        let port: u16 = self.host_port.parse().unwrap_or(5885);
        let bind_addr = format!("{}:{}", self.host_bind_ip, port);
        let password = self.host_password.clone();
        let fps = self.host_fps;
        let quality = self.host_quality;

        let config = HostServerConfig {
            bind_addr: bind_addr.clone(),
            password,
            target_fps: fps,
            jpeg_quality: quality,
        };

        let server = HostServer::new(config);
        let (shutdown_tx, shutdown_rx) = broadcast::channel(1);
        self.host_shutdown_tx = Some(shutdown_tx);
        self.is_host_running = true;
        self.host_status_text = format!("RUNNING on port {} (Target: {} FPS)", port, fps);
        self.show_toast("Host server started successfully!");

        let rt = self.tokio_runtime.clone();
        rt.spawn(async move {
            let _ = server.run_with_shutdown(shutdown_rx).await;
        });
    }

    fn stop_host_server(&mut self) {
        if let Some(tx) = self.host_shutdown_tx.take() {
            let _ = tx.send(());
        }
        self.is_host_running = false;
        self.host_status_text = "Server stopped by user.".to_string();
        self.show_toast("Host server stopped.");
    }

    fn start_client_connect(&mut self) {
        if self.is_client_connecting || self.connection.is_some() {
            return;
        }

        let host_addr = self.client_host_addr.trim().to_string();
        if host_addr.is_empty() {
            self.client_status_message = "Please enter a valid host address (e.g. 100.x.y.z:5885)".to_string();
            self.client_status_is_error = true;
            return;
        }

        self.is_client_connecting = true;
        self.client_status_message = format!("Connecting to {} over direct link...", host_addr);
        self.client_status_is_error = false;

        let password = self.client_password.clone();
        let (tx_frame, rx_frame) = mpsc::channel::<DecodedFrameMessage>(4);
        self.frame_rx = Some(rx_frame);

        let (tx_res, rx_res) = oneshot::channel();
        let target_clone = host_addr.clone();

        self.tokio_runtime.spawn(async move {
            let res = connect_to_host(&target_clone, &password, tx_frame).await;
            let _ = tx_res.send(res);
        });

        self.pending_connection = Some(PendingConnection {
            rx: rx_res,
            target_addr: host_addr,
        });
    }

    fn cancel_client_connect(&mut self) {
        self.pending_connection = None;
        self.is_client_connecting = false;
        self.frame_rx = None;
        self.client_status_message = "Connection cancelled by user.".to_string();
        self.client_status_is_error = false;
    }

    fn disconnect_session(&mut self) {
        self.connection = None;
        self.frame_rx = None;
        self.video_texture = None;
        self.session_start_time = None;
        self.client_status_message = "Disconnected from remote host.".to_string();
        self.client_status_is_error = false;
        self.show_toast("Remote session ended");
    }

    fn sync_local_clipboard_to_host(&mut self) {
        if let Some(conn) = &self.connection {
            if let Ok(mut cb) = arboard::Clipboard::new() {
                if let Ok(text) = cb.get_text() {
                    let _ = conn.tx_clipboard.try_send(text);
                    self.show_toast("Copied local clipboard to remote PC!");
                    return;
                }
            }
        }
        self.show_toast("No text found in clipboard.");
    }
}

impl eframe::App for UnifiedRemoteApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint();

        // FPS calculation
        self.fps_counter += 1;
        if self.last_fps_update.elapsed().as_secs() >= 1 {
            self.current_fps = self.fps_counter;
            self.fps_counter = 0;
            self.last_fps_update = Instant::now();
        }

        // Poll non-blocking pending connection
        if let Some(mut pending) = self.pending_connection.take() {
            match pending.rx.try_recv() {
                Ok(Ok(handle)) => {
                    self.is_client_connecting = false;
                    self.client_status_message = "Connected to remote desktop!".to_string();
                    self.client_status_is_error = false;
                    let target = pending.target_addr.clone();
                    self.connection = Some(handle);
                    self.session_start_time = Some(Instant::now());
                    self.add_recent_host(&target);
                    self.show_toast("Remote session established!");
                }
                Ok(Err(e)) => {
                    self.is_client_connecting = false;
                    self.client_status_message = format!("Failed to connect: {}", e);
                    self.client_status_is_error = true;
                    self.connection = None;
                    self.frame_rx = None;
                }
                Err(oneshot::error::TryRecvError::Empty) => {
                    // Still waiting in background
                    self.pending_connection = Some(pending);
                }
                Err(oneshot::error::TryRecvError::Closed) => {
                    self.is_client_connecting = false;
                    self.client_status_message = "Connection task terminated unexpectedly".to_string();
                    self.client_status_is_error = true;
                    self.connection = None;
                    self.frame_rx = None;
                }
            }
        }

        // Consume video frames if active connection exists
        if let Some(rx) = &mut self.frame_rx {
            while let Ok(msg) = rx.try_recv() {
                let color_image = egui::ColorImage::from_rgba_unmultiplied(
                    [msg.width as usize, msg.height as usize],
                    &msg.rgba_data,
                );
                self.last_frame_size = (msg.width, msg.height);
                self.video_texture = Some(ctx.load_texture(
                    "remote_screen",
                    color_image,
                    egui::TextureOptions::LINEAR,
                ));
            }
        }

        // IF ACTIVE REMOTE SESSION: Render immersive full remote desktop view
        if self.connection.is_some() {
            self.render_remote_session(ctx);
            self.render_toast(ctx);
            return;
        }

        // Left Navigation Sidebar
        egui::SidePanel::left("modern_sidebar")
            .exact_width(240.0f32)
            .resizable(false)
            .frame(
                egui::Frame::none()
                    .fill(THEME.bg_panel)
                    .stroke(Stroke::new(1.0f32, THEME.border_subtle)),
            )
            .show(ctx, |ui| {
                self.render_sidebar(ui, ctx);
            });

        // Main Content Area
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(THEME.bg_deep))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.add_space(20.0f32);
                        match self.current_tab {
                            NavTab::Host => self.render_host_tab(ui, ctx),
                            NavTab::Client => self.render_client_tab(ui),
                            NavTab::Recent => self.render_recent_tab(ui),
                            NavTab::Settings => self.render_settings_tab(ui),
                            NavTab::Diagnostics => self.render_diagnostics_tab(ui, ctx),
                        }
                        ui.add_space(30.0f32);
                    });
            });

        // Floating Toast Notification
        self.render_toast(ctx);
    }
}

// -------------------------------------------------------------------------------------------------
// Sidebar & Navigation
// -------------------------------------------------------------------------------------------------
impl UnifiedRemoteApp {
    fn render_sidebar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.add_space(16.0f32);

        // App Brand Header
        ui.horizontal(|ui| {
            ui.add_space(12.0f32);
            theme::modern_badge(THEME.primary, THEME.primary_hover).show(ui, |ui| {
                ui.label(RichText::new("⚡").size(18.0f32).color(Color32::WHITE));
            });
            ui.vertical(|ui| {
                ui.label(
                    RichText::new("RUST REMOTE")
                        .size(15.0f32)
                        .strong()
                        .color(THEME.text_primary),
                );
                ui.label(
                    RichText::new("Tailscale P2P Engine")
                        .size(11.0f32)
                        .color(THEME.text_secondary),
                );
            });
        });

        ui.add_space(16.0f32);
        ui.separator();
        ui.add_space(12.0f32);

        // Navigation Menu
        ui.horizontal(|ui| {
            ui.add_space(12.0f32);
            ui.label(
                RichText::new("NAVIGATION")
                    .size(11.0f32)
                    .strong()
                    .color(THEME.text_muted),
            );
        });
        ui.add_space(6.0f32);

        self.sidebar_nav_item(ui, NavTab::Host, "📡", "Share Screen");
        self.sidebar_nav_item(ui, NavTab::Client, "🚀", "Connect Remote");
        self.sidebar_nav_item(ui, NavTab::Recent, "⏱️", "Recent Sessions");
        self.sidebar_nav_item(ui, NavTab::Settings, "⚙️", "Settings");
        self.sidebar_nav_item(ui, NavTab::Diagnostics, "ℹ️", "Diagnostics");

        ui.add_space(16.0f32);

        // Bottom Network Status Card
        let tailscale_entry = self.discovered_ips.iter().find(|i| i.is_tailscale);
        let primary_ip = tailscale_entry
            .map(|i| i.ip.clone())
            .unwrap_or_else(|| self.discovered_ips.first().map(|i| i.ip.clone()).unwrap_or_else(|| "127.0.0.1".to_string()));

        let is_tailscale = tailscale_entry.is_some();
        let mut copy_ip_trigger = false;

        ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
            ui.add_space(14.0f32);

            ui.horizontal(|ui| {
                ui.add_space(12.0f32);
                egui::Frame::none()
                    .fill(THEME.bg_card)
                    .rounding(Rounding::same(8.0f32))
                    .stroke(Stroke::new(1.0f32, THEME.border_subtle))
                    .inner_margin(egui::Margin::same(10.0f32))
                    .show(ui, |ui| {
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                let (dot_color, label) = if is_tailscale {
                                    (THEME.tailscale_blue, "Tailscale Active")
                                } else {
                                    (THEME.success, "Local Network Ready")
                                };
                                ui.colored_label(dot_color, RichText::new("●").size(12.0f32));
                                ui.label(RichText::new(label).size(11.0f32).strong().color(THEME.text_primary));
                            });
                            ui.add_space(2.0f32);
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(&primary_ip)
                                        .size(12.0f32)
                                        .monospace()
                                        .color(THEME.text_secondary),
                                );
                                if ui.button(RichText::new("📋").size(12.0f32)).on_hover_text("Copy IP to clipboard").clicked() {
                                    copy_ip_trigger = true;
                                }
                            });
                        });
                    });
                ui.add_space(12.0f32);
            });
        });

        if copy_ip_trigger {
            self.copy_text(ctx, &primary_ip, "IP copied to clipboard!");
        }
    }

    fn sidebar_nav_item(
        &mut self,
        ui: &mut egui::Ui,
        tab: NavTab,
        icon: &str,
        title: &str,
    ) {
        let is_selected = self.current_tab == tab;
        let bg_fill = if is_selected {
            THEME.primary
        } else {
            Color32::TRANSPARENT
        };
        let text_color = if is_selected {
            Color32::WHITE
        } else {
            THEME.text_secondary
        };

        ui.horizontal(|ui| {
            ui.add_space(10.0f32);
            let btn = egui::Button::new(
                RichText::new(format!("{}  {}", icon, title))
                    .size(13.5f32)
                    .strong()
                    .color(text_color),
            )
            .fill(bg_fill)
            .rounding(Rounding::same(8.0f32))
            .min_size(Vec2::new(218.0f32, 36.0f32));

            if ui.add(btn).clicked() {
                self.current_tab = tab;
            }
            ui.add_space(10.0f32);
        });
        ui.add_space(2.0f32);
    }
}

// -------------------------------------------------------------------------------------------------
// Tab 1: Share Screen (Host Mode)
// -------------------------------------------------------------------------------------------------
impl UnifiedRemoteApp {
    fn render_host_tab(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let mut copy_trigger: Option<(String, &'static str)> = None;

        ui.horizontal(|ui| {
            ui.add_space(24.0f32);
            ui.vertical(|ui| {
                // Header
                ui.label(
                    RichText::new("Share This Computer")
                        .size(24.0f32)
                        .strong()
                        .color(THEME.text_primary),
                );
                ui.label(
                    RichText::new("Turn on the host server to allow peer devices to view and control this desktop.")
                        .size(13.0f32)
                        .color(THEME.text_secondary),
                );
                ui.add_space(16.0f32);

                // 1. Live Status Hero Banner
                theme::status_banner_frame(self.is_host_running).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let dot_color = if self.is_host_running { THEME.success } else { THEME.text_muted };
                        ui.colored_label(dot_color, RichText::new("●").size(18.0f32));
                        ui.vertical(|ui| {
                            let status_title = if self.is_host_running {
                                "HOST SERVER IS LIVE & STREAMING"
                            } else {
                                "HOST SERVER IS OFFLINE"
                            };
                            ui.label(
                                RichText::new(status_title)
                                    .size(14.0f32)
                                    .strong()
                                    .color(if self.is_host_running { THEME.success } else { THEME.text_primary }),
                            );
                            ui.label(
                                RichText::new(&self.host_status_text)
                                    .size(12.0f32)
                                    .color(THEME.text_secondary),
                            );
                        });

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if !self.is_host_running {
                                if theme::success_button(
                                    ui,
                                    RichText::new("▶  Start Host Server").size(14.0f32).strong().color(Color32::WHITE),
                                    Vec2::new(170.0f32, 38.0f32),
                                )
                                .clicked()
                                {
                                    self.start_host_server();
                                }
                            } else {
                                if theme::danger_button(
                                    ui,
                                    RichText::new("⏹  Stop Server").size(14.0f32).strong().color(Color32::WHITE),
                                    Vec2::new(140.0f32, 38.0f32),
                                )
                                .clicked()
                                {
                                    self.stop_host_server();
                                }
                            }
                        });
                    });
                });

                ui.add_space(16.0f32);

                // 2. Connection Credentials Card
                theme::modern_card().show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("🔑 Connection Credentials").size(16.0f32).strong().color(THEME.text_primary));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button(RichText::new("🔄 Re-scan IPs").size(12.0f32)).clicked() {
                                self.refresh_ips();
                            }
                        });
                    });
                    ui.label(
                        RichText::new("Give this address and password to authorized client computers to connect.")
                            .size(12.5f32)
                            .color(THEME.text_secondary),
                    );
                    ui.add_space(12.0f32);

                    // Hero Tailscale IP Display Box
                    let tailscale_opt = self.discovered_ips.iter().find(|i| i.is_tailscale);
                    let display_ip = tailscale_opt
                        .map(|i| i.ip.clone())
                        .unwrap_or_else(|| self.discovered_ips.first().map(|i| i.ip.clone()).unwrap_or_else(|| "127.0.0.1".to_string()));
                    let full_connect_addr = format!("{}:{}", display_ip, self.host_port);

                    egui::Frame::none()
                        .fill(THEME.bg_input)
                        .rounding(Rounding::same(10.0f32))
                        .stroke(Stroke::new(1.0f32, THEME.border_focus))
                        .inner_margin(egui::Margin::same(14.0f32))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.vertical(|ui| {
                                    ui.horizontal(|ui| {
                                        if tailscale_opt.is_some() {
                                            theme::modern_badge(THEME.tailscale_soft, THEME.tailscale_blue).show(ui, |ui| {
                                                ui.label(
                                                    RichText::new("TAILSCALE DIRECT")
                                                        .size(10.0f32)
                                                        .strong()
                                                        .color(THEME.tailscale_blue),
                                                );
                                            });
                                        } else {
                                            theme::modern_badge(THEME.bg_card, THEME.border_subtle).show(ui, |ui| {
                                                ui.label(
                                                    RichText::new("LOCAL LAN")
                                                        .size(10.0f32)
                                                        .strong()
                                                        .color(THEME.text_secondary),
                                                );
                                            });
                                        }
                                    });
                                    ui.add_space(4.0f32);
                                    ui.label(
                                        RichText::new(&full_connect_addr)
                                            .size(20.0f32)
                                            .monospace()
                                            .strong()
                                            .color(THEME.text_primary),
                                    );
                                });

                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if theme::primary_button(
                                        ui,
                                        RichText::new("📋 Copy Full Address").size(13.0f32).strong().color(Color32::WHITE),
                                        Vec2::new(160.0f32, 36.0f32),
                                    )
                                    .clicked()
                                    {
                                        copy_trigger = Some((full_connect_addr.clone(), "Address copied to clipboard!"));
                                    }
                                });
                            });
                        });

                    ui.add_space(14.0f32);

                    // Password and port settings
                    egui::Grid::new("host_creds_grid")
                        .spacing([18.0f32, 14.0f32])
                        .show(ui, |ui| {
                            ui.label(RichText::new("Access Password:").strong().color(THEME.text_secondary));
                            ui.horizontal(|ui| {
                                let edit = egui::TextEdit::singleline(&mut self.host_password)
                                    .password(!self.show_host_password)
                                    .desired_width(180.0f32);
                                ui.add(edit);

                                let eye_icon = if self.show_host_password { "🔒 Hide" } else { "👁 Show" };
                                if ui.button(eye_icon).clicked() {
                                    self.show_host_password = !self.show_host_password;
                                }

                                if ui.button("🎲 Generate").on_hover_text("Generate random secure password").clicked() {
                                    self.host_password = format!("pass_{}", rand_val());
                                    self.show_toast("Generated new random password!");
                                }

                                if ui.button("📋 Copy").clicked() {
                                    copy_trigger = Some((self.host_password.clone(), "Password copied!"));
                                }
                            });
                            ui.end_row();

                            ui.label(RichText::new("Port:").strong().color(THEME.text_secondary));
                            ui.horizontal(|ui| {
                                ui.add(egui::TextEdit::singleline(&mut self.host_port).desired_width(100.0f32));
                                ui.label(RichText::new("(Default: 5885)").size(11.5f32).color(THEME.text_muted));
                            });
                            ui.end_row();
                        });

                    ui.add_space(10.0f32);

                    // Expandable list of all network adapters
                    let ips = self.discovered_ips.clone();
                    egui::CollapsingHeader::new(
                        RichText::new("🌐 All Detected Network Interfaces").size(13.0f32).color(THEME.text_secondary),
                    )
                    .default_open(false)
                    .show(ui, |ui| {
                        ui.add_space(4.0f32);
                        for item in &ips {
                            ui.horizontal(|ui| {
                                if item.is_tailscale {
                                    theme::modern_badge(THEME.tailscale_soft, THEME.tailscale_blue).show(ui, |ui| {
                                        ui.label(RichText::new("Tailscale").size(10.0f32).color(THEME.tailscale_blue));
                                    });
                                } else {
                                    theme::modern_badge(THEME.bg_deep, THEME.border_subtle).show(ui, |ui| {
                                        ui.label(RichText::new(&item.label).size(10.0f32).color(THEME.text_muted));
                                    });
                                }
                                ui.label(RichText::new(&item.ip).size(13.0f32).monospace().color(THEME.text_primary));

                                if ui.button(RichText::new("Copy").size(11.0f32)).clicked() {
                                    copy_trigger = Some((item.ip.clone(), "Interface IP copied!"));
                                }
                            });
                        }
                    });
                });

                ui.add_space(16.0f32);

                // 3. Performance & Quality Tuning
                theme::modern_card().show(ui, |ui| {
                    ui.label(RichText::new("⚡ Streaming Performance & Encoder").size(16.0f32).strong().color(THEME.text_primary));
                    ui.label(
                        RichText::new("Adjust frame rate and JPEG compression quality for optimal low latency.")
                            .size(12.5f32)
                            .color(THEME.text_secondary),
                    );
                    ui.add_space(12.0f32);

                    egui::Grid::new("host_tuning_grid")
                        .spacing([18.0f32, 14.0f32])
                        .show(ui, |ui| {
                            ui.label(RichText::new("Target Framerate:").strong().color(THEME.text_secondary));
                            ui.horizontal(|ui| {
                                ui.selectable_value(&mut self.host_fps, 30, RichText::new("30 FPS (Standard)").size(13.0f32));
                                ui.selectable_value(&mut self.host_fps, 60, RichText::new("60 FPS (Ultra Smooth)").size(13.0f32));
                            });
                            ui.end_row();

                            ui.label(RichText::new("Image Quality:").strong().color(THEME.text_secondary));
                            ui.vertical(|ui| {
                                ui.horizontal(|ui| {
                                    ui.add(egui::Slider::new(&mut self.host_quality, 50..=95).suffix("%"));
                                    let quality_desc = if self.host_quality >= 85 {
                                        "Crystal Clear"
                                    } else if self.host_quality >= 70 {
                                        "Balanced (Recommended)"
                                    } else {
                                        "Data Saver / Low Latency"
                                    };
                                    ui.label(RichText::new(quality_desc).size(12.0f32).color(THEME.text_secondary));
                                });
                                ui.horizontal(|ui| {
                                    if ui.button(RichText::new("Low Bandwidth (60%)").size(11.0f32)).clicked() {
                                        self.host_quality = 60;
                                    }
                                    if ui.button(RichText::new("Balanced (75%)").size(11.0f32)).clicked() {
                                        self.host_quality = 75;
                                    }
                                    if ui.button(RichText::new("Ultra (90%)").size(11.0f32)).clicked() {
                                        self.host_quality = 90;
                                    }
                                });
                            });
                            ui.end_row();
                        });
                });
            });
            ui.add_space(24.0f32);
        });

        if let Some((text, msg)) = copy_trigger {
            self.copy_text(ctx, &text, msg);
        }
    }
}

// -------------------------------------------------------------------------------------------------
// Tab 2: Connect Remote (Client Mode)
// -------------------------------------------------------------------------------------------------
impl UnifiedRemoteApp {
    fn render_client_tab(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.add_space(24.0f32);
            ui.vertical(|ui| {
                // Header
                ui.label(
                    RichText::new("Connect to Remote PC")
                        .size(24.0f32)
                        .strong()
                        .color(THEME.text_primary),
                );
                ui.label(
                    RichText::new("Enter the Tailscale or LAN IP address and password of the remote PC you want to control.")
                        .size(13.0f32)
                        .color(THEME.text_secondary),
                );
                ui.add_space(16.0f32);

                // Main Connection Card
                theme::modern_card_elevated().show(ui, |ui| {
                    ui.label(
                        RichText::new("🔗 Direct Remote Session")
                            .size(17.0f32)
                            .strong()
                            .color(THEME.text_primary),
                    );
                    ui.add_space(12.0f32);

                    // Form Fields
                    egui::Grid::new("client_form_grid")
                        .spacing([18.0f32, 16.0f32])
                        .show(ui, |ui| {
                            ui.label(RichText::new("Host Address (IP:Port):").strong().color(THEME.text_secondary));
                            ui.horizontal(|ui| {
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.client_host_addr)
                                        .hint_text("e.g. 100.x.y.z:5885 or 192.168.1.50:5885")
                                        .desired_width(280.0f32),
                                );
                                if ui.button("📋 Paste").clicked() {
                                    if let Ok(mut cb) = arboard::Clipboard::new() {
                                        if let Ok(text) = cb.get_text() {
                                            self.client_host_addr = text.trim().to_string();
                                            self.show_toast("Pasted address from clipboard!");
                                        }
                                    }
                                }
                            });
                            ui.end_row();

                            ui.label(RichText::new("Password / Token:").strong().color(THEME.text_secondary));
                            ui.horizontal(|ui| {
                                let edit = egui::TextEdit::singleline(&mut self.client_password)
                                    .password(!self.show_client_password)
                                    .hint_text("Host access password")
                                    .desired_width(280.0f32);
                                ui.add(edit);

                                let eye_icon = if self.show_client_password { "🔒 Hide" } else { "👁 Show" };
                                if ui.button(eye_icon).clicked() {
                                    self.show_client_password = !self.show_client_password;
                                }
                            });
                            ui.end_row();
                        });

                    ui.add_space(16.0f32);

                    // Status Message Banner
                    if !self.client_status_message.is_empty() {
                        let (banner_bg, banner_border, text_color, icon) = if self.client_status_is_error {
                            (THEME.danger_soft, THEME.danger, THEME.danger_hover, "⚠️")
                        } else if self.is_client_connecting {
                            (THEME.bg_card, THEME.primary, THEME.primary_hover, "⏳")
                        } else {
                            (THEME.bg_card, THEME.border_subtle, THEME.text_secondary, "ℹ️")
                        };

                        egui::Frame::none()
                            .fill(banner_bg)
                            .rounding(Rounding::same(8.0f32))
                            .stroke(Stroke::new(1.0f32, banner_border))
                            .inner_margin(egui::Margin::symmetric(14.0f32, 10.0f32))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(icon).size(14.0f32));
                                    ui.label(RichText::new(&self.client_status_message).size(13.0f32).color(text_color));
                                });
                            });
                        ui.add_space(14.0f32);
                    }

                    // Connect / Cancel Buttons
                    ui.horizontal(|ui| {
                        if !self.is_client_connecting {
                            if theme::primary_button(
                                ui,
                                RichText::new("🚀  Connect to Remote PC")
                                    .size(15.0f32)
                                    .strong()
                                    .color(Color32::WHITE),
                                Vec2::new(240.0f32, 42.0f32),
                            )
                            .clicked()
                            {
                                self.start_client_connect();
                            }
                        } else {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label(RichText::new("Connecting to peer...").strong().color(THEME.primary_hover));
                                ui.add_space(8.0f32);
                                if theme::danger_button(
                                    ui,
                                    RichText::new("✕ Cancel").size(13.0f32).color(Color32::WHITE),
                                    Vec2::new(100.0f32, 36.0f32),
                                )
                                .clicked()
                                {
                                    self.cancel_client_connect();
                                }
                            });
                        }
                    });
                });

                ui.add_space(16.0f32);

                // Quick Connect / Recent Hosts Card
                if !self.recent_hosts.is_empty() {
                    theme::modern_card().show(ui, |ui| {
                        ui.label(RichText::new("⏱️ Quick Connect Recent Machines").size(15.0f32).strong().color(THEME.text_primary));
                        ui.add_space(8.0f32);

                        let mut to_connect = None;
                        for entry in &self.recent_hosts {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("💻").size(14.0f32));
                                ui.label(RichText::new(&entry.address).size(13.0f32).monospace().strong().color(THEME.text_primary));
                                ui.label(RichText::new(format!("({})", entry.timestamp_desc)).size(11.0f32).color(THEME.text_muted));

                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if ui.button(RichText::new("Fill Address").size(11.5f32)).clicked() {
                                        self.client_host_addr = entry.address.clone();
                                    }
                                    if ui.button(RichText::new("Connect Now").size(11.5f32).strong().color(THEME.primary_hover)).clicked() {
                                        to_connect = Some(entry.address.clone());
                                    }
                                });
                            });
                            ui.separator();
                        }

                        if let Some(addr) = to_connect {
                            self.client_host_addr = addr;
                            self.start_client_connect();
                        }
                    });

                    ui.add_space(16.0f32);
                }

                // Tailscale P2P Architecture Guide
                theme::modern_card().show(ui, |ui| {
                    ui.label(RichText::new("💡 Why Tailscale Remote Desktop?").size(15.0f32).strong().color(THEME.text_primary));
                    ui.add_space(6.0f32);
                    ui.label(
                        RichText::new(
                            "• Zero Cloud Relays: Traffic flows directly peer-to-peer over WireGuard mesh.\n\
                             • Ultra Low Latency: Sub-15ms round-trip ping when on fast internet or local networks.\n\
                             • End-to-End Security: Both screen packets and keyboard/mouse commands are authenticated."
                        )
                        .size(12.5f32)
                        .color(THEME.text_secondary),
                    );
                });
            });
            ui.add_space(24.0f32);
        });
    }
}

// -------------------------------------------------------------------------------------------------
// Tab 3: Recent Sessions
// -------------------------------------------------------------------------------------------------
impl UnifiedRemoteApp {
    fn render_recent_tab(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.add_space(24.0f32);
            ui.vertical(|ui| {
                ui.label(
                    RichText::new("Recent Sessions & Saved Hosts")
                        .size(24.0f32)
                        .strong()
                        .color(THEME.text_primary),
                );
                ui.label(
                    RichText::new("Quickly reconnect to remote workstations without retyping addresses.")
                        .size(13.0f32)
                        .color(THEME.text_secondary),
                );
                ui.add_space(16.0f32);

                theme::modern_card().show(ui, |ui| {
                    if self.recent_hosts.is_empty() {
                        ui.label(RichText::new("No recent connections yet. Connect to a remote host to save it here.").color(THEME.text_muted));
                    } else {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(format!("Saved Machines ({})", self.recent_hosts.len())).strong().color(THEME.text_primary));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.button("Clear History").clicked() {
                                    self.recent_hosts.clear();
                                    self.show_toast("Cleared recent history");
                                }
                            });
                        });
                        ui.add_space(10.0f32);

                        let mut to_connect = None;
                        for entry in &self.recent_hosts {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("🖥️").size(16.0f32));
                                ui.vertical(|ui| {
                                    ui.label(RichText::new(&entry.address).size(14.0f32).monospace().strong().color(THEME.text_primary));
                                    ui.label(RichText::new(&entry.timestamp_desc).size(11.0f32).color(THEME.text_muted));
                                });

                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if theme::primary_button(
                                        ui,
                                        RichText::new("🚀 Connect").size(12.0f32).strong().color(Color32::WHITE),
                                        Vec2::new(100.0f32, 30.0f32),
                                    )
                                    .clicked()
                                    {
                                        to_connect = Some(entry.address.clone());
                                    }
                                });
                            });
                            ui.separator();
                        }

                        if let Some(addr) = to_connect {
                            self.client_host_addr = addr;
                            self.current_tab = NavTab::Client;
                            self.start_client_connect();
                        }
                    }
                });
            });
            ui.add_space(24.0f32);
        });
    }
}

// -------------------------------------------------------------------------------------------------
// Tab 4: Settings & Performance
// -------------------------------------------------------------------------------------------------
impl UnifiedRemoteApp {
    fn render_settings_tab(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.add_space(24.0f32);
            ui.vertical(|ui| {
                ui.label(
                    RichText::new("Settings & Preferences")
                        .size(24.0f32)
                        .strong()
                        .color(THEME.text_primary),
                );
                ui.label(
                    RichText::new("Configure display scaling, input sensitivity, and network behavior.")
                        .size(13.0f32)
                        .color(THEME.text_secondary),
                );
                ui.add_space(16.0f32);

                // Remote Display Mode Card
                theme::modern_card().show(ui, |ui| {
                    ui.label(RichText::new("🖥️ Remote Screen Display Mode").size(16.0f32).strong().color(THEME.text_primary));
                    ui.add_space(8.0f32);

                    ui.horizontal(|ui| {
                        ui.radio_value(
                            &mut self.display_scaling,
                            DisplayScaling::FitAspect,
                            RichText::new("Fit to Window (Preserve Aspect Ratio)").size(13.0f32),
                        );
                        ui.radio_value(
                            &mut self.display_scaling,
                            DisplayScaling::Stretch,
                            RichText::new("Stretch to Fill Window").size(13.0f32),
                        );
                    });
                    ui.add_space(4.0f32);
                    ui.label(
                        RichText::new("Fit to Window ensures mouse cursor positions land with pixel perfection without visual distortion.")
                            .size(11.5f32)
                            .color(THEME.text_muted),
                    );
                });

                ui.add_space(16.0f32);

                // Protocol & Runtime Card
                theme::modern_card().show(ui, |ui| {
                    ui.label(RichText::new("⚡ Engine & Protocol Configuration").size(16.0f32).strong().color(THEME.text_primary));
                    ui.add_space(8.0f32);

                    egui::Grid::new("engine_specs_grid")
                        .spacing([20.0f32, 10.0f32])
                        .show(ui, |ui| {
                            ui.label("Protocol Version:");
                            ui.label(RichText::new("v1.0-tokio-xcap").monospace().color(THEME.primary_hover));
                            ui.end_row();

                            ui.label("Video Frame Queue:");
                            ui.label(RichText::new("4 frames (ultra-low latency buffer)").color(THEME.text_secondary));
                            ui.end_row();

                            ui.label("TCP Nodelay (Nagle off):");
                            ui.label(RichText::new("Enabled (sub-millisecond packet delivery)").color(THEME.success));
                            ui.end_row();

                            ui.label("Tokio Async Runtime:");
                            ui.label(RichText::new("Multi-threaded Worker Pool").color(THEME.text_secondary));
                            ui.end_row();
                        });
                });
            });
            ui.add_space(24.0f32);
        });
    }
}

// -------------------------------------------------------------------------------------------------
// Tab 5: Diagnostics & System Info
// -------------------------------------------------------------------------------------------------
impl UnifiedRemoteApp {
    fn render_diagnostics_tab(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let mut copy_trigger: Option<(String, &'static str)> = None;

        ui.horizontal(|ui| {
            ui.add_space(24.0f32);
            ui.vertical(|ui| {
                ui.label(
                    RichText::new("Diagnostics & Hardware")
                        .size(24.0f32)
                        .strong()
                        .color(THEME.text_primary),
                );
                ui.label(
                    RichText::new("Real-time network interfaces, screen capture diagnostics, and system stats.")
                        .size(13.0f32)
                        .color(THEME.text_secondary),
                );
                ui.add_space(16.0f32);

                theme::modern_card().show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("🔍 Network Interfaces").size(16.0f32).strong().color(THEME.text_primary));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("🔄 Re-scan Adapters").clicked() {
                                self.refresh_ips();
                            }
                        });
                    });
                    ui.add_space(10.0f32);

                    let ips = self.discovered_ips.clone();
                    for iface in &ips {
                        ui.horizontal(|ui| {
                            let (badge_color, border) = if iface.is_tailscale {
                                (THEME.tailscale_soft, THEME.tailscale_blue)
                            } else {
                                (THEME.bg_deep, THEME.border_subtle)
                            };
                            theme::modern_badge(badge_color, border).show(ui, |ui| {
                                ui.label(RichText::new(&iface.label).size(10.5f32).color(if iface.is_tailscale { THEME.tailscale_blue } else { THEME.text_secondary }));
                            });
                            ui.label(RichText::new(&iface.ip).monospace().strong().size(13.0f32).color(THEME.text_primary));

                            if ui.button("Copy").clicked() {
                                copy_trigger = Some((iface.ip.clone(), "Copied IP!"));
                            }
                        });
                        ui.add_space(4.0f32);
                    }
                });

                ui.add_space(16.0f32);

                theme::modern_card().show(ui, |ui| {
                    ui.label(RichText::new("💻 Subsystem Information").size(16.0f32).strong().color(THEME.text_primary));
                    ui.add_space(8.0f32);

                    egui::Grid::new("subsys_grid")
                        .spacing([20.0f32, 10.0f32])
                        .show(ui, |ui| {
                            ui.label("Operating System:");
                            ui.label(RichText::new(std::env::consts::OS).color(THEME.text_primary));
                            ui.end_row();

                            ui.label("CPU Architecture:");
                            ui.label(RichText::new(std::env::consts::ARCH).color(THEME.text_primary));
                            ui.end_row();

                            ui.label("Screen Capture Engine:");
                            ui.label(RichText::new("xcap 0.0.14 (DirectX Desktop Duplication / GDI)").color(THEME.text_secondary));
                            ui.end_row();

                            ui.label("Input Simulation:");
                            ui.label(RichText::new("Enigo 0.2 (Win32 SendInput Native)").color(THEME.text_secondary));
                            ui.end_row();

                            ui.label("Render Backend:");
                            ui.label(RichText::new("eframe / glow OpenGL Accelerated").color(THEME.text_secondary));
                            ui.end_row();
                        });
                });
            });
            ui.add_space(24.0f32);
        });

        if let Some((text, msg)) = copy_trigger {
            self.copy_text(ctx, &text, msg);
        }
    }
}

// -------------------------------------------------------------------------------------------------
// Remote Session Streaming View (Active Session)
// -------------------------------------------------------------------------------------------------
impl UnifiedRemoteApp {
    fn render_remote_session(&mut self, ctx: &egui::Context) {
        // Top Floating Control Bar
        egui::TopBottomPanel::top("modern_session_bar")
            .frame(
                egui::Frame::none()
                    .fill(THEME.bg_panel)
                    .stroke(Stroke::new(1.0f32, THEME.border_subtle))
                    .inner_margin(egui::Margin::symmetric(16.0f32, 10.0f32)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    // Live Session Badge
                    theme::modern_badge(THEME.danger_soft, THEME.danger).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.colored_label(THEME.danger, RichText::new("●").size(12.0f32));
                            ui.label(RichText::new("LIVE STREAM").size(11.0f32).strong().color(THEME.danger_hover));
                        });
                    });

                    ui.separator();

                    if let Some(conn) = &self.connection {
                        let rtt = conn.rtt_ms.load(Ordering::Relaxed);
                        let latency_color = if rtt < 35 {
                            THEME.success
                        } else if rtt < 80 {
                            THEME.warning
                        } else {
                            THEME.danger
                        };

                        // Latency pill
                        ui.label(RichText::new("⚡ Latency:").size(12.0f32).color(THEME.text_secondary));
                        ui.label(RichText::new(format!("{} ms", rtt)).strong().size(12.5f32).color(latency_color));

                        ui.separator();

                        // FPS pill
                        ui.label(RichText::new("🎯 FPS:").size(12.0f32).color(THEME.text_secondary));
                        ui.label(RichText::new(format!("{}", self.current_fps)).strong().size(12.5f32).color(THEME.text_primary));

                        ui.separator();

                        // Resolution pill
                        ui.label(RichText::new("🖥️ Resolution:").size(12.0f32).color(THEME.text_secondary));
                        ui.label(
                            RichText::new(format!("{}x{}", conn.screen_width, conn.screen_height))
                                .strong()
                                .size(12.5f32)
                                .color(THEME.text_primary),
                        );

                        ui.separator();

                        // Sync Clipboard button
                        if ui.button(RichText::new("📋 Sync Clipboard").size(12.0f32)).on_hover_text("Send local clipboard text to remote host").clicked() {
                            self.sync_local_clipboard_to_host();
                        }

                        // Display Scaling toggle
                        let scaling_text = match self.display_scaling {
                            DisplayScaling::FitAspect => "🔲 Mode: Fit",
                            DisplayScaling::Stretch => "🔲 Mode: Stretch",
                        };
                        if ui.button(RichText::new(scaling_text).size(12.0f32)).clicked() {
                            self.display_scaling = match self.display_scaling {
                                DisplayScaling::FitAspect => DisplayScaling::Stretch,
                                DisplayScaling::Stretch => DisplayScaling::FitAspect,
                            };
                        }

                        // Disconnect Button on right
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if theme::danger_button(
                                ui,
                                RichText::new("❌ Disconnect").size(12.0f32).strong().color(Color32::WHITE),
                                Vec2::new(110.0f32, 28.0f32),
                            )
                            .clicked()
                            {
                                self.disconnect_session();
                            }
                        });
                    }
                });
            });

        // Remote Video Viewport & Interaction Canvas
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(Color32::BLACK))
            .show(ctx, |ui| {
                if let Some(texture) = &self.video_texture {
                    let available_size = ui.available_size();

                    let (target_size, offset) = match self.display_scaling {
                        DisplayScaling::Stretch => (available_size, Vec2::ZERO),
                        DisplayScaling::FitAspect => {
                            let img_w = self.last_frame_size.0.max(1) as f32;
                            let img_h = self.last_frame_size.1.max(1) as f32;
                            let aspect = img_w / img_h;

                            let (w, h) = if available_size.x / available_size.y > aspect {
                                (available_size.y * aspect, available_size.y)
                            } else {
                                (available_size.x, available_size.x / aspect)
                            };

                            let ox = (available_size.x - w) * 0.5f32;
                            let oy = (available_size.y - h) * 0.5f32;
                            (Vec2::new(w, h), Vec2::new(ox, oy))
                        }
                    };

                    ui.vertical(|ui| {
                        if offset.y > 0.0f32 {
                            ui.add_space(offset.y);
                        }
                        ui.horizontal(|ui| {
                            if offset.x > 0.0f32 {
                                ui.add_space(offset.x);
                            }

                            let image_widget = egui::Image::new(texture)
                                .fit_to_exact_size(target_size)
                                .sense(egui::Sense::click_and_drag());

                            let response = ui.add(image_widget);
                            let rect = response.rect;

                            if let Some(conn) = &self.connection {
                                // Mouse cursor position tracking
                                if let Some(pointer_pos) = response.hover_pos() {
                                    let x_ratio = ((pointer_pos.x - rect.min.x) / rect.width()).clamp(0.0f32, 1.0f32);
                                    let y_ratio = ((pointer_pos.y - rect.min.y) / rect.height()).clamp(0.0f32, 1.0f32);
                                    let _ = conn.tx_input.try_send(InputEvent::MouseMove { x_ratio, y_ratio });
                                }

                                // Clicks & Drags
                                if response.drag_started_by(egui::PointerButton::Primary) || response.clicked_by(egui::PointerButton::Primary) {
                                    let _ = conn.tx_input.try_send(InputEvent::MouseDown {
                                        button: MouseBtn::Left,
                                    });
                                }
                                if response.drag_stopped_by(egui::PointerButton::Primary) || response.clicked_by(egui::PointerButton::Primary) {
                                    let _ = conn.tx_input.try_send(InputEvent::MouseUp {
                                        button: MouseBtn::Left,
                                    });
                                }

                                if response.drag_started_by(egui::PointerButton::Secondary) || response.clicked_by(egui::PointerButton::Secondary) {
                                    let _ = conn.tx_input.try_send(InputEvent::MouseDown {
                                        button: MouseBtn::Right,
                                    });
                                }
                                if response.drag_stopped_by(egui::PointerButton::Secondary) || response.clicked_by(egui::PointerButton::Secondary) {
                                    let _ = conn.tx_input.try_send(InputEvent::MouseUp {
                                        button: MouseBtn::Right,
                                    });
                                }

                                if response.clicked_by(egui::PointerButton::Middle) {
                                    let _ = conn.tx_input.try_send(InputEvent::MouseDown {
                                        button: MouseBtn::Middle,
                                    });
                                    let _ = conn.tx_input.try_send(InputEvent::MouseUp {
                                        button: MouseBtn::Middle,
                                    });
                                }

                                // Mouse scroll wheel
                                let scroll_delta = ctx.input(|i| i.raw_scroll_delta);
                                if scroll_delta.y.abs() > 0.1f32 || scroll_delta.x.abs() > 0.1f32 {
                                    let _ = conn.tx_input.try_send(InputEvent::MouseScroll {
                                        delta_x: scroll_delta.x as i32,
                                        delta_y: scroll_delta.y as i32,
                                    });
                                }

                                // Keyboard input
                                ctx.input(|i| {
                                    for event in &i.events {
                                        match event {
                                            egui::Event::Key { key, pressed, .. } => {
                                                let keycode = map_egui_key(*key);
                                                if *pressed {
                                                    let _ = conn.tx_input.try_send(InputEvent::KeyDown { key: keycode });
                                                } else {
                                                    let _ = conn.tx_input.try_send(InputEvent::KeyUp { key: keycode });
                                                }
                                            }
                                            egui::Event::Text(text) => {
                                                for c in text.chars() {
                                                    let _ = conn.tx_input.try_send(InputEvent::KeyDown {
                                                        key: KeyCode::Unicode(c),
                                                    });
                                                    let _ = conn.tx_input.try_send(InputEvent::KeyUp {
                                                        key: KeyCode::Unicode(c),
                                                    });
                                                }
                                            }
                                            _ => {}
                                        }
                                    }
                                });
                            }
                        });
                    });
                } else {
                    // Waiting for first video frame
                    ui.centered_and_justified(|ui| {
                        theme::modern_card_elevated().show(ui, |ui| {
                            ui.vertical_centered(|ui| {
                                ui.spinner();
                                ui.add_space(10.0f32);
                                ui.label(RichText::new("Negotiating Video Stream...").size(16.0f32).strong().color(THEME.text_primary));
                                ui.label(RichText::new("Connected to remote host. Awaiting incoming desktop frames.").size(12.5f32).color(THEME.text_secondary));
                            });
                        });
                    });
                }
            });
    }

    fn render_toast(&mut self, ctx: &egui::Context) {
        if let Some(toast) = &self.toast {
            if toast.created_at.elapsed().as_secs_f32() < toast.duration_secs {
                let text = toast.message.clone();
                egui::Area::new(egui::Id::new("floating_toast"))
                    .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-20.0f32, -20.0f32))
                    .show(ctx, |ui| {
                        egui::Frame::none()
                            .fill(THEME.bg_card_elevated)
                            .rounding(Rounding::same(8.0f32))
                            .stroke(Stroke::new(1.2f32, THEME.primary_hover))
                            .inner_margin(egui::Margin::symmetric(16.0f32, 10.0f32))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new("✓").size(14.0f32).color(THEME.success));
                                    ui.label(RichText::new(text).size(13.0f32).strong().color(THEME.text_primary));
                                });
                            });
                    });
            } else {
                self.toast = None;
            }
        }
    }
}

fn rand_val() -> u32 {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    ((now ^ (now >> 16)) % 900000 + 100000) as u32
}

fn map_egui_key(key: egui::Key) -> KeyCode {
    match key {
        egui::Key::Backspace => KeyCode::Backspace,
        egui::Key::Tab => KeyCode::Tab,
        egui::Key::Enter => KeyCode::Enter,
        egui::Key::Escape => KeyCode::Escape,
        egui::Key::Space => KeyCode::Space,
        egui::Key::ArrowDown => KeyCode::DownArrow,
        egui::Key::ArrowLeft => KeyCode::LeftArrow,
        egui::Key::ArrowRight => KeyCode::RightArrow,
        egui::Key::ArrowUp => KeyCode::UpArrow,
        egui::Key::Delete => KeyCode::Delete,
        egui::Key::Home => KeyCode::Home,
        egui::Key::End => KeyCode::End,
        egui::Key::PageUp => KeyCode::PageUp,
        egui::Key::PageDown => KeyCode::PageDown,
        egui::Key::F1 => KeyCode::F(1),
        egui::Key::F2 => KeyCode::F(2),
        egui::Key::F3 => KeyCode::F(3),
        egui::Key::F4 => KeyCode::F(4),
        egui::Key::F5 => KeyCode::F(5),
        egui::Key::F6 => KeyCode::F(6),
        egui::Key::F7 => KeyCode::F(7),
        egui::Key::F8 => KeyCode::F(8),
        egui::Key::F9 => KeyCode::F(9),
        egui::Key::F10 => KeyCode::F(10),
        egui::Key::F11 => KeyCode::F(11),
        egui::Key::F12 => KeyCode::F(12),
        _ => KeyCode::Other(0),
    }
}
