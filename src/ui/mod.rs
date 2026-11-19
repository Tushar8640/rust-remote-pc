pub mod theme;

use crate::client::connection::{connect_to_host, ConnectOptions, ConnectionHandle, DecodedFrameMessage};
use crate::common::net_info::{get_local_ips, InterfaceIp};
use crate::common::protocol::QualityMode;
use crate::common::transport;
use crate::common::protocol::{InputEvent, KeyCode, MouseBtn};
use crate::host::server::{HostServer, HostServerConfig};
use eframe::egui::{self, Color32, Margin, RichText, Rounding, Stroke, Vec2};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};
use theme::{BtnKind, C};
use tokio::sync::{broadcast, mpsc, oneshot};

const DEFAULT_PORT: u16 = 5885;
const MAX_RECENT: usize = 5;

// ═════════════════════════════════════════════════════════════════════════════
// Types
// ═════════════════════════════════════════════════════════════════════════════

#[derive(PartialEq, Clone, Copy)]
enum Tab {
    Host,
    Client,
    Premium,
}

#[derive(PartialEq, Clone, Copy)]
enum Scaling {
    Fit,
    Stretch,
}

struct Toast {
    msg: String,
    t: Instant,
}

struct Pending {
    rx: oneshot::Receiver<anyhow::Result<ConnectionHandle>>,
    addr: String,
}

// ═════════════════════════════════════════════════════════════════════════════
// App State
// ═════════════════════════════════════════════════════════════════════════════

pub struct UnifiedRemoteApp {
    tab: Tab,

    // Host
    host_port: String,
    host_password: String,
    show_host_pw: bool,
    host_fps: u32,
    host_quality: u8,
    host_audio: bool,
    host_running: bool,
    host_started: Option<Instant>,
    host_shutdown: Option<broadcast::Sender<()>>,
    ips: Vec<InterfaceIp>,
    p2p_key: iroh::SecretKey,
    p2p_id: String,
    host_mode: QualityMode,

    // Saved settings and premium relay
    settings: transport::Settings,
    show_relay_key: bool,
    /// Set when the user tried to turn on Premium without an active subscription or coupon.
    premium_gate: bool,
    relay_test: Option<oneshot::Receiver<Result<String, String>>>,
    relay_test_result: Option<(bool, String)>,

    // Client
    client_addr: String,
    client_password: String,
    show_client_pw: bool,
    client_msg: String,
    client_err: bool,
    connecting: bool,
    pending: Option<Pending>,
    recents: Vec<String>,
    focus_addr: bool,
    client_mode: QualityMode,

    // Session
    conn: Option<ConnectionHandle>,
    frame_rx: Option<mpsc::Receiver<DecodedFrameMessage>>,
    texture: Option<egui::TextureHandle>,
    frame_size: (u32, u32),
    scaling: Scaling,
    session_started: Instant,
    bar_pinned: bool,

    // Stats
    fps_count: u32,
    fps: u32,
    fps_tick: Instant,

    toast: Option<Toast>,
    rt: Arc<tokio::runtime::Runtime>,
}

