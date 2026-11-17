#![allow(dead_code)]
use eframe::egui::{self, Color32, Margin, Response, Rounding, Stroke, Vec2, WidgetText};

pub struct ThemeColors {
    pub bg_deep: Color32,
    pub bg_panel: Color32,
    pub bg_card: Color32,
    pub bg_card_hover: Color32,
    pub bg_card_elevated: Color32,
    pub bg_input: Color32,
    pub border_subtle: Color32,
    pub border_focus: Color32,
    
    // Accents
    pub primary: Color32,
    pub primary_hover: Color32,
    pub primary_active: Color32,
    pub success: Color32,
    pub success_hover: Color32,
    pub success_soft: Color32,
    pub warning: Color32,
    pub warning_soft: Color32,
    pub danger: Color32,
    pub danger_hover: Color32,
    pub danger_soft: Color32,
    pub tailscale_blue: Color32,
    pub tailscale_soft: Color32,
    
    // Text
    pub text_primary: Color32,
    pub text_secondary: Color32,
    pub text_muted: Color32,
}

pub const THEME: ThemeColors = ThemeColors {
    bg_deep: Color32::from_rgb(11, 15, 25),          // #0B0F19 (Deep Slate)
    bg_panel: Color32::from_rgb(16, 22, 36),         // #101624 (Sidebar / Topbar)
    bg_card: Color32::from_rgb(23, 31, 51),          // #171F33 (Content Cards)
    bg_card_hover: Color32::from_rgb(30, 41, 67),    // #1E2943 (Card Hover)
    bg_card_elevated: Color32::from_rgb(28, 38, 62), // #1C263E (Elevated Card)
    bg_input: Color32::from_rgb(14, 19, 32),         // #0E1320 (Text edit bg)
    border_subtle: Color32::from_rgb(38, 50, 78),    // #26324E (Borders)
    border_focus: Color32::from_rgb(99, 102, 241),   // #6366F1 (Active border)
    
    primary: Color32::from_rgb(99, 102, 241),        // Indigo 500
    primary_hover: Color32::from_rgb(129, 140, 248), // Indigo 400
    primary_active: Color32::from_rgb(79, 70, 229),  // Indigo 600
    
    success: Color32::from_rgb(16, 185, 129),        // Emerald 500
    success_hover: Color32::from_rgb(52, 211, 153),  // Emerald 400
    success_soft: Color32::from_rgb(6, 78, 59),      // Emerald 900
    
    warning: Color32::from_rgb(245, 158, 11),        // Amber 500
    warning_soft: Color32::from_rgb(69, 26, 3),      // Amber 950
    
    danger: Color32::from_rgb(239, 68, 68),          // Red 500
    danger_hover: Color32::from_rgb(248, 113, 113),  // Red 400
    danger_soft: Color32::from_rgb(69, 10, 10),      // Red 950
    
    tailscale_blue: Color32::from_rgb(14, 165, 233), // Sky 500
    tailscale_soft: Color32::from_rgb(8, 47, 73),    // Sky 950
    
    text_primary: Color32::from_rgb(248, 250, 252),  // Slate 50
    text_secondary: Color32::from_rgb(148, 163, 184),// Slate 400
    text_muted: Color32::from_rgb(100, 116, 139),    // Slate 500
};

