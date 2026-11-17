pub mod theme;

use crate::client::connection::{connect_to_host, ConnectionHandle, DecodedFrameMessage};
use crate::common::net_info::{get_local_ips, InterfaceIp};
use crate::common::protocol::{InputEvent, KeyCode, MouseBtn};
use crate::host::server::{HostServer, HostServerConfig};
use eframe::egui::{self, Color32, RichText, Rounding, Stroke, Vec2};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Instant;
use theme::C;
use tokio::sync::{broadcast, mpsc, oneshot};

// ═════════════════════════════════════════════════════════════════════════════
// Types
// ═════════════════════════════════════════════════════════════════════════════

#[derive(PartialEq, Clone, Copy)]
enum Tab { Host, Client }

#[derive(PartialEq, Clone, Copy)]
enum Scaling { Fit, Stretch }

struct Toast { msg: String, t: Instant }

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
    host_running: bool,
    host_shutdown: Option<broadcast::Sender<()>>,
    ips: Vec<InterfaceIp>,

    // Client
    client_addr: String,
    client_password: String,
    show_client_pw: bool,
    client_msg: String,
    client_err: bool,
    connecting: bool,
    pending: Option<Pending>,

    // Session
    conn: Option<ConnectionHandle>,
    frame_rx: Option<mpsc::Receiver<DecodedFrameMessage>>,
    texture: Option<egui::TextureHandle>,
    frame_size: (u32, u32),
    scaling: Scaling,

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
        let default_addr = if let Some(h) = initial_host {
            h
        } else if let Some(ts) = ips.iter().find(|i| i.is_tailscale) {
            format!("{}:5885", ts.ip)
        } else {
            "100.x.y.z:5885".to_string()
        };

        Self {
            tab: Tab::Host,
            host_port: "5885".to_string(),
            host_password: initial_password.clone().unwrap_or_else(|| "remote88".to_string()),
            show_host_pw: false,
            host_fps: 60,
            host_quality: 75,
            host_running: false,
            host_shutdown: None,
            ips,

            client_addr: default_addr,
            client_password: initial_password.unwrap_or_default(),
            show_client_pw: false,
            client_msg: String::new(),
            client_err: false,
            connecting: false,
            pending: None,

            conn: None,
            frame_rx: None,
            texture: None,
            frame_size: (0, 0),
            scaling: Scaling::Fit,

            fps_count: 0,
            fps: 0,
            fps_tick: Instant::now(),

            toast: None,
            rt: tokio_runtime,
        }
    }

    fn toast(&mut self, s: &str) { self.toast = Some(Toast { msg: s.to_string(), t: Instant::now() }); }

    fn copy(&mut self, ctx: &egui::Context, s: &str, msg: &str) {
        ctx.output_mut(|o| o.copied_text = s.to_string());
        if let Ok(mut cb) = arboard::Clipboard::new() { let _ = cb.set_text(s); }
        self.toast(msg);
    }

    fn primary_ip(&self) -> String {
        self.ips.iter().find(|i| i.is_tailscale)
            .or(self.ips.first())
            .map(|i| i.ip.clone())
            .unwrap_or_else(|| "127.0.0.1".to_string())
    }

    fn start_host(&mut self) {
        if self.host_running { return; }
        let port: u16 = self.host_port.parse().unwrap_or(5885);
        let cfg = HostServerConfig {
            bind_addr: format!("0.0.0.0:{}", port),
            password: self.host_password.clone(),
            target_fps: self.host_fps,
            jpeg_quality: self.host_quality,
        };
        let srv = HostServer::new(cfg);
        let (tx, rx) = broadcast::channel(1);
        self.host_shutdown = Some(tx);
        self.host_running = true;
        self.toast("Server started");
        self.rt.spawn(async move { let _ = srv.run_with_shutdown(rx).await; });
    }

    fn stop_host(&mut self) {
        if let Some(tx) = self.host_shutdown.take() { let _ = tx.send(()); }
        self.host_running = false;
        self.toast("Server stopped");
    }

    fn connect(&mut self) {
        if self.connecting || self.conn.is_some() { return; }
        let addr = self.client_addr.trim().to_string();
        if addr.is_empty() {
            self.client_msg = "Enter a valid address".to_string();
            self.client_err = true;
            return;
        }
        self.connecting = true;
        self.client_msg = format!("Connecting to {}...", addr);
        self.client_err = false;

        let pw = self.client_password.clone();
        let (tx_f, rx_f) = mpsc::channel::<DecodedFrameMessage>(4);
        self.frame_rx = Some(rx_f);

        let (tx_r, rx_r) = oneshot::channel();
        let a = addr.clone();
        self.rt.spawn(async move {
            let r = connect_to_host(&a, &pw, tx_f).await;
            let _ = tx_r.send(r);
        });
        self.pending = Some(Pending { rx: rx_r, addr });
    }

    fn disconnect(&mut self) {
        self.conn = None;
        self.frame_rx = None;
        self.texture = None;
        self.client_msg.clear();
        self.toast("Disconnected");
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Main update loop
// ═════════════════════════════════════════════════════════════════════════════

impl eframe::App for UnifiedRemoteApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint();

        // FPS
        self.fps_count += 1;
        if self.fps_tick.elapsed().as_secs() >= 1 {
            self.fps = self.fps_count;
            self.fps_count = 0;
            self.fps_tick = Instant::now();
        }

        // Poll pending connection (non-blocking)
        if let Some(mut p) = self.pending.take() {
            match p.rx.try_recv() {
                Ok(Ok(h)) => {
                    self.connecting = false;
                    self.client_msg.clear();
                    self.conn = Some(h);
                    self.toast("Connected");
                }
                Ok(Err(e)) => {
                    self.connecting = false;
                    self.client_msg = format!("{}", e);
                    self.client_err = true;
                    self.frame_rx = None;
                }
                Err(oneshot::error::TryRecvError::Empty) => { self.pending = Some(p); }
                Err(_) => {
                    self.connecting = false;
                    self.client_msg = "Connection lost".to_string();
                    self.client_err = true;
                    self.frame_rx = None;
                }
            }
        }

        // Consume frames
        if let Some(rx) = &mut self.frame_rx {
            while let Ok(m) = rx.try_recv() {
                let img = egui::ColorImage::from_rgba_unmultiplied(
                    [m.width as usize, m.height as usize], &m.rgba_data,
                );
                self.frame_size = (m.width, m.height);
                self.texture = Some(ctx.load_texture("screen", img, egui::TextureOptions::LINEAR));
            }
        }

        // ── Active session → full-screen remote view ──
        if self.conn.is_some() {
            self.render_session(ctx);
            self.render_toast(ctx);
            return;
        }

        // ── Normal UI ──
        // Top tab bar
        egui::TopBottomPanel::top("topbar")
            .frame(egui::Frame::none()
                .fill(C.bg_surface)
                .stroke(Stroke::new(1.0f32, C.border))
                .inner_margin(egui::Margin::symmetric(20.0f32, 10.0f32)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Remote")
                        .size(16.0f32).strong().color(C.text));
                    ui.label(RichText::new("Desktop")
                        .size(16.0f32).color(C.text_dim));

                    ui.add_space(24.0f32);

                    let host_label = if self.tab == Tab::Host {
                        RichText::new("Host").size(13.0f32).strong().color(C.text)
                    } else {
                        RichText::new("Host").size(13.0f32).color(C.text_dim)
                    };
                    if ui.add(egui::Button::new(host_label)
                        .fill(if self.tab == Tab::Host { C.bg_elevated } else { Color32::TRANSPARENT })
                        .stroke(if self.tab == Tab::Host { Stroke::new(1.0f32, C.border_hover) } else { Stroke::NONE })
                        .rounding(Rounding::same(6.0f32))).clicked()
                    {
                        self.tab = Tab::Host;
                    }

                    let client_label = if self.tab == Tab::Client {
                        RichText::new("Connect").size(13.0f32).strong().color(C.text)
                    } else {
                        RichText::new("Connect").size(13.0f32).color(C.text_dim)
                    };
                    if ui.add(egui::Button::new(client_label)
                        .fill(if self.tab == Tab::Client { C.bg_elevated } else { Color32::TRANSPARENT })
                        .stroke(if self.tab == Tab::Client { Stroke::new(1.0f32, C.border_hover) } else { Stroke::NONE })
                        .rounding(Rounding::same(6.0f32))).clicked()
                    {
                        self.tab = Tab::Client;
                    }

                    // Right side — network status dot
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let ip = self.primary_ip();
                        let is_ts = self.ips.iter().any(|i| i.is_tailscale);
                        let dot = if is_ts { C.cyan } else { C.green };
                        ui.colored_label(dot, RichText::new("●").size(10.0f32));
                        ui.label(RichText::new(&ip).size(12.0f32).monospace().color(C.text_dim));
                    });
                });
            });

        // Central content
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(C.bg_base).inner_margin(egui::Margin::same(0.0f32)))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    // Center content with max width
                    let avail = ui.available_width();
                    let max_w = 600.0f32;
                    let pad = ((avail - max_w) * 0.5f32).max(24.0f32);

                    ui.horizontal(|ui| {
                        ui.add_space(pad);
                        ui.vertical(|ui| {
                            ui.set_max_width(max_w);
                            ui.add_space(28.0f32);
                            match self.tab {
                                Tab::Host => self.render_host(ui, ctx),
                                Tab::Client => self.render_client(ui),
                            }
                            ui.add_space(40.0f32);
                        });
                    });
                });
            });

        self.render_toast(ctx);
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Host tab
// ═════════════════════════════════════════════════════════════════════════════