impl UnifiedRemoteApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        initial_host: Option<String>,
        initial_password: Option<String>,
        tokio_runtime: Arc<tokio::runtime::Runtime>,
    ) -> Self {
        theme::apply_theme(&cc.egui_ctx);

        let ips = get_local_ips();
        let p2p_key = transport::load_or_create_secret_key();
        let settings = transport::load_settings();
        let recents = load_recents();
        let default_addr = if let Some(h) = initial_host {
            h
        } else if let Some(last) = recents.first() {
            last.clone()
        } else if let Some(ts) = ips.iter().find(|i| i.is_tailscale) {
            format!("{}:{}", ts.ip, DEFAULT_PORT)
        } else {
            String::new()
        };

        Self {
            tab: Tab::Host,
            host_port: DEFAULT_PORT.to_string(),
            // A fresh random password per launch unless one was supplied.
            host_password: initial_password.clone().unwrap_or_else(|| random_password(12)),
            show_host_pw: false,
            host_fps: 60,
            host_quality: 75,
            host_audio: true,
            host_running: false,
            host_started: None,
            host_shutdown: None,
            ips,
            p2p_id: transport::pairing_id(&p2p_key),
            p2p_key,
            host_mode: settings.host_quality,
            client_mode: settings.client_quality,
            settings,
            show_relay_key: false,
            premium_gate: false,
            relay_test: None,
            relay_test_result: None,

            client_addr: default_addr,
            client_password: initial_password.unwrap_or_default(),
            show_client_pw: false,
            client_msg: String::new(),
            client_err: false,
            connecting: false,
            pending: None,
            recents,
            focus_addr: true,

            conn: None,
            frame_rx: None,
            texture: None,
            frame_size: (0, 0),
            scaling: Scaling::Fit,
            session_started: Instant::now(),
            bar_pinned: false,

            fps_count: 0,
            fps: 0,
            fps_tick: Instant::now(),

            toast: None,
            rt: tokio_runtime,
        }
    }

    /// Opens the page where a subscription is bought or renewed.
    fn open_purchase_page(&mut self) {
        let url = self.settings.purchase_url.trim().to_string();
        if url.is_empty() {
            self.toast("No subscription page is set up yet. Ask your provider for a licence.");
        } else if !transport::open_link(&url) {
            self.toast("Could not open the subscription page.");
        }
    }

    /// Persists mode choices and the premium relay settings.
    fn save_settings(&mut self) {
        self.settings.host_quality = self.host_mode;
        self.settings.client_quality = self.client_mode;
        transport::save_settings(&self.settings);
    }

    fn toast(&mut self, s: &str) {
        self.toast = Some(Toast { msg: s.to_string(), t: Instant::now() });
    }

    fn copy(&mut self, ctx: &egui::Context, s: &str, msg: &str) {
        ctx.output_mut(|o| o.copied_text = s.to_string());
        if let Ok(mut cb) = arboard::Clipboard::new() {
            let _ = cb.set_text(s);
        }
        self.toast(msg);
    }

    fn primary_ip(&self) -> String {
        self.ips
            .iter()
            .find(|i| i.is_tailscale)
            .or(self.ips.first())
            .map(|i| i.ip.clone())
            .unwrap_or_else(|| "127.0.0.1".to_string())
    }

    fn has_tailscale(&self) -> bool {
        self.ips.iter().any(|i| i.is_tailscale)
    }

    fn host_port_num(&self) -> Option<u16> {
        self.host_port.trim().parse::<u16>().ok().filter(|p| *p > 0)
    }

    fn start_host(&mut self) {
        if self.host_running {
            return;
        }
        let Some(port) = self.host_port_num() else {
            self.toast("Enter a valid port (1-65535)");
            return;
        };
        if self.host_password.is_empty() {
            self.toast("Set a password first");
            return;
        }
        let premium_relay = match self.settings.host_relay() {
            Ok(r) => r,
            Err(e) => {
                self.toast(&e);
                return;
            }
        };
        let cfg = HostServerConfig {
            bind_addr: format!("0.0.0.0:{}", port),
            password: self.host_password.clone(),
            target_fps: self.host_fps,
            jpeg_quality: self.host_quality,
            audio_enabled: self.host_audio,
            p2p_key: Some(self.p2p_key.clone()),
            quality_mode: self.host_mode,
            premium_relay,
        };
        let srv = HostServer::new(cfg);
        let (tx, rx) = broadcast::channel(1);
        self.host_shutdown = Some(tx);
        self.host_running = true;
        self.host_started = Some(Instant::now());
        self.toast("Server started");
        self.rt.spawn(async move {
            let _ = srv.run_with_shutdown(rx).await;
        });
    }

    fn stop_host(&mut self) {
        if let Some(tx) = self.host_shutdown.take() {
            let _ = tx.send(());
        }
        self.host_running = false;
        self.host_started = None;
        self.toast("Server stopped");
    }

    fn connect(&mut self) {
        if self.connecting || self.conn.is_some() {
            return;
        }
        let addr = match normalize_addr(&self.client_addr) {
            Ok(a) => a,
            Err(e) => {
                self.client_msg = e;
                self.client_err = true;
                return;
            }
        };
        self.client_addr = addr.clone();
        self.connecting = true;
        self.client_msg.clear();
        self.client_err = false;

        let pw = self.client_password.clone();
        let (tx_f, rx_f) = mpsc::channel::<DecodedFrameMessage>(4);
        self.frame_rx = Some(rx_f);

        let (tx_r, rx_r) = oneshot::channel();
        let a = addr.clone();
        let opts = ConnectOptions {
            premium: None, // a join code supplies the relay and pass
            quality: self.client_mode,
        };
        self.rt.spawn(async move {
            let r = connect_to_host(&a, &pw, tx_f, opts).await;
            let _ = tx_r.send(r);
        });
        self.pending = Some(Pending { rx: rx_r, addr });
    }

    fn cancel_connect(&mut self) {
        self.pending = None;
        self.connecting = false;
        self.frame_rx = None;
        self.client_msg.clear();
    }

    fn disconnect(&mut self, ctx: &egui::Context) {
        self.conn = None;
        self.frame_rx = None;
        self.texture = None;
        self.client_msg.clear();
        self.bar_pinned = false;
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
        self.toast("Disconnected");
    }

    fn remember(&mut self, addr: &str) {
        // Join codes expire within a day, so they are not worth saving.
        if transport::is_join_code(addr) {
            return;
        }
        self.recents.retain(|a| a != addr);
        self.recents.insert(0, addr.to_string());
        self.recents.truncate(MAX_RECENT);
        save_recents(&self.recents);
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Main update loop
// ═════════════════════════════════════════════════════════════════════════════

impl eframe::App for UnifiedRemoteApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Only spin at full rate while streaming; idle screens redraw slowly.
        if self.conn.is_some() {
            ctx.request_repaint();
        } else if self.host_running || self.connecting || self.relay_test.is_some() {
            ctx.request_repaint_after(Duration::from_millis(33));
        } else {
            ctx.request_repaint_after(Duration::from_millis(500));
        }

        // FPS = decoded video frames per second
        if self.fps_tick.elapsed().as_secs() >= 1 {
            self.fps = self.fps_count;
            self.fps_count = 0;
            self.fps_tick = Instant::now();
        }

        // Poll the premium relay connection test (non-blocking)
        if let Some(mut rx) = self.relay_test.take() {
            match rx.try_recv() {
                Ok(r) => {
                    self.relay_test_result = Some(match r {
                        Ok(m) => (true, m),
                        Err(e) => (false, e),
                    });
                }
                Err(oneshot::error::TryRecvError::Empty) => self.relay_test = Some(rx),
                Err(_) => self.relay_test_result = Some((false, "The test did not finish.".to_string())),
            }
        }

        // Poll pending connection (non-blocking)
        if let Some(mut p) = self.pending.take() {
            match p.rx.try_recv() {
                Ok(Ok(h)) => {
                    self.connecting = false;
                    self.client_msg.clear();
                    self.conn = Some(h);
                    self.session_started = Instant::now();
                    self.remember(&p.addr);
                    self.toast("Connected");
                }
                Ok(Err(e)) => {
                    self.connecting = false;
                    self.client_msg = friendly_error(&e.to_string());
                    self.client_err = true;
                    self.frame_rx = None;
                }
                Err(oneshot::error::TryRecvError::Empty) => {
                    self.pending = Some(p);
                }
                Err(_) => {
                    self.connecting = false;
                    self.client_msg = "Connection lost before it was established.".to_string();
                    self.client_err = true;
                    self.frame_rx = None;
                }
            }
        }

        // Consume frames: upload only the newest one to the GPU.
        if let Some(rx) = &mut self.frame_rx {
            let mut latest = None;
            let mut n = 0u32;
            while let Ok(m) = rx.try_recv() {
                latest = Some(m);
                n += 1;
            }
            if let Some(m) = latest {
                let img = egui::ColorImage::from_rgba_unmultiplied(
                    [m.width as usize, m.height as usize],
                    &m.rgba_data,
                );
                self.frame_size = (m.width, m.height);
                self.fps_count += n;
                match &mut self.texture {
                    Some(t) => t.set(img, egui::TextureOptions::LINEAR),
                    None => {
                        self.texture =
                            Some(ctx.load_texture("screen", img, egui::TextureOptions::LINEAR))
                    }
                }
            }
        }

        // ── Active session → full-screen remote view ──
        if self.conn.is_some() {
            self.render_session(ctx);
            self.render_toast(ctx);
            return;
        }

        // ── Sidebar ──
        egui::SidePanel::left("nav")
            .exact_width(208.0f32)
            .resizable(false)
            .frame(
                egui::Frame::none()
                    .fill(C.bg_surface)
                    .inner_margin(Margin::symmetric(14.0f32, 18.0f32)),
            )
            .show(ctx, |ui| {
                let r = ui.max_rect();
                ui.painter().vline(
                    r.right() + 14.0f32,
                    r.y_range(),
                    Stroke::new(1.0f32, C.border),
                );

                // Brand
                ui.horizontal(|ui| {
                    ui.add_space(4.0f32);
                    let (rect, _) = ui.allocate_exact_size(Vec2::splat(24.0f32), egui::Sense::hover());
                    ui.painter().rect_filled(rect, Rounding::same(7.0f32), C.accent);
                    theme::icon_monitor(ui, rect, Color32::WHITE);
                    ui.add_space(4.0f32);
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 0.0f32;
                        ui.label(RichText::new("Remote Desktop").size(13.5f32).strong());
                        ui.label(RichText::new("over Tailscale").size(10.5f32).color(C.text_faint));
                    });
                });

                ui.add_space(22.0f32);
                theme::section_label(ui, "Menu");
                ui.add_space(2.0f32);

                let host_dot = self.host_running.then_some(C.green);
                if theme::nav_item(ui, "Share this PC", self.tab == Tab::Host, host_dot, theme::icon_monitor)
                    .clicked()
                {
                    self.tab = Tab::Host;
                }
                if theme::nav_item(ui, "Connect", self.tab == Tab::Client, None, theme::icon_connect)
                    .clicked()
                {
                    self.tab = Tab::Client;
                    self.focus_addr = true;
                }
                let premium_dot = self.settings.premium_enabled.then_some(C.amber);
                if theme::nav_item(ui, "Premium relay", self.tab == Tab::Premium, premium_dot, theme::icon_connect)
                    .clicked()
                {
                    self.tab = Tab::Premium;
                }

                // Network status pinned to the bottom
                ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                    let ts = self.has_tailscale();
                    theme::card().inner_margin(Margin::same(12.0f32)).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            let c = if ts { C.cyan } else { C.amber };
                            ui.colored_label(c, RichText::new("●").size(9.0f32));
                            ui.label(
                                RichText::new(if ts { "Tailscale" } else { "Local network" })
                                    .size(12.0f32)
                                    .strong(),
                            );
                        });
                        ui.label(
                            RichText::new(self.primary_ip())
                                .size(11.5f32)
                                .monospace()
                                .color(C.text_dim),
                        );
                    });
                });
            });

        // ── Content ──
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(C.bg_base))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    let avail = ui.available_width();
                    let max_w = 620.0f32;
                    let pad = ((avail - max_w) * 0.5f32).max(28.0f32);
                    ui.horizontal_top(|ui| {
                        ui.add_space(pad);
                        ui.vertical(|ui| {
                            ui.set_width((avail - pad * 2.0f32).clamp(300.0f32, max_w));
                            ui.add_space(30.0f32);
                            match self.tab {
                                Tab::Host => self.render_host(ui, ctx),
                                Tab::Client => self.render_client(ui),
                                Tab::Premium => self.render_premium(ui, ctx),
                            }
                            ui.add_space(40.0f32);
                        });
                    });
                });
            });

        self.render_toast(ctx);
    }
}

