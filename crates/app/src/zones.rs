// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux

use crate::widgets::name_edit;
use eframe::egui::{self, Ui};
use rfcps_core::records::{Zone, MAX_ZONES};
use rfcps_core::Codeplug;

pub fn show(ui: &mut Ui, cp: &mut Codeplug, selected: &mut usize) {
    if *selected >= cp.zones.len() {
        *selected = cp.zones.len().saturating_sub(1);
    }
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(200.0);
            if ui
                .add_enabled(cp.zones.len() < MAX_ZONES, egui::Button::new("Add zone"))
                .clicked()
            {
                cp.zones.push(Zone {
                    slot: cp.zones.len(),
                    name: format!("Zone {}", cp.zones.len() + 1),
                    channels: Vec::new(),
                });
                *selected = cp.zones.len() - 1;
            }
            ui.label(format!("{} of {} zones", cp.zones.len(), MAX_ZONES));
            ui.separator();
            egui::ScrollArea::vertical()
                .id_salt("zone_list")
                .show(ui, |ui| {
                    for (i, z) in cp.zones.iter().enumerate() {
                        ui.selectable_value(
                            selected,
                            i,
                            format!("{}  ({})", z.name, z.channels.len()),
                        );
                    }
                });
        });
        ui.separator();
        ui.vertical(|ui| {
            let Some(zone) = cp.zones.get(*selected).cloned() else {
                ui.label("No zones.");
                return;
            };
            let names: Vec<(u16, String)> = cp
                .channels
                .iter()
                .map(|c| (c.number, c.name.clone()))
                .collect();
            let i = *selected;
            ui.horizontal(|ui| {
                ui.label("Name");
                name_edit(ui, &mut cp.zones[i].name, 16, 160.0);
                if ui.button("Move up").clicked() && i > 0 {
                    cp.zones.swap(i, i - 1);
                    *selected = i - 1;
                }
                if ui.button("Move down").clicked() && i + 1 < cp.zones.len() {
                    cp.zones.swap(i, i + 1);
                    *selected = i + 1;
                }
                if ui.button("Delete zone").clicked() {
                    cp.zones.remove(i);
                }
            });
            ui.separator();
            if i < cp.zones.len() {
                let full = zone.channels.len() >= 80;
                let in_zone = &zone.channels;
                ui.horizontal(|ui| {
                    egui::ComboBox::from_id_salt("add_member")
                        .selected_text("Add a channel…")
                        .width(220.0)
                        .show_ui(ui, |ui| {
                            for (n, name) in names.iter().filter(|(n, _)| !in_zone.contains(n)) {
                                if ui
                                    .add_enabled(!full, egui::Button::new(format!("{n}  {name}")))
                                    .clicked()
                                {
                                    cp.zones[i].channels.push(*n);
                                }
                            }
                        });
                    ui.label(format!("{} of 80 channels", zone.channels.len()));
                });
                ui.add_space(4.0);
                let mut act: Option<(usize, i32)> = None;
                egui::ScrollArea::vertical()
                    .id_salt("zone_members")
                    .show(ui, |ui| {
                        for (k, n) in zone.channels.iter().enumerate() {
                            ui.horizontal(|ui| {
                                let name = names
                                    .iter()
                                    .find(|(m, _)| m == n)
                                    .map_or("(missing channel)", |(_, s)| s.as_str());
                                ui.label(format!("{:>2}.  {:>4}  {}", k + 1, n, name));
                                if ui.small_button("Up").clicked() {
                                    act = Some((k, -1));
                                }
                                if ui.small_button("Down").clicked() {
                                    act = Some((k, 1));
                                }
                                if ui.small_button("Remove").clicked() {
                                    act = Some((k, 0));
                                }
                            });
                        }
                    });
                if let Some((k, d)) = act {
                    let list = &mut cp.zones[i].channels;
                    match d {
                        0 => {
                            list.remove(k);
                        }
                        -1 if k > 0 => list.swap(k, k - 1),
                        1 if k + 1 < list.len() => list.swap(k, k + 1),
                        _ => {}
                    }
                }
            }
        });
    });
}