impl UnifiedRemoteApp {
    fn render_host(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let mut copy_trigger: Option<(String, &'static str)> = None;

        ui.label(RichText::new("Share this computer").size(22.0f32).strong().color(C.text));
        ui.add_space(2.0f32);
        ui.label(RichText::new("Start the host server so other devices can view and control this desktop.")
            .size(13.0f32).color(C.text_dim));
        ui.add_space(20.0f32);

        // ── Status + action ──
        theme::card().show(ui, |ui| {
            ui.horizontal(|ui| {
                let dot = if self.host_running { C.green } else { C.text_faint };
                ui.colored_label(dot, RichText::new("●").size(14.0f32));
                ui.vertical(|ui| {
                    ui.label(RichText::new(if self.host_running { "Server is running" } else { "Server is offline" })
                        .size(14.0f32).strong().color(if self.host_running { C.green } else { C.text }));
                    if self.host_running {
                        ui.label(RichText::new(format!("Port {} · {} FPS · {}% quality",
                            self.host_port, self.host_fps, self.host_quality))
                            .size(12.0f32).color(C.text_dim));
                    }
                });

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if !self.host_running {
                        if theme::btn_green(ui,
                            RichText::new("Start server").size(13.0f32).strong().color(Color32::WHITE), 120.0f32
                        ).clicked() {
                            self.start_host();
                        }
                    } else {
                        if theme::btn_danger(ui,
                            RichText::new("Stop").size(13.0f32).color(Color32::WHITE), 70.0f32
                        ).clicked() {
                            self.stop_host();
                        }
                    }
                });
            });
        });