fn page_header(ui: &mut egui::Ui, title: &str, sub: &str) {
    ui.label(RichText::new(title).size(24.0f32).strong());
    ui.add_space(2.0f32);
    ui.label(RichText::new(sub).size(13.0f32).color(C.text_dim));
    ui.add_space(22.0f32);
}

fn field_label(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).size(12.0f32).color(C.text_dim));
}

// ═════════════════════════════════════════════════════════════════════════════
// Host page
// ═════════════════════════════════════════════════════════════════════════════

impl UnifiedRemoteApp {
    fn render_host(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let mut copy_trigger: Option<(String, &'static str)> = None;

        page_header(
            ui,
            "Share this PC",
            "Let another device view and control this desktop.",
        );

        // ── Status hero ──
        theme::card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                // Pulsing status indicator
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(26.0f32), egui::Sense::hover());
                let c = rect.center();
                if self.host_running {
                    let t = ctx.input(|i| i.time) as f32;
                    let pulse = (t * 2.2f32).sin() * 0.5f32 + 0.5f32;
                    ui.painter()
                        .circle_filled(c, 7.0f32 + pulse * 5.0f32, C.green.linear_multiply(0.18f32 * (1.0f32 - pulse)));
                    ui.painter().circle_filled(c, 6.0f32, C.green);
                } else {
                    ui.painter().circle_stroke(c, 6.0f32, Stroke::new(1.6f32, C.text_faint));
                }

                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 2.0f32;
                    ui.label(
                        RichText::new(if self.host_running { "Sharing is on" } else { "Sharing is off" })
                            .size(15.0f32)
                            .strong(),
                    );
                    let sub = match self.host_started {
                        Some(t) if self.host_running => format!(
                            "Running for {} · port {}",
                            fmt_duration(t.elapsed()),
                            self.host_port
                        ),
                        _ => "Start the server to accept connections.".to_string(),
                    };
                    ui.label(RichText::new(sub).size(12.0f32).color(C.text_dim));
                });

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if self.host_running {
                        if theme::button(ui, "Stop sharing", BtnKind::Danger, 120.0f32).clicked() {
                            self.stop_host();
                        }
                    } else if theme::button(ui, "Start sharing", BtnKind::Success, 120.0f32).clicked() {
                        self.start_host();
                    }
                });
            });
        });

        ui.add_space(14.0f32);

        // ── Connection details ──
        let ip = self.primary_ip();
        let port = self.host_port.trim().to_string();
        let full_addr = format!("{}:{}", ip, port);

        // ── Pairing ID (works from any network) ──
        let p2p_id = self.p2p_id.clone();

        // ── Connection mode: standard (free public relays) or premium (your relay) ──
        let mut open_premium_page = false;
        let mut open_purchase = false;
        theme::card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                theme::section_label(ui, "Connection");
                if self.host_running {
                    ui.label(RichText::new("· stop sharing to change").size(11.0f32).color(C.text_faint));
                }
            });
            ui.add_space(8.0f32);
            ui.add_enabled_ui(!self.host_running, |ui| {
                let before = self.settings.premium_enabled;
                theme::segmented(
                    ui,
                    &mut self.settings.premium_enabled,
                    &[(false, "Standard"), (true, "Premium")],
                );
                if self.settings.premium_enabled != before {
                    if self.settings.premium_enabled && self.settings.premium_blocker().is_some() {
                        // Premium needs a subscription or a coupon: stay on Standard and say why.
                        self.settings.premium_enabled = false;
                        self.premium_gate = true;
                    } else {
                        self.premium_gate = false;
                        self.save_settings();
                    }
                }
            });
            ui.add_space(6.0f32);
            let note = if self.settings.premium_enabled {
                "Your premium relay carries the connection. Share the join code below; clients need nothing else."
            } else {
                "Free public relays. Works anywhere, but delay can be higher."
            };
            ui.label(RichText::new(note).size(11.5f32).color(C.text_faint));

            // Premium is locked until there is an active subscription or a coupon. Shown when
            // the user tried to switch it on, or when it was on and the subscription has ended.
            let blocker = self.settings.premium_blocker();
            if blocker.is_some() && (self.premium_gate || self.settings.premium_enabled) {
                ui.add_space(10.0f32);
                theme::banner(C.amber).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new(blocker.unwrap_or_default()).size(12.0f32).color(C.amber));
                });
                ui.add_space(6.0f32);
                ui.horizontal(|ui| {
                    if theme::button_sized(ui, "Subscribe", BtnKind::Primary, 100.0f32, 28.0f32, 12.0f32).clicked() {
                        open_purchase = true;
                    }
                    if theme::button_sized(ui, "Enter coupon or licence", BtnKind::Ghost, 0.0f32, 28.0f32, 12.0f32).clicked() {
                        open_premium_page = true;
                    }
                });
            }
        });
        if open_premium_page {
            self.tab = Tab::Premium;
        }
        if open_purchase {
            self.open_purchase_page();
        }
        ui.add_space(14.0f32);

        // ── What to share ──
        let join_code = if self.settings.premium_configured() {
            self.settings.join_code(&p2p_id)
        } else {
            None
        };
        theme::card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            if let Some(code) = &join_code {
                ui.horizontal(|ui| {
                    theme::section_label(ui, "Join code · premium");
                    theme::pill(C.amber).show(ui, |ui| {
                        ui.label(RichText::new("Premium relay · encrypted").size(10.5f32).color(C.amber));
                    });
                });
                ui.add_space(8.0f32);
                ui.add(
                    egui::Label::new(RichText::new(transport::display_target(code)).size(15.0f32).monospace().strong()),
                );
                ui.add_space(6.0f32);
                ui.horizontal(|ui| {
                    if theme::button_sized(ui, "Copy join code", BtnKind::Primary, 130.0f32, 28.0f32, 12.0f32).clicked() {
                        copy_trigger = Some((code.clone(), "Join code copied"));
                    }
                    if theme::button_sized(ui, "Copy ID only", BtnKind::Ghost, 100.0f32, 28.0f32, 12.0f32).clicked() {
                        copy_trigger = Some((p2p_id.clone(), "Pairing ID copied"));
                    }
                });
                ui.add_space(6.0f32);
                ui.label(
                    RichText::new(
                        "Send the join code and the password. The other device pastes the code and connects through your relay. The code expires after about a day; copy a fresh one when needed.",
                    )
                    .size(11.5f32)
                    .color(C.text_faint),
                );
            } else {
                ui.horizontal(|ui| {
                    theme::section_label(ui, "Pairing ID · connect from anywhere");
                    theme::pill(C.green).show(ui, |ui| {
                        ui.label(RichText::new("Internet · encrypted").size(10.5f32).color(C.green));
                    });
                });
                ui.add_space(8.0f32);
                ui.horizontal(|ui| {
                    ui.add(
                        egui::Label::new(RichText::new(&p2p_id).size(12.5f32).monospace().strong())
                            .wrap(),
                    );
                });
                ui.add_space(6.0f32);
                ui.horizontal(|ui| {
                    if theme::button_sized(ui, "Copy ID", BtnKind::Ghost, 84.0f32, 28.0f32, 12.0f32).clicked() {
                        copy_trigger = Some((p2p_id.clone(), "Pairing ID copied"));
                    }
                    ui.label(
                        RichText::new(if self.host_running {
                            "Enter this ID and the password on the other device. No VPN or port forwarding needed."
                        } else {
                            "Start sharing, then enter this ID and the password on the other device."
                        })
                        .size(11.5f32)
                        .color(C.text_faint),
                    );
                });
            }
        });

        ui.add_space(14.0f32);

        theme::card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::section_label(ui, "Same network or Tailscale");
            ui.add_space(8.0f32);

            ui.horizontal(|ui| {
                ui.label(RichText::new(&full_addr).size(20.0f32).monospace().strong());
                ui.add_space(6.0f32);
                if self.has_tailscale() {
                    theme::pill(C.cyan).show(ui, |ui| {
                        ui.label(RichText::new("Tailscale").size(10.5f32).color(C.cyan));
                    });
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if theme::button_sized(ui, "Copy", BtnKind::Ghost, 64.0f32, 28.0f32, 12.0f32).clicked() {
                        copy_trigger = Some((full_addr.clone(), "Address copied"));
                    }
                });
            });

            ui.add_space(12.0f32);
            ui.separator();
            ui.add_space(8.0f32);

            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    field_label(ui, "Password");
                    let shown = if self.show_host_pw {
                        self.host_password.clone()
                    } else {
                        "•".repeat(self.host_password.chars().count().clamp(0, 24))
                    };
                    ui.label(RichText::new(shown).size(15.0f32).monospace());
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if theme::button_sized(ui, "Copy", BtnKind::Ghost, 64.0f32, 28.0f32, 12.0f32).clicked() {
                        copy_trigger = Some((self.host_password.clone(), "Password copied"));
                    }
                    let lbl = if self.show_host_pw { "Hide" } else { "Show" };
                    if theme::button_sized(ui, lbl, BtnKind::Ghost, 64.0f32, 28.0f32, 12.0f32).clicked() {
                        self.show_host_pw = !self.show_host_pw;
                    }
                });
            });

            if !self.has_tailscale() {
                ui.add_space(10.0f32);
                theme::banner(C.amber).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(
                        RichText::new("Tailscale was not detected. This address only works on your local network. Use the Pairing ID above to connect from anywhere.")
                            .size(12.0f32)
                            .color(C.amber),
                    );
                });
            }
        });

        ui.add_space(14.0f32);

        // ── Settings ──
        theme::card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                theme::section_label(ui, "Settings");
                if self.host_running {
                    ui.label(RichText::new("· stop sharing to change").size(11.0f32).color(C.text_faint));
                }
            });
            ui.add_space(8.0f32);

            ui.add_enabled_ui(!self.host_running, |ui| {
                egui::Grid::new("host_grid")
                    .num_columns(2)
                    .min_col_width(100.0f32)
                    .spacing([16.0f32, 14.0f32])
                    .show(ui, |ui| {
                        field_label(ui, "Password");
                        ui.horizontal(|ui| {
                            theme::text_field(ui, &mut self.host_password, "password", 170.0f32, !self.show_host_pw, false);
                            if theme::button_sized(ui, "Generate", BtnKind::Ghost, 0.0f32, 30.0f32, 12.0f32).clicked() {
                                self.host_password = random_password(12);
                                self.show_host_pw = true;
                                self.toast("New password generated");
                            }
                        });
                        ui.end_row();

                        field_label(ui, "Port");
                        ui.horizontal(|ui| {
                            let bad = self.host_port_num().is_none();
                            theme::text_field(ui, &mut self.host_port, "5885", 80.0f32, false, true);
                            if bad {
                                ui.label(RichText::new("1-65535").size(11.0f32).color(C.red));
                            }
                        });
                        ui.end_row();

                        field_label(ui, "Frame rate");
                        theme::segmented(ui, &mut self.host_fps, &[(30u32, "30 fps"), (60u32, "60 fps")]);
                        ui.end_row();

                        field_label(ui, "Image quality");
                        ui.horizontal(|ui| {
                            ui.add(egui::Slider::new(&mut self.host_quality, 50..=95).suffix("%"));
                        });
                        ui.end_row();

                        field_label(ui, "Share audio");
                        theme::segmented(ui, &mut self.host_audio, &[(true, "On"), (false, "Off")]);
                        ui.end_row();

                        field_label(ui, "Quality mode");
                        let before = self.host_mode;
                        theme::segmented(
                            ui,
                            &mut self.host_mode,
                            &[
                                (QualityMode::Auto, "Auto"),
                                (QualityMode::Smooth, "Smooth"),
                                (QualityMode::Sharp, "Sharp"),
                                (QualityMode::DataSaver, "Saver"),
                            ],
                        );
                        if self.host_mode != before {
                            self.save_settings();
                        }
                        ui.end_row();
                    });
            });
        });

        ui.add_space(14.0f32);

        // ── Network interfaces ──
        let ips = self.ips.clone();
        egui::CollapsingHeader::new(RichText::new("Network interfaces").size(12.0f32).color(C.text_dim))
            .default_open(false)
            .show(ui, |ui| {
                ui.add_space(4.0f32);
                for item in &ips {
                    ui.horizontal(|ui| {
                        let c = if item.is_tailscale { C.cyan } else { C.text_faint };
                        ui.colored_label(c, RichText::new("●").size(8.0f32));
                        ui.label(RichText::new(&item.ip).size(12.0f32).monospace());
                        ui.label(RichText::new(&item.label).size(11.0f32).color(C.text_faint));
                        if theme::button_sized(ui, "Copy", BtnKind::Ghost, 0.0f32, 22.0f32, 11.0f32).clicked() {
                            copy_trigger = Some((item.ip.clone(), "IP copied"));
                        }
                    });
                }
            });

        if let Some((txt, msg)) = copy_trigger {
            self.copy(ctx, &txt, msg);
        }
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Client page
// ═════════════════════════════════════════════════════════════════════════════

