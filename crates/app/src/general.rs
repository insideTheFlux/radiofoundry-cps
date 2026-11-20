// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux

//! The General tab: the radio's name and DMR ID, the call hang times and the boot text.

use crate::theme;
use crate::widgets::{checked_text, name_edit};
use eframe::egui::{self, RichText, Ui};
use rfcps_core::Codeplug;

fn group(ui: &mut Ui, title: &str) {
    ui.add_space(8.0);
    ui.label(
        RichText::new(title)
            .strong()
            .size(16.0)
            .color(theme::palette(ui).accent),
    );
    ui.separator();
}

pub fn show(ui: &mut Ui, cp: &mut Codeplug) {
    let muted = theme::palette(ui).muted;
    let g = &mut cp.general;
    ui.set_max_width(560.0);
    group(ui, "Radio");
    egui::Grid::new("general_radio")
        .num_columns(2)
        .spacing([16.0, 9.0])
        .show(ui, |ui| {
            ui.label("Radio name (callsign)");
            name_edit(ui, &mut g.radio_name, 8, 140.0);
            ui.end_row();
            ui.label("DMR ID");
            let id = egui::Id::unique("general_dmr_id");
            let shown = if g.dmr_id == 0 {
                String::new()
            } else {
                g.dmr_id.to_string()
            };
            let (_, v) = checked_text(ui, id, shown, 140.0, |s| {
                let t = s.trim();
                if t.is_empty() {
                    Some(0u32)
                } else {
                    t.parse::<u32>().ok().filter(|v| *v <= 16_777_215)
                }
            });
            if let Some(v) = v {
                g.dmr_id = v;
            }
            ui.end_row();
        });
    ui.label(RichText::new("The radio's name is sent as the talker alias. A DMR channel needs a DMR ID to transmit.").size(13.0).color(muted));

    group(ui, "Call hang time");
    egui::Grid::new("general_hang")
        .num_columns(2)
        .spacing([16.0, 9.0])
        .show(ui, |ui| {
            for (label, value, salt) in [
                ("Group call", &mut g.group_hang, "gh"),
                ("Private call", &mut g.private_hang, "ph"),
            ] {
                ui.label(label);
                let id = egui::Id::unique(("general_hang", salt));
                let (_, v) = checked_text(ui, id, value.to_string(), 80.0, |s| {
                    s.trim().parse::<u8>().ok()
                });
                if let Some(v) = v {
                    *value = v;
                }
                ui.end_row();
            }
        });
    ui.label(
        RichText::new("In the radio's own steps (0 to 255).")
            .size(13.0)
            .color(muted),
    );

    group(ui, "Boot text");
    egui::Grid::new("general_boot")
        .num_columns(2)
        .spacing([16.0, 9.0])
        .show(ui, |ui| {
            ui.label("Line 1");
            name_edit(ui, &mut g.boot_line1, 16, 240.0);
            ui.end_row();
            ui.label("Line 2");
            name_edit(ui, &mut g.boot_line2, 16, 240.0);
            ui.end_row();
        });
    ui.add_space(10.0);
    ui.label(
        RichText::new("Settings the editor does not show stay as they are.")
            .size(13.0)
            .color(muted),
    );
}
