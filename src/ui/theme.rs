#![allow(dead_code)]
use eframe::egui::{
    self, Align2, Color32, CursorIcon, FontId, Margin, Rect, Response, Rounding, Sense, Stroke, Ui,
    Vec2,
};

// Neutral dark palette with a single indigo accent.
pub struct Colors {
    pub bg_base: Color32,
    pub bg_surface: Color32,
    pub bg_elevated: Color32,
    pub bg_hover: Color32,
    pub bg_input: Color32,
    pub border: Color32,
    pub border_hover: Color32,

    pub accent: Color32,
    pub accent_hover: Color32,
    pub accent_press: Color32,
    pub green: Color32,
    pub red: Color32,
    pub amber: Color32,
    pub cyan: Color32,

    pub text: Color32,
    pub text_dim: Color32,
    pub text_faint: Color32,
}

pub const C: Colors = Colors {
    bg_base: Color32::from_rgb(11, 11, 14),
    bg_surface: Color32::from_rgb(19, 19, 23),
    bg_elevated: Color32::from_rgb(28, 28, 34),
    bg_hover: Color32::from_rgb(34, 34, 41),
    bg_input: Color32::from_rgb(14, 14, 18),
    border: Color32::from_rgb(42, 42, 50),
    border_hover: Color32::from_rgb(70, 70, 82),

    accent: Color32::from_rgb(99, 102, 241),
    accent_hover: Color32::from_rgb(121, 124, 247),
    accent_press: Color32::from_rgb(79, 82, 221),
    green: Color32::from_rgb(52, 211, 153),
    red: Color32::from_rgb(248, 113, 113),
    amber: Color32::from_rgb(251, 191, 36),
    cyan: Color32::from_rgb(34, 211, 238),

    text: Color32::from_rgb(238, 238, 242),
    text_dim: Color32::from_rgb(150, 150, 162),
    text_faint: Color32::from_rgb(92, 92, 104),
};

pub fn apply_theme(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();

    v.override_text_color = Some(C.text);
    v.panel_fill = C.bg_base;
    v.window_fill = C.bg_surface;
    v.extreme_bg_color = C.bg_input;
    v.faint_bg_color = C.bg_surface;

    let r = Rounding::same(8.0f32);

    v.widgets.noninteractive.bg_fill = C.bg_surface;
    v.widgets.noninteractive.weak_bg_fill = C.bg_surface;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0f32, C.border);
    v.widgets.noninteractive.rounding = r;
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0f32, C.text_dim);

    v.widgets.inactive.bg_fill = C.bg_elevated;
    v.widgets.inactive.weak_bg_fill = C.bg_elevated;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0f32, C.border);
    v.widgets.inactive.rounding = r;
    v.widgets.inactive.fg_stroke = Stroke::new(1.0f32, C.text);

    v.widgets.hovered.bg_fill = C.bg_hover;
    v.widgets.hovered.weak_bg_fill = C.bg_hover;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0f32, C.border_hover);
    v.widgets.hovered.rounding = r;
    v.widgets.hovered.fg_stroke = Stroke::new(1.0f32, Color32::WHITE);

    v.widgets.active.bg_fill = C.accent;
    v.widgets.active.weak_bg_fill = C.accent;
    v.widgets.active.bg_stroke = Stroke::NONE;
    v.widgets.active.rounding = r;
    v.widgets.active.fg_stroke = Stroke::new(1.0f32, Color32::WHITE);

    v.widgets.open.bg_fill = C.bg_elevated;
    v.widgets.open.rounding = r;

    v.selection.bg_fill = C.accent.linear_multiply(0.55f32);
    v.selection.stroke = Stroke::new(1.0f32, C.accent_hover);

    v.window_rounding = Rounding::same(12.0f32);
    v.menu_rounding = Rounding::same(8.0f32);
    v.window_shadow = egui::epaint::Shadow {
        offset: Vec2::new(0.0f32, 8.0f32),
        blur: 24.0f32,
        spread: 0.0f32,
        color: Color32::from_black_alpha(120),
    };
    v.popup_shadow = v.window_shadow;

    ctx.set_visuals(v);

    let mut s = (*ctx.style()).clone();
    s.spacing.item_spacing = Vec2::new(8.0f32, 8.0f32);
    s.spacing.button_padding = Vec2::new(12.0f32, 6.0f32);
    s.spacing.window_margin = Margin::same(14.0f32);
    s.spacing.interact_size.y = 30.0f32;
    s.spacing.slider_width = 200.0f32;
    ctx.set_style(s);
}