impl UnifiedRemoteApp {
    fn render_client(&mut self, ui: &mut egui::Ui) {
        page_header(
            ui,
            "Connect to a PC",
            "Paste the join code or Pairing ID (or an address) and the password shown on the host.",
        );

        let mut submit = false;

        theme::card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.add_enabled_ui(!self.connecting, |ui| {
                field_label(ui, "Join code, Pairing ID or address");
                let w = ui.available_width();
                let r = theme::text_field(ui, &mut self.client_addr, "Paste a join code or Pairing ID", w, false, true);
                if self.focus_addr {
                    r.request_focus();
                    self.focus_addr = false;
                }
                if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    submit = true;
                }

                ui.add_space(6.0f32);
                field_label(ui, "Password");
                ui.horizontal(|ui| {
                    let w = ui.available_width() - 76.0f32;
                    let r = theme::text_field(
                        ui,
                        &mut self.client_password,
                        "password",
                        w.max(120.0f32),
                        !self.show_client_pw,
                        false,
                    );
                    if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        submit = true;
                    }
                    let lbl = if self.show_client_pw { "Hide" } else { "Show" };
                    if theme::button_sized(ui, lbl, BtnKind::Ghost, 64.0f32, 34.0f32, 12.0f32).clicked() {
                        self.show_client_pw = !self.show_client_pw;
                    }
                });
            });

            if !self.client_msg.is_empty() {
                ui.add_space(10.0f32);
                theme::banner(C.red).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new(&self.client_msg).size(12.5f32).color(C.red));
                });
            }

            ui.add_space(14.0f32);

            if self.connecting {
                ui.horizontal(|ui| {
                    ui.spinner();
                    let addr = self
                        .pending
                        .as_ref()
                        .map(|p| transport::display_target(&p.addr))
                        .unwrap_or_else(|| "host".to_string());
                    ui.label(
                        RichText::new(format!("Connecting to {}…", addr))
                            .size(12.5f32)
                            .color(C.accent_hover),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if theme::button(ui, "Cancel", BtnKind::Ghost, 90.0f32).clicked() {
                            self.cancel_connect();
                        }
                    });
                });
            } else {
                let w = ui.available_width();
                if theme::button_sized(ui, "Connect", BtnKind::Primary, w, 38.0f32, 14.0f32).clicked() {
                    submit = true;
                }
            }
        });

        if submit {
            self.connect();
        }

        // ── Recent hosts ──
        if !self.recents.is_empty() && !self.connecting {
            ui.add_space(22.0f32);
            theme::section_label(ui, "Recent");
            ui.add_space(4.0f32);

            let mut pick: Option<String> = None;
            let mut forget: Option<String> = None;
            for addr in &self.recents {
                let (rect, resp) = ui.allocate_exact_size(
                    Vec2::new(ui.available_width(), 40.0f32),
                    egui::Sense::click(),
                );
                let hovered = resp.hovered();
                ui.painter().rect(
                    rect,
                    Rounding::same(8.0f32),
                    if hovered { C.bg_hover } else { C.bg_surface },
                    Stroke::new(1.0f32, if hovered { C.border_hover } else { C.border }),
                );
                theme::icon_monitor(ui, egui::Rect::from_center_size(
                    egui::pos2(rect.left() + 24.0f32, rect.center().y),
                    Vec2::splat(18.0f32),
                ), C.text_dim);
                ui.painter().text(
                    egui::pos2(rect.left() + 44.0f32, rect.center().y),
                    egui::Align2::LEFT_CENTER,
                    transport::display_target(addr),
                    egui::FontId::monospace(13.0f32),
                    C.text,
                );
                if hovered {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    ui.painter().text(
                        egui::pos2(rect.right() - 14.0f32, rect.center().y),
                        egui::Align2::RIGHT_CENTER,
                        "Use",
                        egui::FontId::proportional(11.5f32),
                        C.text_dim,
                    );
                }
                if resp.clicked() {
                    pick = Some(addr.clone());
                }
                resp.context_menu(|ui| {
                    if ui.button("Remove from recent").clicked() {
                        forget = Some(addr.clone());
                        ui.close_menu();
                    }
                });
            }
            if let Some(a) = pick {
                self.client_addr = a;
                self.client_msg.clear();
            }
            if let Some(a) = forget {
                self.recents.retain(|r| *r != a);
                save_recents(&self.recents);
            }
            ui.add_space(4.0f32);
            ui.label(RichText::new("Right-click an entry to remove it.").size(11.0f32).color(C.text_faint));
        }
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Premium page (host setup)
// ═════════════════════════════════════════════════════════════════════════════

