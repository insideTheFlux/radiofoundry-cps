// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux

//! Small editing widgets shared by the tables.

use eframe::egui::{self, Color32, Response, Stroke, Ui};

/// A frequency in MHz as typed text, so a drag of the mouse can't change it. Returns the new frequency (Hz, on a
/// 10 Hz grid) when the typed text is a frequency in 30 to 1000 MHz; other text is outlined in red and not applied.
pub fn freq_edit(ui: &mut Ui, id: egui::Id, hz: u64, enabled: bool) -> (Response, Option<u64>) {
    let shown = format!("{:.5}", hz as f64 / 1e6);
    ui.add_enabled_ui(enabled, |ui| checked_text(ui, id, shown, 86.0, parse_mhz))
        .inner
}

/// A channel's TX frequency field. A TX frequency typed in a receive-only band (air band, broadcast FM) is refused
/// while the RX is outside one, as in the web editor: the radio never transmits there, so the channel keeps its TX
/// frequency. TX on an RX in such a band (TX follows RX there) and a TX left as it is are taken.
pub fn tx_freq_edit(ui: &mut Ui, id: egui::Id, hz: u64, rx_hz: u64, enabled: bool) -> (Response, Option<u64>) {
    use rfcps_core::freqs::listen_band;
    let shown = format!("{:.5}", hz as f64 / 1e6);
    ui.add_enabled_ui(enabled, |ui| {
        checked_text(ui, id, shown, 86.0, |s| {
            parse_mhz(s).filter(|h| !(listen_band(*h).is_some() && listen_band(rx_hz).is_none() && *h != hz))
        })
    })
    .inner
}

/// "146.52" or "146,52" MHz as Hz on a 10 Hz grid, or `None` if it is no frequency in 30 to 1000 MHz.
pub fn parse_mhz(text: &str) -> Option<u64> {
    let mhz: f64 = text.trim().replace(',', ".").parse().ok()?;
    ((30.0..=1000.0).contains(&mhz)).then(|| (mhz * 1e5).round() as u64 * 10)
}

/// Runs a file chooser on its own thread and sends back the paths picked (none: cancelled). The window stays alive
/// meanwhile: it keeps drawing and answers the desktop, which would otherwise offer "Force Quit" after a few seconds of
/// a chooser that blocks it. The window is repainted when the answer arrives.
pub fn spawn_dialog(
    ctx: egui::Context,
    run: impl FnOnce(rfd::FileDialog) -> Vec<std::path::PathBuf> + Send + 'static,
) -> std::sync::mpsc::Receiver<Vec<std::path::PathBuf>> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let paths = run(rfd::FileDialog::new());
        let _ = tx.send(paths);
        ctx.request_repaint();
    });
    rx
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

/// A name field of at most `max` bytes of the field: a character the radio draws is one byte, a Chinese one three
/// (`fields::name_bytes`). Text that is too long is cut at a whole character.
pub fn name_edit(ui: &mut Ui, name: &mut String, max: usize, width: f32) -> Response {
    let r = ui.add(egui::TextEdit::singleline(name).desired_width(width));
    if r.changed() {
        while rfcps_core::fields::name_bytes(name) > max {
            name.pop();
        }
    }
    r
}

#[cfg(test)]
mod tests {
    use super::parse_mhz;

    #[test]
    fn frequencies_parse_to_a_10_hz_grid() {
        assert_eq!(parse_mhz("146.52"), Some(146_520_000));
        assert_eq!(parse_mhz(" 446,76000 "), Some(446_760_000));
        assert_eq!(parse_mhz("446.7625"), Some(446_762_500));
        assert_eq!(parse_mhz("12"), None);
        assert_eq!(parse_mhz("1500"), None);
        assert_eq!(parse_mhz("abc"), None);
    }
}