// ── Containers ───────────────────────────────────────────────────────────────

pub fn card() -> egui::Frame {
    egui::Frame::none()
        .fill(C.bg_surface)
        .rounding(Rounding::same(12.0f32))
        .stroke(Stroke::new(1.0f32, C.border))
        .inner_margin(Margin::same(18.0f32))
}

pub fn pill(color: Color32) -> egui::Frame {
    egui::Frame::none()
        .fill(color.linear_multiply(0.15f32))
        .rounding(Rounding::same(20.0f32))
        .inner_margin(Margin::symmetric(9.0f32, 3.0f32))
}

/// Inline banner used for errors / info messages.
pub fn banner(color: Color32) -> egui::Frame {
    egui::Frame::none()
        .fill(color.linear_multiply(0.12f32))
        .stroke(Stroke::new(1.0f32, color.linear_multiply(0.35f32)))
        .rounding(Rounding::same(8.0f32))
        .inner_margin(Margin::symmetric(12.0f32, 8.0f32))
}

pub fn section_label(ui: &mut Ui, text: &str) {
    ui.label(
        egui::RichText::new(text.to_uppercase())
            .size(10.5f32)
            .strong()
            .color(C.text_faint),
    );
}

// ── Buttons ──────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq)]
pub enum BtnKind {
    Primary,
    Success,
    Danger,
    Ghost,
}

fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

pub fn button(ui: &mut Ui, text: &str, kind: BtnKind, min_w: f32) -> Response {
    button_sized(ui, text, kind, min_w, 34.0f32, 13.0f32)
}

pub fn button_sized(ui: &mut Ui, text: &str, kind: BtnKind, min_w: f32, h: f32, font: f32) -> Response {
    let enabled = ui.is_enabled();
    let (bg, hover, press, fg, stroke) = match kind {
        BtnKind::Primary => (C.accent, C.accent_hover, C.accent_press, Color32::WHITE, Stroke::NONE),
        BtnKind::Success => {
            let g = mix(C.green, Color32::BLACK, 0.35f32);
            (g, mix(g, Color32::WHITE, 0.12f32), mix(g, Color32::BLACK, 0.15f32), Color32::WHITE, Stroke::NONE)
        }
        BtnKind::Danger => {
            let r = mix(C.red, Color32::BLACK, 0.3f32);
            (r, mix(r, Color32::WHITE, 0.12f32), mix(r, Color32::BLACK, 0.15f32), Color32::WHITE, Stroke::NONE)
        }
        BtnKind::Ghost => (
            Color32::TRANSPARENT,
            C.bg_hover,
            C.bg_elevated,
            C.text,
            Stroke::new(1.0f32, C.border),
        ),
    };

    let fg = if enabled { fg } else { C.text_faint };
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_string(), FontId::proportional(font), fg);
    let size = Vec2::new((galley.size().x + 28.0f32).max(min_w), h);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());

    if ui.is_rect_visible(rect) {
        let fill = if !enabled {
            if kind == BtnKind::Ghost { Color32::TRANSPARENT } else { C.bg_elevated }
        } else if resp.is_pointer_button_down_on() {
            press
        } else if resp.hovered() {
            hover
        } else {
            bg
        };
        let stroke = if resp.hovered() && kind == BtnKind::Ghost && enabled {
            Stroke::new(1.0f32, C.border_hover)
        } else {
            stroke
        };
        ui.painter().rect(rect, Rounding::same(8.0f32), fill, stroke);
        ui.painter()
            .galley(rect.center() - galley.size() * 0.5f32, galley, fg);
    }
    if resp.hovered() && enabled {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    resp
}

