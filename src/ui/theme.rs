#![allow(dead_code)]
use eframe::egui::{self, Color32, Margin, Response, Rounding, Stroke, Vec2, WidgetText};

// Minimal neutral dark palette — inspired by Linear / Vercel / Raycast
pub struct Colors {
    pub bg_base: Color32,
    pub bg_surface: Color32,
    pub bg_elevated: Color32,
    pub bg_input: Color32,
    pub border: Color32,
    pub border_hover: Color32,

    pub accent: Color32,
    pub accent_hover: Color32,
    pub green: Color32,
    pub red: Color32,
    pub amber: Color32,
    pub cyan: Color32,

    pub text: Color32,
    pub text_dim: Color32,
    pub text_faint: Color32,
}

pub const C: Colors = Colors {
    bg_base: Color32::from_rgb(10, 10, 10),
    bg_surface: Color32::from_rgb(18, 18, 18),
    bg_elevated: Color32::from_rgb(26, 26, 26),
    bg_input: Color32::from_rgb(14, 14, 14),
    border: Color32::from_rgb(40, 40, 40),
    border_hover: Color32::from_rgb(65, 65, 65),

    accent: Color32::from_rgb(99, 102, 241),
    accent_hover: Color32::from_rgb(129, 140, 248),
    green: Color32::from_rgb(52, 211, 153),
    red: Color32::from_rgb(248, 113, 113),
    amber: Color32::from_rgb(251, 191, 36),
    cyan: Color32::from_rgb(34, 211, 238),

    text: Color32::from_rgb(237, 237, 237),
    text_dim: Color32::from_rgb(140, 140, 140),
    text_faint: Color32::from_rgb(85, 85, 85),
};

pub fn apply_theme(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();

    v.override_text_color = Some(C.text);
    v.panel_fill = C.bg_base;
    v.window_fill = C.bg_base;
    v.extreme_bg_color = C.bg_input;
    v.faint_bg_color = C.bg_surface;

    v.widgets.noninteractive.bg_fill = C.bg_surface;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0f32, C.border);
    v.widgets.noninteractive.rounding = Rounding::same(6.0f32);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0f32, C.text_dim);

    v.widgets.inactive.bg_fill = C.bg_elevated;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0f32, C.border);
    v.widgets.inactive.rounding = Rounding::same(6.0f32);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0f32, C.text);

    v.widgets.hovered.bg_fill = C.bg_elevated;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0f32, C.border_hover);
    v.widgets.hovered.rounding = Rounding::same(6.0f32);
    v.widgets.hovered.fg_stroke = Stroke::new(1.0f32, Color32::WHITE);

    v.widgets.active.bg_fill = C.accent;
    v.widgets.active.bg_stroke = Stroke::NONE;
    v.widgets.active.rounding = Rounding::same(6.0f32);
    v.widgets.active.fg_stroke = Stroke::new(1.0f32, Color32::WHITE);

    v.widgets.open.bg_fill = C.bg_elevated;
    v.widgets.open.rounding = Rounding::same(6.0f32);

    v.selection.bg_fill = C.accent;
    v.selection.stroke = Stroke::new(1.0f32, C.accent_hover);

    v.window_rounding = Rounding::same(8.0f32);
    v.menu_rounding = Rounding::same(6.0f32);
    v.window_shadow = egui::epaint::Shadow::NONE;

    ctx.set_visuals(v);

    let mut s = (*ctx.style()).clone();
    s.spacing.item_spacing = Vec2::new(8.0f32, 6.0f32);
    s.spacing.button_padding = Vec2::new(12.0f32, 6.0f32);
    s.spacing.window_margin = Margin::same(12.0f32);
    ctx.set_style(s);
}

// ---- Minimal helpers ----

pub fn card() -> egui::Frame {
    egui::Frame::none()
        .fill(C.bg_surface)
        .rounding(Rounding::same(8.0f32))
        .stroke(Stroke::new(1.0f32, C.border))
        .inner_margin(Margin::same(16.0f32))
}

pub fn pill(color: Color32) -> egui::Frame {
    egui::Frame::none()
        .fill(color.linear_multiply(0.15f32))
        .rounding(Rounding::same(4.0f32))
        .inner_margin(Margin::symmetric(8.0f32, 3.0f32))
}

pub fn btn_accent(ui: &mut egui::Ui, label: impl Into<WidgetText>, w: f32) -> Response {
    ui.add(
        egui::Button::new(label)
            .fill(C.accent)
            .rounding(Rounding::same(6.0f32))
            .min_size(Vec2::new(w, 32.0f32)),
    )
}

pub fn btn_danger(ui: &mut egui::Ui, label: impl Into<WidgetText>, w: f32) -> Response {
    ui.add(
        egui::Button::new(label)
            .fill(C.red.linear_multiply(0.7f32))
            .rounding(Rounding::same(6.0f32))
            .min_size(Vec2::new(w, 32.0f32)),
    )
}

pub fn btn_green(ui: &mut egui::Ui, label: impl Into<WidgetText>, w: f32) -> Response {
    ui.add(
        egui::Button::new(label)
            .fill(C.green.linear_multiply(0.5f32))
            .rounding(Rounding::same(6.0f32))
            .min_size(Vec2::new(w, 32.0f32)),
    )
}

pub fn btn_ghost(ui: &mut egui::Ui, label: impl Into<WidgetText>) -> Response {
    ui.add(
        egui::Button::new(label)
            .fill(Color32::TRANSPARENT)
            .stroke(Stroke::new(1.0f32, C.border))
            .rounding(Rounding::same(6.0f32)),
    )
}