impl UnifiedRemoteApp {
    fn render_premium(&mut self, ui: &mut egui::Ui, _ctx: &egui::Context) {
        page_header(
            ui,
            "Premium relay",
            "Run this PC's connections through your own relay server for lower delay and steadier quality.",
        );

        let mut changed = false;
        let mut run_test = false;
        let mut clear = false;
        let mut subscribe = false;

        theme::card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::section_label(ui, "Set up this PC as a premium host");
            ui.add_space(8.0f32);

            field_label(ui, "Relay URL");
            let w = ui.available_width();
            changed |= theme::text_field(
                ui,
                &mut self.settings.premium_url,
                "https://your-relay.onrender.com",
                w,
                false,
                true,
            )
            .changed();

            ui.add_space(6.0f32);
            field_label(ui, "Coupon or licence code");
            ui.horizontal(|ui| {
                let w = ui.available_width() - 76.0f32;
                changed |= theme::text_field(
                    ui,
                    &mut self.settings.premium_license,
                    "RRPC-… coupon or rrpc-lic2.… licence",
                    w.max(120.0f32),
                    !self.show_relay_key,
                    false,
                )
                .changed();
                let lbl = if self.show_relay_key { "Hide" } else { "Show" };
                if theme::button_sized(ui, lbl, BtnKind::Ghost, 64.0f32, 34.0f32, 12.0f32).clicked() {
                    self.show_relay_key = !self.show_relay_key;
                }
            });

            // Status of the code that was entered
            ui.add_space(8.0f32);
            let activation = self.settings.activation();
            let status_color = match &activation {
                transport::Activation::Active { .. } => C.green,
                transport::Activation::NotSet => C.text_dim,
                _ => C.amber,
            };
            ui.label(RichText::new(activation.describe()).size(12.5f32).color(status_color));

            ui.add_space(12.0f32);
            ui.horizontal(|ui| {
                let configured = self.settings.premium_configured() && activation.is_active();
                ui.add_enabled_ui(configured && self.relay_test.is_none(), |ui| {
                    if theme::button(ui, "Test connection", BtnKind::Primary, 140.0f32).clicked() {
                        run_test = true;
                    }
                });
                ui.add_enabled_ui(
                    !self.settings.premium_license.is_empty() || self.settings.premium_url != transport::DEFAULT_PREMIUM_URL,
                    |ui| {
                        if theme::button(ui, "Clear", BtnKind::Ghost, 80.0f32).clicked() {
                            clear = true;
                        }
                    },
                );
                if theme::button(ui, "Subscribe / renew", BtnKind::Ghost, 140.0f32).clicked() {
                    subscribe = true;
                }
                if self.relay_test.is_some() {
                    ui.spinner();
                    ui.label(RichText::new("Testing…").size(12.5f32).color(C.accent_hover));
                }
            });

            if let Some((ok, msg)) = &self.relay_test_result {
                ui.add_space(10.0f32);
                let c = if *ok { C.green } else { C.red };
                theme::banner(c).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new(msg).size(12.5f32).color(c));
                });
            }
        });

        ui.add_space(14.0f32);
        theme::card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::section_label(ui, "How it works");
            ui.add_space(6.0f32);
            for line in [
                "Premium is a paid feature. Subscribe to get a licence key, or enter a coupon code if you were given one.",
                "1. Enter the relay URL and your coupon or licence code, then press Test connection.",
                "2. On Share this PC, set Connection to Premium and start sharing.",
                "3. Copy the join code and send it, with the password, to whoever should connect.",
                "4. They paste the join code on the Connect page (or in the Android app). They need no licence or settings.",
                "The code stays on this PC. Join codes carry a short-lived pass instead, so a leaked join code stops working on its own.",
                "A subscription licence stops working when it ends; renew it to keep Premium. Coupons do not expire.",
                "The code is stored on this PC in plain text.",
            ] {
                ui.label(RichText::new(line).size(12.0f32).color(C.text_dim));
                ui.add_space(3.0f32);
            }
        });

        if changed {
            self.relay_test_result = None;
            self.premium_gate = false;
            self.save_settings();
        }
        if subscribe {
            self.open_purchase_page();
        }
        if clear {
            self.settings.premium_license.clear();
            self.settings.premium_url = transport::DEFAULT_PREMIUM_URL.to_string();
            self.settings.premium_enabled = false;
            self.relay_test_result = None;
            self.save_settings();
            self.toast("Premium settings cleared");
        }
        if run_test {
            let mut s = self.settings.clone();
            s.premium_enabled = true;
            self.relay_test_result = None;
            match s.host_relay() {
                Ok(Some(p)) => {
                    let (tx, rx) = oneshot::channel();
                    self.relay_test = Some(rx);
                    self.rt.spawn(async move {
                        let _ = tx.send(transport::test_premium_relay(&p).await);
                    });
                }
                Ok(None) => {}
                Err(e) => self.relay_test_result = Some((false, e)),
            }
        }
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Remote session (full-screen streaming view)
// ═════════════════════════════════════════════════════════════════════════════

