// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux

//! The APRS tab: the beacon configurations a channel can use (open-firmware-cps.md 7.3.8). A list on the left and the
//! selected configuration's form on the right. A configuration transmits only when a channel uses it.

use crate::theme;
use crate::widgets::{checked_text, name_edit, parse_mhz};
use eframe::egui::{self, RichText, Ui};
use rfcps_core::records::{Aprs, APRS_COMMENT_MAX, APRS_NAME_MAX, MAX_APRS};
use rfcps_core::Codeplug;

/// A path: up to six letters or digits, and an SSID of 0-15 after a dash; empty is none.
fn path_ok(s: &str) -> Option<String> {
    let t = s.trim().to_uppercase();
    if t.is_empty() {
        return Some(t);
    }
    let (call, ssid) = t.split_once('-').map_or((t.as_str(), None), |(c, d)| (c, Some(d)));
    let ok = (1..=6).contains(&call.chars().count())
        && call.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
        && ssid.is_none_or(|d| !d.is_empty() && d.chars().all(|c| c.is_ascii_digit()) && d.parse::<u8>().is_ok_and(|v| v <= 15));
    ok.then_some(t)
}

/// One printable character.
fn char_ok(s: &str) -> Option<char> {
    let mut it = s.chars();
    match (it.next(), it.next()) {
        (Some(c), None) if (' '..='~').contains(&c) => Some(c),
        _ => None,
    }
}

fn group(ui: &mut Ui, title: &str) {
    ui.add_space(8.0);
    ui.label(RichText::new(title).strong().size(16.0).color(theme::palette(ui).accent));
    ui.separator();
}

pub fn show(ui: &mut Ui, cp: &mut Codeplug, selected: &mut Option<u8>) {
    if selected.is_some_and(|n| !cp.aprs.iter().any(|a| a.number == n)) {
        *selected = None;
    }
    if selected.is_none() {
        *selected = cp.aprs.first().map(|a| a.number);
    }
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(230.0);
            if ui.add_enabled(cp.aprs.len() < MAX_APRS, egui::Button::new("Add APRS configuration")).clicked() {
                if let Some(n) = (1..=MAX_APRS as u8).find(|n| !cp.aprs.iter().any(|a| a.number == *n)) {
                    cp.aprs.push(Aprs::new(n, &format!("APRS {n}")));
                    *selected = Some(n);
                }
            }
            ui.label(format!("{} of {MAX_APRS} configurations", cp.aprs.len()));
            ui.separator();
            for a in &cp.aprs {
                ui.selectable_value(selected, Some(a.number), format!("{}  {}", a.number, a.name));
            }
        });
        ui.separator();
        ui.vertical(|ui| {
            let Some(number) = *selected else {
                ui.label("No APRS configurations. A configuration sets how a channel sends its position.");
                return;
            };
            egui::ScrollArea::vertical().id_salt("aprs_form").auto_shrink([false, false]).show(ui, |ui| {
                form(ui, cp, number, selected);
            });
        });
    });
}

