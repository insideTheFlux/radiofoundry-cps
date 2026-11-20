// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux

//! Small editing widgets shared by the tables.

use eframe::egui::{self, Color32, Response, Stroke, Ui};

/// A frequency in MHz, edited in place; the value is kept in Hz on a 10 Hz grid.
pub fn freq_mhz(ui: &mut Ui, hz: &mut u64) -> Response {
    let mut mhz = *hz as f64 / 1e6;
    let r = ui.add(
        egui::DragValue::new(&mut mhz)
            .speed(0.00625)
            .fixed_decimals(5)
            .range(30.0..=1000.0),
    );
    if r.changed() {
        *hz = (mhz * 1e5).round() as u64 * 10;
    }
    r
}

/// A text field that applies its text only when `accept` gives a value for it; while it is being typed the text
/// stays as typed, and a rejected text is outlined in red.
pub fn checked_text<T: Clone>(
    ui: &mut Ui,
    id: egui::Id,
    shown: String,
    width: f32,
    accept: impl Fn(&str) -> Option<T>,
) -> (Response, Option<T>) {
    let focused = ui.memory(|m| m.has_focus(id));
    let mut text: String = if focused {
        ui.data_mut(|d| d.get_temp(id)).unwrap_or(shown)
    } else {
        shown
    };
    let bad = accept(&text).is_none();
    if bad {
        ui.visuals_mut().widgets.inactive.bg_stroke =
            Stroke::new(1.0, Color32::from_rgb(200, 60, 60));
    }
    let r = ui.add(
        egui::TextEdit::singleline(&mut text)
            .id(id)
            .desired_width(width),
    );
    let mut out = None;
    if r.changed() {
        ui.data_mut(|d| d.insert_temp(id, text.clone()));
        out = accept(&text);
    }
    (r, out)
}

/// A name field of at most `max` characters.
pub fn name_edit(ui: &mut Ui, name: &mut String, max: usize, width: f32) -> Response {
    ui.add(
        egui::TextEdit::singleline(name)
            .char_limit(max)
            .desired_width(width),
    )
}