impl UnifiedRemoteApp {
    fn render_session(&mut self, ctx: &egui::Context) {
        let mut do_disconnect = false;
        let mut toggle_fullscreen = false;
        let mut sync_clipboard = false;

        // Video canvas
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(Color32::BLACK))
            .show(ctx, |ui| {
                if let Some(tex) = &self.texture {
                    let avail = ui.max_rect();
                    let rect = match self.scaling {
                        Scaling::Stretch => avail,
                        Scaling::Fit => {
                            let ar = self.frame_size.0.max(1) as f32 / self.frame_size.1.max(1) as f32;
                            let (w, h) = if avail.width() / avail.height() > ar {
                                (avail.height() * ar, avail.height())
                            } else {
                                (avail.width(), avail.width() / ar)
                            };
                            egui::Rect::from_center_size(avail.center(), Vec2::new(w, h))
                        }
                    };

                    let r = ui.allocate_rect(rect, egui::Sense::click_and_drag());
                    ui.painter().image(
                        tex.id(),
                        rect,
                        egui::Rect::from_min_max(egui::pos2(0.0f32, 0.0f32), egui::pos2(1.0f32, 1.0f32)),
                        Color32::WHITE,
                    );

                    if let Some(conn) = &self.conn {
                        if let Some(p) = r.hover_pos() {
                            let xr = ((p.x - rect.min.x) / rect.width()).clamp(0.0f32, 1.0f32);
                            let yr = ((p.y - rect.min.y) / rect.height()).clamp(0.0f32, 1.0f32);
                            let _ = conn.tx_input.try_send(InputEvent::MouseMove { x_ratio: xr, y_ratio: yr });
                        }

                        for (btn, kind) in [
                            (egui::PointerButton::Primary, MouseBtn::Left),
                            (egui::PointerButton::Secondary, MouseBtn::Right),
                        ] {
                            if r.drag_started_by(btn) || r.clicked_by(btn) {
                                let _ = conn.tx_input.try_send(InputEvent::MouseDown { button: kind });
                            }
                            if r.drag_stopped_by(btn) || r.clicked_by(btn) {
                                let _ = conn.tx_input.try_send(InputEvent::MouseUp { button: kind });
                            }
                        }
                        if r.clicked_by(egui::PointerButton::Middle) {
                            let _ = conn.tx_input.try_send(InputEvent::MouseDown { button: MouseBtn::Middle });
                            let _ = conn.tx_input.try_send(InputEvent::MouseUp { button: MouseBtn::Middle });
                        }

                        if r.hovered() {
                            let sd = ctx.input(|i| i.raw_scroll_delta);
                            if sd.y.abs() > 0.1f32 || sd.x.abs() > 0.1f32 {
                                let _ = conn.tx_input.try_send(InputEvent::MouseScroll {
                                    delta_x: sd.x as i32,
                                    delta_y: sd.y as i32,
                                });
                            }
                        }

                        ctx.input(|i| {
                            for ev in &i.events {
                                match ev {
                                    egui::Event::Key { key, pressed, .. } => {
                                        // F11 is reserved for local full-screen toggle.
                                        if *key == egui::Key::F11 {
                                            if *pressed {
                                                toggle_fullscreen = true;
                                            }
                                            continue;
                                        }
                                        let kc = map_key(*key);
                                        let ev = if *pressed {
                                            InputEvent::KeyDown { key: kc }
                                        } else {
                                            InputEvent::KeyUp { key: kc }
                                        };
                                        let _ = conn.tx_input.try_send(ev);
                                    }
                                    egui::Event::Text(t) => {
                                        for c in t.chars() {
                                            let _ = conn.tx_input.try_send(InputEvent::KeyDown { key: KeyCode::Unicode(c) });
                                            let _ = conn.tx_input.try_send(InputEvent::KeyUp { key: KeyCode::Unicode(c) });
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        });
                    }
                } else {
                    ui.centered_and_justified(|ui| {
                        ui.vertical_centered(|ui| {
                            ui.add_space((ui.available_height() * 0.4f32).max(0.0f32));
                            ui.spinner();
                            ui.add_space(10.0f32);
                            ui.label(RichText::new("Waiting for video…").size(14.0f32).color(C.text_dim));
                        });
                    });
                }
            });

        // Floating toolbar: visible for a few seconds after connecting, when
        // pinned, or whenever the pointer is near the top edge.
        let near_top = ctx.pointer_hover_pos().map_or(false, |p| p.y < 70.0f32);
        let show = self.bar_pinned || near_top || self.session_started.elapsed().as_secs_f32() < 3.0f32;
        let t = ctx.animate_bool_with_time(egui::Id::new("session_bar_vis"), show, 0.18f32);
        if t > 0.01f32 {
            egui::Area::new(egui::Id::new("session_bar"))
                .order(egui::Order::Foreground)
                .anchor(egui::Align2::CENTER_TOP, Vec2::new(0.0f32, 12.0f32 - (1.0f32 - t) * 24.0f32))
                .show(ctx, |ui| {
                    ui.set_opacity(t);
                    egui::Frame::none()
                        .fill(Color32::from_rgba_unmultiplied(16, 16, 20, 235))
                        .stroke(Stroke::new(1.0f32, C.border_hover))
                        .rounding(Rounding::same(14.0f32))
                        .shadow(egui::epaint::Shadow {
                            offset: Vec2::new(0.0f32, 6.0f32),
                            blur: 18.0f32,
                            spread: 0.0f32,
                            color: Color32::from_black_alpha(140),
                        })
                        .inner_margin(Margin::symmetric(12.0f32, 7.0f32))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 8.0f32;
                                if let Some(conn) = &self.conn {
                                    let rtt = conn.rtt_ms.load(Ordering::Relaxed);
                                    let (lc, label) = if rtt < 30 {
                                        (C.green, "Excellent")
                                    } else if rtt < 80 {
                                        (C.amber, "Good")
                                    } else {
                                        (C.red, "Slow")
                                    };
                                    ui.colored_label(lc, RichText::new("●").size(10.0f32));
                                    ui.label(RichText::new(label).size(12.0f32).color(C.text));
                                    ui.label(
                                        RichText::new(format!(
                                            "{} ms  ·  {} fps  ·  {}×{}",
                                            rtt, self.fps, conn.screen_width, conn.screen_height
                                        ))
                                        .size(11.5f32)
                                        .monospace()
                                        .color(C.text_dim),
                                    );
                                }

                                ui.add_space(6.0f32);
                                ui.separator();
                                ui.add_space(2.0f32);

                                if let Some(a) = self.conn.as_ref().and_then(|c| c.audio.clone()) {
                                    let muted = a.muted();
                                    let lbl = if muted {
                                        "Unmute"
                                    } else if a.receiving() {
                                        "Mute"
                                    } else {
                                        "No audio"
                                    };
                                    if theme::button_sized(ui, lbl, BtnKind::Ghost, 0.0f32, 28.0f32, 12.0f32).clicked() {
                                        a.set_muted(!muted);
                                    }
                                    let mut vol = a.volume();
                                    ui.spacing_mut().slider_width = 70.0f32;
                                    if ui
                                        .add(egui::Slider::new(&mut vol, 0.0f32..=1.0f32).show_value(false))
                                        .changed()
                                    {
                                        a.set_volume(vol);
                                        if vol > 0.0f32 {
                                            a.set_muted(false);
                                        }
                                    }
                                }

                                if let Some(c) = &self.conn {
                                    let link = c.link.lock().map(|l| l.clone()).unwrap_or_default();
                                    ui.label(RichText::new(link).size(11.5f32).color(C.text_faint));
                                }
                                let mut qm = self.client_mode;
                                egui::ComboBox::from_id_source("quality_mode")
                                    .width(96.0f32)
                                    .selected_text(qm.label())
                                    .show_ui(ui, |ui| {
                                        for m in QualityMode::ALL {
                                            ui.selectable_value(&mut qm, m, m.label());
                                        }
                                    });
                                if qm != self.client_mode {
                                    self.client_mode = qm;
                                    if let Some(c) = &self.conn {
                                        let _ = c.tx_quality.try_send(qm);
                                    }
                                    self.save_settings();
                                }

                                let sc = match self.scaling {
                                    Scaling::Fit => "Fit",
                                    Scaling::Stretch => "Fill",
                                };
                                if theme::button_sized(ui, sc, BtnKind::Ghost, 0.0f32, 28.0f32, 12.0f32).clicked() {
                                    self.scaling = match self.scaling {
                                        Scaling::Fit => Scaling::Stretch,
                                        Scaling::Stretch => Scaling::Fit,
                                    };
                                }
                                if theme::button_sized(ui, "Paste text", BtnKind::Ghost, 0.0f32, 28.0f32, 12.0f32).clicked() {
                                    sync_clipboard = true;
                                }
                                let fs = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
                                if theme::button_sized(ui, if fs { "Exit full screen" } else { "Full screen" }, BtnKind::Ghost, 0.0f32, 28.0f32, 12.0f32).clicked() {
                                    toggle_fullscreen = true;
                                }
                                let pin = if self.bar_pinned { "Unpin" } else { "Pin bar" };
                                if theme::button_sized(ui, pin, BtnKind::Ghost, 0.0f32, 28.0f32, 12.0f32).clicked() {
                                    self.bar_pinned = !self.bar_pinned;
                                }
                                if theme::button_sized(ui, "Disconnect", BtnKind::Danger, 0.0f32, 28.0f32, 12.0f32).clicked() {
                                    do_disconnect = true;
                                }
                            });
                        });
                });
        }

