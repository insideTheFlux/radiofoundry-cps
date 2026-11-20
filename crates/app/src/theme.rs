// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux

//! The look of RadioFoundry's web editor: its colors (an orange accent on warm grays, light and dark), rounded
//! controls and room to breathe. Light or dark follows the desktop unless chosen.

use eframe::egui::{self, Color32, CornerRadius, FontId, Stroke, TextStyle, Theme};

#[derive(Clone, Copy)]
#[allow(dead_code)] // `ok` and `bad` are for the Problems list
pub struct Palette {
    pub bg: Color32,
    pub surface: Color32,
    pub surface2: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub border: Color32,
    pub accent: Color32,
    pub accent_text: Color32,
    pub accent_soft: Color32,
    pub ok: Color32,
    pub warn: Color32,
    pub bad: Color32,
}

const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

pub const DARK: Palette = Palette {
    bg: rgb(0x121417),
    surface: rgb(0x1b1e22),
    surface2: rgb(0x23272c),
    text: rgb(0xeceef0),
    muted: rgb(0xa6adb6),
    border: rgb(0x343a41),
    accent: rgb(0xef7a50),
    accent_text: rgb(0x15171a),
    accent_soft: rgb(0x3a2219),
    ok: rgb(0x6fd08d),
    warn: rgb(0xf0c060),
    bad: rgb(0xff8a7a),
};

pub const LIGHT: Palette = Palette {
    bg: rgb(0xf7f7f5),
    surface: rgb(0xffffff),
    surface2: rgb(0xefefeb),
    text: rgb(0x15171a),
    muted: rgb(0x555a62),
    border: rgb(0xd9dad5),
    accent: rgb(0xb8431f),
    accent_text: rgb(0xffffff),
    accent_soft: rgb(0xfbe9e2),
    ok: rgb(0x1f7a3a),
    warn: rgb(0x8a5a00),
    bad: rgb(0xa8261b),
};

/// The palette in use.
pub fn palette(ui: &egui::Ui) -> Palette {
    if ui.visuals().dark_mode {
        DARK
    } else {
        LIGHT
    }
}

fn visuals(p: &Palette, dark: bool) -> egui::Visuals {
    let mut v = if dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    v.override_text_color = Some(p.text);
    v.panel_fill = p.bg;
    v.window_fill = p.surface;
    v.extreme_bg_color = p.surface;
    v.faint_bg_color = p.surface2;
    v.code_bg_color = p.surface2;
    v.selection.bg_fill = p.accent_soft;
    v.selection.stroke = Stroke::new(1.0, p.accent);
    v.hyperlink_color = p.accent;
    v.window_stroke = Stroke::new(1.0, p.border);
    let r = CornerRadius::same(7);
    let w = &mut v.widgets;
    w.noninteractive.corner_radius = r;
    w.noninteractive.bg_stroke = Stroke::new(1.0, p.border);
    w.noninteractive.fg_stroke = Stroke::new(1.0, p.text);
    w.inactive.corner_radius = r;
    w.inactive.weak_bg_fill = p.surface2;
    w.inactive.bg_fill = p.surface2;
    w.inactive.bg_stroke = Stroke::new(1.0, p.border);
    w.inactive.fg_stroke = Stroke::new(1.0, p.text);
    w.hovered.corner_radius = r;
    w.hovered.weak_bg_fill = p.surface2;
    w.hovered.bg_stroke = Stroke::new(1.0, p.accent);
    w.hovered.fg_stroke = Stroke::new(1.0, p.text);
    w.active.corner_radius = r;
    w.active.weak_bg_fill = p.accent_soft;
    w.active.bg_stroke = Stroke::new(1.0, p.accent);
    w.active.fg_stroke = Stroke::new(1.0, p.text);
    w.open.corner_radius = r;
    v.window_corner_radius = CornerRadius::same(10);
    v
}

pub fn apply(ctx: &egui::Context) {
    ctx.set_visuals_of(Theme::Dark, visuals(&DARK, true));
    ctx.set_visuals_of(Theme::Light, visuals(&LIGHT, false));
    ctx.all_styles_mut(|s| {
        s.spacing.item_spacing = egui::vec2(8.0, 7.0);
        s.spacing.button_padding = egui::vec2(12.0, 6.0);
        s.spacing.interact_size.y = 28.0;
        s.spacing.window_margin = egui::Margin::same(12);
        s.text_styles
            .insert(TextStyle::Body, FontId::proportional(15.0));
        s.text_styles
            .insert(TextStyle::Button, FontId::proportional(15.0));
        s.text_styles
            .insert(TextStyle::Small, FontId::proportional(12.5));
        s.text_styles
            .insert(TextStyle::Heading, FontId::proportional(24.0));
    });
}

/// The main action's button: filled with the accent color.
pub fn primary(ui: &egui::Ui, text: impl Into<String>, size: f32) -> egui::Button<'static> {
    let p = palette(ui);
    egui::Button::new(
        egui::RichText::new(text.into())
            .strong()
            .size(size)
            .color(p.accent_text),
    )
    .fill(p.accent)
}
