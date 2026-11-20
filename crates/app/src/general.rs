// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux

use crate::widgets::name_edit;
use eframe::egui::{self, Ui};
use rfcps_core::Codeplug;

pub fn show(ui: &mut Ui, cp: &mut Codeplug) {
    let g = &mut cp.general;
    egui::Grid::new("general")
        .num_columns(2)
        .spacing([16.0, 8.0])
        .show(ui, |ui| {
            ui.label("Radio name (callsign)");
            name_edit(ui, &mut g.radio_name, 8, 120.0);
            ui.end_row();
            ui.label("DMR ID");
            ui.add(egui::DragValue::new(&mut g.dmr_id).range(0..=16_777_215));
            ui.end_row();
            ui.label("Group call hang time");
            ui.add(egui::DragValue::new(&mut g.group_hang).range(0..=255));
            ui.end_row();
            ui.label("Private call hang time");
            ui.add(egui::DragValue::new(&mut g.private_hang).range(0..=255));
            ui.end_row();
            ui.label("Boot text, line 1");
            name_edit(ui, &mut g.boot_line1, 16, 200.0);
            ui.end_row();
            ui.label("Boot text, line 2");
            name_edit(ui, &mut g.boot_line2, 16, 200.0);
            ui.end_row();
        });
    ui.add_space(8.0);
    ui.weak("Hang times are in the radio's own steps. Settings the editor does not show stay as they are.");
}