        ui.add_space(16.0f32);

        // ── Connection info ──
        let ip = self.primary_ip();
        let full_addr = format!("{}:{}", ip, self.host_port);
        let is_ts = self.ips.iter().any(|i| i.is_tailscale);

        theme::card().show(ui, |ui| {
            ui.label(RichText::new("Connection address").size(12.0f32).color(C.text_dim));
            ui.add_space(6.0f32);

            ui.horizontal(|ui| {
                ui.label(RichText::new(&full_addr).size(18.0f32).monospace().strong().color(C.text));
                ui.add_space(8.0f32);
                if is_ts {
                    theme::pill(C.cyan).show(ui, |ui| {
                        ui.label(RichText::new("Tailscale").size(10.0f32).color(C.cyan));
                    });
                }
            });

            ui.add_space(8.0f32);
            ui.horizontal(|ui| {
                if theme::btn_accent(ui,
                    RichText::new("Copy address").size(12.0f32).color(Color32::WHITE), 110.0f32
                ).clicked() {
                    copy_trigger = Some((full_addr.clone(), "Address copied"));
                }
                if theme::btn_ghost(ui, RichText::new("Copy password").size(12.0f32).color(C.text_dim)).clicked() {
                    copy_trigger = Some((self.host_password.clone(), "Password copied"));
                }
            });
        });