fn form(ui: &mut Ui, cp: &mut Codeplug, number: u8, selected: &mut Option<u8>) {
    let muted = theme::palette(ui).muted;
    let used = cp.channels.iter().filter(|c| c.aprs == number).count();
    let Some(a) = cp.aprs.iter_mut().find(|a| a.number == number) else { return };
    ui.label(RichText::new(format!("APRS configuration {number}")).heading());
    if used == 0 {
        ui.label(RichText::new("No channel uses it.").color(muted));
    } else {
        ui.label(RichText::new(format!("{used} channel(s) use it.")).color(muted));
    }
    if number == 1 {
        // The firmware's APRS engine reads record 1 whether or not a channel names it (docs/spec/aprs.md 11).
        ui.label(
            RichText::new("The radio takes this configuration's SSID, path, symbol and comment for its own beacons, and keeps its own SSID and path in step with it: a different one written here replaces the radio's. Whether the radio beacons at all is set on its APRS page.")
                .size(13.0)
                .color(theme::palette(ui).warn),
        );
    }

    group(ui, "Station");
    egui::Grid::new(("aprs_station", number)).num_columns(2).spacing([14.0, 9.0]).show(ui, |ui| {
        let l = ui.label("Name");
        name_edit(ui, &mut a.name, APRS_NAME_MAX, 140.0).labelled_by(l.id);
        ui.end_row();
        let l = ui.label("SSID");
        let id = egui::Id::unique(("aprs_ssid", number));
        let (r, v) = checked_text(ui, id, a.ssid.to_string(), 60.0, |t| t.trim().parse::<u8>().ok().filter(|v| *v <= 15));
        r.labelled_by(l.id);
        if let Some(v) = v {
            a.ssid = v;
        }
        ui.end_row();
        for (label, salt, path) in [("Path 1", "aprs_p1", &mut a.path1), ("Path 2", "aprs_p2", &mut a.path2)] {
            let l = ui.label(label);
            let id = egui::Id::unique((salt, number));
            let (r, v) = checked_text(ui, id, path.clone(), 110.0, path_ok);
            r.labelled_by(l.id);
            if let Some(v) = v {
                *path = v;
            }
            ui.end_row();
        }
        let l = ui.label("Comment");
        let id = egui::Id::unique(("aprs_comment", number));
        let (r, v) = checked_text(ui, id, a.comment.clone(), 260.0, |t| {
            (t.chars().count() <= APRS_COMMENT_MAX).then(|| t.to_string())
        });
        r.labelled_by(l.id);
        if let Some(v) = v {
            a.comment = v;
        }
        ui.end_row();
    });
    ui.label(RichText::new("The call sign is the radio's name. A path is a call sign of up to six characters and an SSID, such as WIDE1-1.").size(13.0).color(muted));

    group(ui, "Symbol");
    egui::Grid::new(("aprs_symbol", number)).num_columns(2).spacing([14.0, 9.0]).show(ui, |ui| {
        for (label, salt, is_table) in [("Symbol table", "aprs_st", true), ("Symbol", "aprs_sc", false)] {
            let l = ui.label(label);
            let cur = if is_table { a.sym_table } else { a.sym_code };
            let id = egui::Id::unique((salt, number));
            let (r, v) = checked_text(ui, id, cur.to_string(), 50.0, char_ok);
            r.labelled_by(l.id);
            if let Some(v) = v {
                if is_table {
                    a.sym_table = v;
                } else {
                    a.sym_code = v;
                }
            }
            ui.end_row();
        }
    });
    ui.label(RichText::new("The table is / or a backslash; the symbol is one character, such as > for a car.").size(13.0).color(muted));

    group(ui, "Transmit");
    egui::Grid::new(("aprs_tx", number)).num_columns(2).spacing([14.0, 9.0]).show(ui, |ui| {
        let l = ui.label("TX frequency MHz");
        let id = egui::Id::unique(("aprs_tx_hz", number));
        let shown = if a.tx_hz == 0 { String::new() } else { format!("{:.5}", a.tx_hz as f64 / 1e6) };
        let (r, v) = checked_text(ui, id, shown, 110.0, |t| {
            let t = t.trim();
            if t.is_empty() { Some(0u64) } else { parse_mhz(t) }
        });
        r.labelled_by(l.id);
        if let Some(v) = v {
            a.tx_hz = v;
        }
        ui.end_row();        ui.label("");
        ui.label(RichText::new("Empty: the radio uses its own APRS frequency setting.").size(13.0).color(muted));
        ui.end_row();
        for (label, value) in [
            ("300 baud (else 1200)", &mut a.baud300),
            ("Transmit QSY information", &mut a.qsy),
            ("Beacon silently", &mut a.silent),
        ] {
            let l = ui.label(label);
            ui.checkbox(value, "").labelled_by(l.id);
            ui.end_row();
        }
    });

    group(ui, "Position");
    egui::Grid::new(("aprs_pos", number)).num_columns(2).spacing([14.0, 9.0]).show(ui, |ui| {
        let l = ui.label("Use the fixed position");
        ui.checkbox(&mut a.use_fixed, "").labelled_by(l.id);
        ui.end_row();
        for (label, salt, value, lim) in [("Latitude", "aprs_lat", &mut a.lat, 90.0), ("Longitude", "aprs_lon", &mut a.lon, 180.0)] {
            let l = ui.label(label);
            let id = egui::Id::unique((salt, number));
            let shown = if *value == 0.0 { String::new() } else { format!("{value:.4}") };
            let (r, v) = checked_text(ui, id, shown, 110.0, |t| {
                let t = t.trim().replace(',', ".");
                if t.is_empty() { Some(0.0) } else { t.parse::<f64>().ok().filter(|x| x.abs() <= lim) }
            });
            r.labelled_by(l.id);
            if let Some(v) = v {
                *value = v;
            }
            ui.end_row();
        }
        let l = ui.label("Position ambiguity");
        egui::ComboBox::from_id_salt(("aprs_amb", number))
            .selected_text(a.ambiguity.to_string())
            .width(60.0)
            .show_ui(ui, |ui| {
                for n in 0..=4u8 {
                    ui.selectable_value(&mut a.ambiguity, n, n.to_string());
                }
            })
            .response
            .labelled_by(l.id);
        ui.end_row();
    });
    ui.label(RichText::new("Degrees, south and west negative. Ambiguity 0 is exact; each step blurs the position by a digit.").size(13.0).color(muted));

    group(ui, "Delete");
    let confirm = egui::Id::unique(("aprs_delete", number));
    let asking = ui.data(|d| d.get_temp::<bool>(confirm)).unwrap_or(false);
    if asking {
        ui.label(if used > 0 {
            format!("Delete this configuration? {used} channel(s) stop using it.")
        } else {
            "Delete this configuration?".to_string()
        });
        ui.horizontal(|ui| {
            if ui.button("Yes, delete it").clicked() {
                ui.data_mut(|d| d.remove::<bool>(confirm));
                cp.remove_aprs(number);
                *selected = None;
            } else if ui.button("Cancel").clicked() {
                ui.data_mut(|d| d.remove::<bool>(confirm));
            }
        });
    } else if ui.button("Delete configuration…").clicked() {
        ui.data_mut(|d| d.insert_temp(confirm, true));
    }
}