        if sync_clipboard {
            if let Some(conn) = &self.conn {
                if let Ok(mut cb) = arboard::Clipboard::new() {
                    if let Ok(text) = cb.get_text() {
                        let _ = conn.tx_clipboard.try_send(text);
                        self.toast("Clipboard sent to host");
                    }
                }
            }
        }
        if toggle_fullscreen {
            let fs = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(!fs));
        }
        if do_disconnect {
            self.disconnect(ctx);
        }
    }

    fn render_toast(&mut self, ctx: &egui::Context) {
        if let Some(t) = &self.toast {
            let age = t.t.elapsed().as_secs_f32();
            if age < 2.2f32 {
                let msg = t.msg.clone();
                // Fade in/out
                let a = (age / 0.15f32).min(1.0f32).min((2.2f32 - age) / 0.3f32).clamp(0.0f32, 1.0f32);
                egui::Area::new(egui::Id::new("toast"))
                    .order(egui::Order::Tooltip)
                    .anchor(egui::Align2::CENTER_BOTTOM, Vec2::new(0.0f32, -24.0f32))
                    .show(ctx, |ui| {
                        ui.set_opacity(a);
                        egui::Frame::none()
                            .fill(C.bg_elevated)
                            .rounding(Rounding::same(10.0f32))
                            .stroke(Stroke::new(1.0f32, C.border_hover))
                            .inner_margin(Margin::symmetric(16.0f32, 9.0f32))
                            .show(ui, |ui| {
                                ui.label(RichText::new(msg).size(12.5f32).color(C.text));
                            });
                    });
                ctx.request_repaint();
            } else {
                self.toast = None;
            }
        }
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Helpers
// ═════════════════════════════════════════════════════════════════════════════

/// Cryptographically random password from an unambiguous alphabet.
fn random_password(len: usize) -> String {
    const ALPHABET: &[u8] = b"abcdefghjkmnpqrstuvwxyzABCDEFGHJKMNPQRSTUVWXYZ23456789";
    let mut buf = vec![0u8; len];
    if getrandom::getrandom(&mut buf).is_err() {
        // Extremely unlikely; fall back to time-derived bytes rather than panic.
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        for (i, b) in buf.iter_mut().enumerate() {
            *b = (n >> ((i % 16) * 8)) as u8 ^ (i as u8).wrapping_mul(31);
        }
    }
    // Rejection sampling avoids modulo bias.
    let limit = 256 - (256 % ALPHABET.len());
    buf.iter()
        .map(|&b| {
            let v = if (b as usize) < limit { b as usize } else { b as usize - limit };
            ALPHABET[v % ALPHABET.len()] as char
        })
        .collect()
}

fn fmt_duration(d: Duration) -> String {
    let s = d.as_secs();
    format!("{:02}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
}

/// Accepts `host`, `host:port` and returns `host:port`.
fn normalize_addr(raw: &str) -> Result<String, String> {
    let a = raw.trim();
    if a.is_empty() {
        return Err("Enter the join code or Pairing ID shown on the host, or an address like 100.64.0.1:5885.".to_string());
    }
    if transport::is_pairing_id(a) {
        return Ok(a.to_ascii_lowercase());
    }
    if transport::is_join_code(a) {
        // Validate now so a mistyped code gives an instant, clear error.
        return transport::resolve_target(a).map(|_| a.to_string());
    }
    match a.rsplit_once(':') {
        Some((host, port)) if !host.is_empty() => match port.parse::<u16>() {
            Ok(p) if p > 0 => Ok(format!("{}:{}", host, p)),
            _ => Err("The port must be a number between 1 and 65535.".to_string()),
        },
        Some(_) => Err("The address is missing a host.".to_string()),
        None => Ok(format!("{}:{}", a, DEFAULT_PORT)),
    }
}

/// Turns low-level errors into something a person can act on.
fn friendly_error(e: &str) -> String {
    let l = e.to_lowercase();
    if l.contains("refused") {
        "Connection refused. Check that sharing is on at the host and the port is correct.".to_string()
    } else if l.contains("timed out") || l.contains("timeout") {
        "Timed out. Check the Pairing ID or address, and that sharing is on at the host.".to_string()
    } else if l.contains("auth") || l.contains("password") {
        "Authentication failed. Check the password.".to_string()
    } else {
        e.to_string()
    }
}

fn recents_path() -> Option<std::path::PathBuf> {
    let base = std::env::var_os("APPDATA")?;
    Some(std::path::PathBuf::from(base).join("rust-remote-pc").join("recent.txt"))
}

fn load_recents() -> Vec<String> {
    recents_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|s| {
            s.lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .take(MAX_RECENT)
                .map(String::from)
                .collect()
        })
        .unwrap_or_default()
}

/// Stores addresses only; passwords are never written to disk.
fn save_recents(list: &[String]) {
    if let Some(p) = recents_path() {
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(p, list.join("\n"));
    }
}

fn map_key(key: egui::Key) -> KeyCode {
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