/// Segmented control. Returns true if the value changed.
pub fn segmented<T: PartialEq + Copy>(ui: &mut Ui, value: &mut T, options: &[(T, &str)]) -> bool {
    let mut changed = false;
    egui::Frame::none()
        .fill(C.bg_input)
        .stroke(Stroke::new(1.0f32, C.border))
        .rounding(Rounding::same(8.0f32))
        .inner_margin(Margin::same(3.0f32))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 2.0f32;
            ui.horizontal(|ui| {
                for (v, label) in options {
                    let sel = *value == *v;
                    let color = if sel { C.text } else { C.text_dim };
                    let galley = ui.painter().layout_no_wrap(
                        label.to_string(),
                        FontId::proportional(12.5f32),
                        color,
                    );
                    let size = Vec2::new(galley.size().x + 24.0f32, 26.0f32);
                    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
                    if sel {
                        ui.painter().rect_filled(rect, Rounding::same(6.0f32), C.bg_hover);
                    } else if resp.hovered() {
                        ui.painter().rect_filled(rect, Rounding::same(6.0f32), C.bg_elevated);
                    }
                    ui.painter()
                        .galley(rect.center() - galley.size() * 0.5f32, galley, color);
                    if resp.hovered() {
                        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                    }
                    if resp.clicked() && !sel {
                        *value = *v;
                        changed = true;
                    }
                }
            });
        });
    changed
}

/// Single-line text field with comfortable padding.
pub fn text_field(
    ui: &mut Ui,
    value: &mut String,
    hint: &str,
    width: f32,
    password: bool,
    mono: bool,
) -> Response {
    let mut edit = egui::TextEdit::singleline(value)
        .hint_text(hint)
        .desired_width(width)
        .margin(Margin::symmetric(10.0f32, 8.0f32))
        .password(password);
    if mono {
        edit = edit.font(egui::TextStyle::Monospace);
    }
    ui.add(edit)
}

// ── Small drawn icons (default fonts lack most symbols) ──────────────────────

pub fn icon_monitor(ui: &Ui, rect: Rect, color: Color32) {
    let p = ui.painter();
    let body = Rect::from_center_size(rect.center() - Vec2::new(0.0f32, 1.5f32), Vec2::new(14.0f32, 10.0f32));
    p.rect_stroke(body, Rounding::same(2.0f32), Stroke::new(1.4f32, color));
    let y = body.bottom() + 2.5f32;
    p.line_segment(
        [egui::pos2(rect.center().x - 3.5f32, y), egui::pos2(rect.center().x + 3.5f32, y)],
        Stroke::new(1.4f32, color),
    );
}

pub fn icon_connect(ui: &Ui, rect: Rect, color: Color32) {
    let p = ui.painter();
    let c = rect.center();
    let s = Stroke::new(1.5f32, color);
    p.line_segment([egui::pos2(c.x - 6.0f32, c.y), egui::pos2(c.x + 6.0f32, c.y)], s);
    p.line_segment([egui::pos2(c.x + 2.0f32, c.y - 4.0f32), egui::pos2(c.x + 6.0f32, c.y)], s);
    p.line_segment([egui::pos2(c.x + 2.0f32, c.y + 4.0f32), egui::pos2(c.x + 6.0f32, c.y)], s);
}

/// Sidebar navigation row. Returns the response (click to select).
pub fn nav_item(
    ui: &mut Ui,
    label: &str,
    selected: bool,
    status: Option<Color32>,
    icon: fn(&Ui, Rect, Color32),
) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 38.0f32), Sense::click());
    if ui.is_rect_visible(rect) {
        let p = ui.painter();
        if selected {
            p.rect_filled(rect, Rounding::same(8.0f32), C.bg_elevated);
            let bar = Rect::from_center_size(
                egui::pos2(rect.left() + 1.5f32, rect.center().y),
                Vec2::new(3.0f32, 18.0f32),
            );
            p.rect_filled(bar, Rounding::same(2.0f32), C.accent);
        } else if resp.hovered() {
            p.rect_filled(rect, Rounding::same(8.0f32), C.bg_hover.linear_multiply(0.6f32));
        }
        let color = if selected { C.text } else { C.text_dim };
        let icon_rect = Rect::from_center_size(
            egui::pos2(rect.left() + 22.0f32, rect.center().y),
            Vec2::splat(18.0f32),
        );
        icon(ui, icon_rect, color);
        ui.painter().text(
            egui::pos2(rect.left() + 40.0f32, rect.center().y),
            Align2::LEFT_CENTER,
            label,
            FontId::proportional(13.5f32),
            color,
        );
        if let Some(c) = status {
            ui.painter()
                .circle_filled(egui::pos2(rect.right() - 14.0f32, rect.center().y), 3.5f32, c);
        }
    }
    if resp.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    resp
}