pub fn apply_modern_theme(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();

    visuals.override_text_color = Some(THEME.text_primary);
    visuals.panel_fill = THEME.bg_panel;
    visuals.window_fill = THEME.bg_deep;
    visuals.extreme_bg_color = THEME.bg_input;
    visuals.faint_bg_color = THEME.bg_card;

    // Widget states styling
    visuals.widgets.noninteractive.bg_fill = THEME.bg_card;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0f32, THEME.border_subtle);
    visuals.widgets.noninteractive.rounding = Rounding::same(8.0f32);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0f32, THEME.text_secondary);

    visuals.widgets.inactive.bg_fill = THEME.bg_card;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0f32, THEME.border_subtle);
    visuals.widgets.inactive.rounding = Rounding::same(8.0f32);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0f32, THEME.text_primary);

    visuals.widgets.hovered.bg_fill = THEME.bg_card_hover;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.2f32, THEME.primary_hover);
    visuals.widgets.hovered.rounding = Rounding::same(8.0f32);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0f32, Color32::WHITE);

    visuals.widgets.active.bg_fill = THEME.primary;
    visuals.widgets.active.bg_stroke = Stroke::new(1.0f32, THEME.primary_active);
    visuals.widgets.active.rounding = Rounding::same(8.0f32);
    visuals.widgets.active.fg_stroke = Stroke::new(1.0f32, Color32::WHITE);

    visuals.widgets.open.bg_fill = THEME.bg_card_hover;
    visuals.widgets.open.rounding = Rounding::same(8.0f32);

    visuals.selection.bg_fill = THEME.primary;
    visuals.selection.stroke = Stroke::new(1.0f32, THEME.primary_hover);

    visuals.window_rounding = Rounding::same(12.0f32);
    visuals.menu_rounding = Rounding::same(8.0f32);

    ctx.set_visuals(visuals);

    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = Vec2::new(10.0f32, 10.0f32);
    style.spacing.button_padding = Vec2::new(14.0f32, 8.0f32);
    style.spacing.window_margin = Margin::same(16.0f32);
    ctx.set_style(style);
}

// Reusable card container frame
pub fn modern_card() -> egui::Frame {
    egui::Frame::none()
        .fill(THEME.bg_card)
        .rounding(Rounding::same(12.0f32))
        .stroke(Stroke::new(1.0f32, THEME.border_subtle))
        .inner_margin(Margin::same(18.0f32))
}

// Elevated card container frame (for hero sections)
pub fn modern_card_elevated() -> egui::Frame {
    egui::Frame::none()
        .fill(THEME.bg_card_elevated)
        .rounding(Rounding::same(14.0f32))
        .stroke(Stroke::new(1.2f32, THEME.border_subtle))
        .inner_margin(Margin::same(20.0f32))
}

// Status banner frame
pub fn status_banner_frame(is_active: bool) -> egui::Frame {
    if is_active {
        egui::Frame::none()
            .fill(Color32::from_rgb(6, 40, 30))
            .rounding(Rounding::same(12.0f32))
            .stroke(Stroke::new(1.5f32, THEME.success))
            .inner_margin(Margin::same(16.0f32))
    } else {
        egui::Frame::none()
            .fill(THEME.bg_card)
            .rounding(Rounding::same(12.0f32))
            .stroke(Stroke::new(1.0f32, THEME.border_subtle))
            .inner_margin(Margin::same(16.0f32))
    }
}

// Custom styled button helper
pub fn primary_button(ui: &mut egui::Ui, text: impl Into<WidgetText>, min_size: Vec2) -> Response {
    let btn = egui::Button::new(text)
        .fill(THEME.primary)
        .rounding(Rounding::same(8.0f32))
        .min_size(min_size);
    ui.add(btn)
}

pub fn success_button(ui: &mut egui::Ui, text: impl Into<WidgetText>, min_size: Vec2) -> Response {
    let btn = egui::Button::new(text)
        .fill(THEME.success)
        .rounding(Rounding::same(8.0f32))
        .min_size(min_size);
    ui.add(btn)
}

pub fn danger_button(ui: &mut egui::Ui, text: impl Into<WidgetText>, min_size: Vec2) -> Response {
    let btn = egui::Button::new(text)
        .fill(THEME.danger)
        .rounding(Rounding::same(8.0f32))
        .min_size(min_size);
    ui.add(btn)
}

pub fn outline_button(ui: &mut egui::Ui, text: impl Into<WidgetText>, min_size: Vec2) -> Response {
    let btn = egui::Button::new(text)
        .fill(THEME.bg_card)
        .stroke(Stroke::new(1.0f32, THEME.border_subtle))
        .rounding(Rounding::same(8.0f32))
        .min_size(min_size);
    ui.add(btn)
}

// Badge container frame (e.g. for Tailscale / Status tags)
pub fn modern_badge(fill: Color32, border: Color32) -> egui::Frame {
    egui::Frame::none()
        .fill(fill)
        .rounding(Rounding::same(6.0f32))
        .stroke(Stroke::new(1.0f32, border))
        .inner_margin(Margin::symmetric(8.0f32, 4.0f32))
}
