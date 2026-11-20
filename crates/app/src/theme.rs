// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux

//! The look: a dark theme with one accent color, rounded controls and room to breathe.

use eframe::egui::{self, Color32, CornerRadius, FontId, Stroke, TextStyle};

pub const ACCENT: Color32 = Color32::from_rgb(0, 170, 160);
pub const ACCENT_DARK: Color32 = Color32::from_rgb(0, 120, 114);
pub const WARN: Color32 = Color32::from_rgb(214, 150, 40);

pub fn apply(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    v.panel_fill = Color32::from_rgb(27, 29, 33);
    v.window_fill = Color32::from_rgb(34, 37, 42);
    v.extreme_bg_color = Color32::from_rgb(20, 22, 25);
    v.faint_bg_color = Color32::from_rgb(33, 36, 41);
    v.selection.bg_fill = ACCENT_DARK;
    v.selection.stroke = Stroke::new(1.0, Color32::WHITE);
    v.hyperlink_color = ACCENT;
    let r = CornerRadius::same(6);
    v.widgets.noninteractive.corner_radius = r;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(52, 56, 63));
    v.widgets.inactive.corner_radius = r;
    v.widgets.inactive.weak_bg_fill = Color32::from_rgb(46, 50, 57);
    v.widgets.inactive.bg_fill = Color32::from_rgb(46, 50, 57);
    v.widgets.hovered.corner_radius = r;
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(58, 64, 73);
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT);
    v.widgets.active.corner_radius = r;
    v.widgets.active.weak_bg_fill = ACCENT_DARK;
    v.widgets.open.corner_radius = r;
    v.window_corner_radius = CornerRadius::same(10);
    ctx.set_visuals(v);

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
            .insert(TextStyle::Heading, FontId::proportional(26.0));
    });
}