        ui.add_space(16.0f32);

        // ── Settings ──
        theme::card().show(ui, |ui| {
            ui.label(RichText::new("Settings").size(14.0f32).strong().color(C.text));
            ui.add_space(10.0f32);

            egui::Grid::new("host_grid").spacing([12.0f32, 10.0f32]).show(ui, |ui| {
                ui.label(RichText::new("Password").size(12.0f32).color(C.text_dim));
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.host_password)
                        .password(!self.show_host_pw).desired_width(160.0f32));
                    if ui.button(if self.show_host_pw { "Hide" } else { "Show" }).clicked() {
                        self.show_host_pw = !self.show_host_pw;
                    }
                    if ui.button("Randomize").clicked() {
                        self.host_password = format!("p{}", rand_val());
                        self.toast("New password generated");
                    }
                });
                ui.end_row();

                ui.label(RichText::new("Port").size(12.0f32).color(C.text_dim));
                ui.add(egui::TextEdit::singleline(&mut self.host_port).desired_width(80.0f32));
                ui.end_row();

                ui.label(RichText::new("Frame rate").size(12.0f32).color(C.text_dim));
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.host_fps, 30, "30 fps");
                    ui.selectable_value(&mut self.host_fps, 60, "60 fps");
                });
                ui.end_row();

                ui.label(RichText::new("Quality").size(12.0f32).color(C.text_dim));
                ui.add(egui::Slider::new(&mut self.host_quality, 50..=95).suffix("%"));
                ui.end_row();
            });
        });

        ui.add_space(16.0f32);

        // ── Network adapters (collapsed) ──
        let ips = self.ips.clone();
        egui::CollapsingHeader::new(RichText::new("Network interfaces").size(12.0f32).color(C.text_dim))
            .default_open(false)
            .show(ui, |ui| {
                for item in &ips {
                    ui.horizontal(|ui| {
                        let c = if item.is_tailscale { C.cyan } else { C.text_faint };
                        ui.colored_label(c, RichText::new("●").size(8.0f32));
                        ui.label(RichText::new(&item.ip).size(12.0f32).monospace().color(C.text));
                        ui.label(RichText::new(&item.label).size(11.0f32).color(C.text_faint));
                        if ui.small_button("Copy").clicked() {
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
// Client tab
// ═════════════════════════════════════════════════════════════════════════════

impl UnifiedRemoteApp {
    fn render_client(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("Connect to remote PC").size(22.0f32).strong().color(C.text));
        ui.add_space(2.0f32);
        ui.label(RichText::new("Enter the address and password of the host you want to control.")
            .size(13.0f32).color(C.text_dim));
        ui.add_space(20.0f32);

        theme::card().show(ui, |ui| {
            egui::Grid::new("conn_grid").spacing([12.0f32, 12.0f32]).show(ui, |ui| {
                ui.label(RichText::new("Address").size(12.0f32).color(C.text_dim));
                ui.add(egui::TextEdit::singleline(&mut self.client_addr)
                    .hint_text("100.x.y.z:5885").desired_width(300.0f32));
                ui.end_row();

                ui.label(RichText::new("Password").size(12.0f32).color(C.text_dim));
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.client_password)
                        .password(!self.show_client_pw).hint_text("password").desired_width(240.0f32));
                    if ui.button(if self.show_client_pw { "Hide" } else { "Show" }).clicked() {
                        self.show_client_pw = !self.show_client_pw;
                    }
                });
                ui.end_row();
            });

            ui.add_space(14.0f32);

            // Status line
            if !self.client_msg.is_empty() {
                let c = if self.client_err { C.red } else { C.text_dim };
                ui.label(RichText::new(&self.client_msg).size(12.0f32).color(c));
                ui.add_space(8.0f32);
            }

            // Connect button
            if !self.connecting {
                if theme::btn_accent(ui,
                    RichText::new("Connect").size(14.0f32).strong().color(Color32::WHITE), 140.0f32
                ).clicked() {
                    self.connect();
                }
            } else {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(RichText::new("Connecting...").size(12.0f32).color(C.accent_hover));
                    ui.add_space(8.0f32);
                    if ui.button(RichText::new("Cancel").size(12.0f32).color(C.text_dim)).clicked() {
                        self.pending = None;
                        self.connecting = false;
                        self.frame_rx = None;
                        self.client_msg.clear();
                    }
                });
            }
        });
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Remote Session (full-screen streaming view)
// ═════════════════════════════════════════════════════════════════════════════

impl UnifiedRemoteApp {
    fn render_session(&mut self, ctx: &egui::Context) {
        // Minimal floating top bar — only essential telemetry
        egui::TopBottomPanel::top("session_bar")
            .frame(egui::Frame::none()
                .fill(Color32::from_rgba_premultiplied(10, 10, 10, 220))
                .stroke(Stroke::new(1.0f32, C.border))
                .inner_margin(egui::Margin::symmetric(16.0f32, 7.0f32)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    // Live dot
                    ui.colored_label(C.red, RichText::new("●").size(10.0f32));
                    ui.label(RichText::new("LIVE").size(11.0f32).strong().color(C.text_dim));

                    ui.add_space(16.0f32);

                    if let Some(conn) = &self.conn {
                        let rtt = conn.rtt_ms.load(Ordering::Relaxed);
                        let lc = if rtt < 30 { C.green } else if rtt < 80 { C.amber } else { C.red };

                        ui.label(RichText::new(format!("{} ms", rtt)).size(12.0f32).monospace().color(lc));
                        ui.label(RichText::new("·").size(12.0f32).color(C.text_faint));
                        ui.label(RichText::new(format!("{} fps", self.fps)).size(12.0f32).monospace().color(C.text_dim));
                        ui.label(RichText::new("·").size(12.0f32).color(C.text_faint));
                        ui.label(RichText::new(format!("{}×{}", conn.screen_width, conn.screen_height))
                            .size(12.0f32).monospace().color(C.text_dim));
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add(egui::Button::new(
                            RichText::new("Disconnect").size(11.0f32).color(C.red))
                            .fill(Color32::TRANSPARENT)
                            .stroke(Stroke::new(1.0f32, C.red.linear_multiply(0.4f32)))
                            .rounding(Rounding::same(4.0f32))
                        ).clicked() {
                            self.disconnect();
                        }

                        // Scaling toggle
                        let sc = match self.scaling { Scaling::Fit => "Fit", Scaling::Stretch => "Fill" };
                        if theme::btn_ghost(ui, RichText::new(sc).size(11.0f32).color(C.text_dim)).clicked() {
                            self.scaling = match self.scaling { Scaling::Fit => Scaling::Stretch, Scaling::Stretch => Scaling::Fit };
                        }

                        // Clipboard sync
                        if theme::btn_ghost(ui, RichText::new("Clipboard").size(11.0f32).color(C.text_dim)).clicked() {
                            if let Some(conn) = &self.conn {
                                if let Ok(mut cb) = arboard::Clipboard::new() {
                                    if let Ok(text) = cb.get_text() {
                                        let _ = conn.tx_clipboard.try_send(text);
                                        self.toast("Clipboard synced");
                                    }
                                }
                            }
                        }
                    });
                });
            });

        // Video canvas
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(Color32::BLACK))
            .show(ctx, |ui| {
                if let Some(tex) = &self.texture {
                    let avail = ui.available_size();

                    let (sz, off) = match self.scaling {
                        Scaling::Stretch => (avail, Vec2::ZERO),
                        Scaling::Fit => {
                            let iw = self.frame_size.0.max(1) as f32;
                            let ih = self.frame_size.1.max(1) as f32;
                            let ar = iw / ih;
                            let (w, h) = if avail.x / avail.y > ar {
                                (avail.y * ar, avail.y)
                            } else {
                                (avail.x, avail.x / ar)
                            };
                            (Vec2::new(w, h), Vec2::new((avail.x - w) * 0.5f32, (avail.y - h) * 0.5f32))
                        }
                    };

                    ui.vertical(|ui| {
                        if off.y > 0.0f32 { ui.add_space(off.y); }
                        ui.horizontal(|ui| {
                            if off.x > 0.0f32 { ui.add_space(off.x); }

                            let r = ui.add(egui::Image::new(tex)
                                .fit_to_exact_size(sz)
                                .sense(egui::Sense::click_and_drag()));
                            let rect = r.rect;

                            if let Some(conn) = &self.conn {
                                if let Some(p) = r.hover_pos() {
                                    let xr = ((p.x - rect.min.x) / rect.width()).clamp(0.0f32, 1.0f32);
                                    let yr = ((p.y - rect.min.y) / rect.height()).clamp(0.0f32, 1.0f32);
                                    let _ = conn.tx_input.try_send(InputEvent::MouseMove { x_ratio: xr, y_ratio: yr });
                                }

                                // Left mouse
                                if r.drag_started_by(egui::PointerButton::Primary) || r.clicked_by(egui::PointerButton::Primary) {
                                    let _ = conn.tx_input.try_send(InputEvent::MouseDown { button: MouseBtn::Left });
                                }
                                if r.drag_stopped_by(egui::PointerButton::Primary) || r.clicked_by(egui::PointerButton::Primary) {
                                    let _ = conn.tx_input.try_send(InputEvent::MouseUp { button: MouseBtn::Left });
                                }

                                // Right mouse
                                if r.drag_started_by(egui::PointerButton::Secondary) || r.clicked_by(egui::PointerButton::Secondary) {
                                    let _ = conn.tx_input.try_send(InputEvent::MouseDown { button: MouseBtn::Right });
                                }
                                if r.drag_stopped_by(egui::PointerButton::Secondary) || r.clicked_by(egui::PointerButton::Secondary) {
                                    let _ = conn.tx_input.try_send(InputEvent::MouseUp { button: MouseBtn::Right });
                                }

                                // Middle
                                if r.clicked_by(egui::PointerButton::Middle) {
                                    let _ = conn.tx_input.try_send(InputEvent::MouseDown { button: MouseBtn::Middle });
                                    let _ = conn.tx_input.try_send(InputEvent::MouseUp { button: MouseBtn::Middle });
                                }

                                // Scroll
                                let sd = ctx.input(|i| i.raw_scroll_delta);
                                if sd.y.abs() > 0.1f32 || sd.x.abs() > 0.1f32 {
                                    let _ = conn.tx_input.try_send(InputEvent::MouseScroll {
                                        delta_x: sd.x as i32, delta_y: sd.y as i32,
                                    });
                                }

                                // Keyboard
                                ctx.input(|i| {
                                    for ev in &i.events {
                                        match ev {
                                            egui::Event::Key { key, pressed, .. } => {
                                                let kc = map_key(*key);
                                                if *pressed {
                                                    let _ = conn.tx_input.try_send(InputEvent::KeyDown { key: kc });
                                                } else {
                                                    let _ = conn.tx_input.try_send(InputEvent::KeyUp { key: kc });
                                                }
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
                        });
                    });
                } else {
                    ui.centered_and_justified(|ui| {
                        ui.vertical_centered(|ui| {
                            ui.spinner();
                            ui.add_space(8.0f32);
                            ui.label(RichText::new("Waiting for video stream...")
                                .size(14.0f32).color(C.text_dim));
                        });
                    });
                }
            });
    }

    fn render_toast(&mut self, ctx: &egui::Context) {
        if let Some(t) = &self.toast {
            if t.t.elapsed().as_secs_f32() < 2.0f32 {
                let msg = t.msg.clone();
                egui::Area::new(egui::Id::new("toast"))
                    .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0f32, -24.0f32))
                    .show(ctx, |ui| {
                        egui::Frame::none()
                            .fill(C.bg_elevated)
                            .rounding(Rounding::same(6.0f32))
                            .stroke(Stroke::new(1.0f32, C.border_hover))
                            .inner_margin(egui::Margin::symmetric(16.0f32, 8.0f32))
                            .show(ui, |ui| {
                                ui.label(RichText::new(msg).size(12.0f32).color(C.text));
                            });
                    });
            } else {
                self.toast = None;
            }
        }
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Helpers
// ═════════════════════════════════════════════════════════════════════════════

fn rand_val() -> u32 {
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    ((n ^ (n >> 16)) % 900000 + 100000) as u32
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
