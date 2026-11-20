// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux

use crate::widgets::{checked_text, freq_mhz, name_edit};
use eframe::egui::{self, Ui};
use egui_extras::{Column, TableBuilder};
use rfcps_core::fields::parse_tone;
use rfcps_core::records::{Channel, Mode, MAX_CHANNELS};
use rfcps_core::Codeplug;

const POWERS: [(u8, &str); 6] = [
    (0, "Default"),
    (1, "Min"),
    (3, "Low"),
    (6, "Mid"),
    (9, "High"),
    (10, "Max"),
];

fn power_name(p: u8) -> String {
    POWERS
        .iter()
        .find(|(v, _)| *v == p)
        .map_or_else(|| p.to_string(), |(_, n)| (*n).to_string())
}

fn new_channel(cp: &Codeplug) -> Option<Channel> {
    let number = (1..=MAX_CHANNELS as u16).find(|n| !cp.channels.iter().any(|c| c.number == *n))?;
    Some(Channel {
        number,
        name: format!("Channel {number}"),
        rx_hz: 146_520_000,
        tx_hz: 146_520_000,
        mode: Mode::Fm,
        power: 0,
        tot: 0,
        rx_tone: String::new(),
        tx_tone: String::new(),
        bw: 25.0,
        rx_only: false,
        all_skip: false,
        zone_skip: false,
        vox: false,
        squelch: 5,
        color_code: 1,
        timeslot: 1,
        contact: 0,
        rx_list: 0,
        aprs: 0,
        dmr_id: 0,
        no_beep: false,
        no_eco: false,
        dmo: false,
        roam: false,
        lat: 0.0,
        lon: 0.0,
        use_location: false,
        alias_ts1: 0,
        alias_ts2: 0,
        sfr: false,
        sfr_a: 0,
        sfr_b: 0,
        tone_burst: false,
    })
}

pub fn show(ui: &mut Ui, cp: &mut Codeplug) {
    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                cp.channels.len() < MAX_CHANNELS,
                egui::Button::new("Add channel"),
            )
            .clicked()
        {
            if let Some(c) = new_channel(cp) {
                cp.channels.push(c);
            }
        }
        ui.label(format!(
            "{} of {} channels",
            cp.channels.len(),
            MAX_CHANNELS
        ));
    });
    ui.add_space(4.0);

    let mut delete: Option<usize> = None;
    let contacts: Vec<(u16, String)> = cp
        .contacts
        .iter()
        .map(|c| (c.number, c.name.clone()))
        .collect();
    let height = ui.available_height();
    egui::ScrollArea::horizontal()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            TableBuilder::new(ui)
                .striped(true)
                .resizable(true)
                .max_scroll_height(height)
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                .column(Column::exact(38.0))
                .column(Column::exact(190.0))
                .column(Column::exact(104.0))
                .column(Column::exact(104.0))
                .column(Column::exact(68.0))
                .column(Column::exact(88.0))
                .column(Column::exact(86.0))
                .column(Column::exact(86.0))
                .column(Column::exact(100.0))
                .column(Column::exact(58.0))
                .column(Column::exact(58.0))
                .column(Column::exact(136.0))
                .column(Column::exact(74.0))
                .column(Column::exact(66.0))
                .column(Column::exact(70.0))
                .header(22.0, |mut h| {
                    for t in [
                        "#", "Name", "RX MHz", "TX MHz", "Mode", "Power", "RX tone", "TX tone",
                        "Width", "CC", "TS", "Contact", "Squelch", "RX only", "",
                    ] {
                        h.col(|ui| {
                            ui.strong(t);
                        });
                    }
                })
                .body(|body| {
                    body.rows(24.0, cp.channels.len(), |mut row| {
                        let i = row.index();
                        let number = cp.channels[i].number;
                        let c = &mut cp.channels[i];
                        row.col(|ui| {
                            ui.label(number.to_string());
                        });
                        row.col(|ui| {
                            name_edit(ui, &mut c.name, 16, 176.0);
                        });
                        row.col(|ui| {
                            freq_mhz(ui, &mut c.rx_hz);
                        });
                        row.col(|ui| {
                            freq_mhz(ui, &mut c.tx_hz);
                        });
                        row.col(|ui| {
                            egui::ComboBox::from_id_salt(("mode", number))
                                .selected_text(if c.mode == Mode::Dmr { "DMR" } else { "FM" })
                                .width(48.0)
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(&mut c.mode, Mode::Fm, "FM");
                                    ui.selectable_value(&mut c.mode, Mode::Dmr, "DMR");
                                });
                        });
                        row.col(|ui| {
                            egui::ComboBox::from_id_salt(("power", number))
                                .selected_text(power_name(c.power))
                                .width(60.0)
                                .show_ui(ui, |ui| {
                                    for (v, n) in POWERS {
                                        ui.selectable_value(&mut c.power, v, n);
                                    }
                                });
                        });
                        row.col(|ui| {
                            let id = ui.scope_id().with(("rx_tone", number));
                            let (_, v) = checked_text(ui, id, c.rx_tone.clone(), 72.0, parse_tone);
                            if let Some(v) = v {
                                c.rx_tone = v;
                            }
                        });
                        row.col(|ui| {
                            let id = ui.scope_id().with(("tx_tone", number));
                            let (_, v) = checked_text(ui, id, c.tx_tone.clone(), 72.0, parse_tone);
                            if let Some(v) = v {
                                c.tx_tone = v;
                            }
                        });
                        row.col(|ui| {
                            egui::ComboBox::from_id_salt(("bw", number))
                                .selected_text(if c.bw == 25.0 { "25 kHz" } else { "12.5 kHz" })
                                .width(64.0)
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(&mut c.bw, 12.5, "12.5 kHz");
                                    ui.selectable_value(&mut c.bw, 25.0, "25 kHz");
                                });
                        });
                        row.col(|ui| {
                            ui.add_enabled(
                                c.mode == Mode::Dmr,
                                egui::DragValue::new(&mut c.color_code).range(0..=15),
                            );
                        });
                        row.col(|ui| {
                            ui.add_enabled_ui(c.mode == Mode::Dmr, |ui| {
                                egui::ComboBox::from_id_salt(("ts", number))
                                    .selected_text(c.timeslot.to_string())
                                    .width(30.0)
                                    .show_ui(ui, |ui| {
                                        ui.selectable_value(&mut c.timeslot, 1, "1");
                                        ui.selectable_value(&mut c.timeslot, 2, "2");
                                    });
                            });
                        });
                        row.col(|ui| {
                            ui.add_enabled_ui(c.mode == Mode::Dmr, |ui| {
                                let shown = if c.contact == 0 {
                                    "none".to_string()
                                } else {
                                    contacts
                                        .iter()
                                        .find(|(n, _)| *n == c.contact)
                                        .map_or(format!("#{}", c.contact), |(_, n)| n.clone())
                                };
                                egui::ComboBox::from_id_salt(("contact", number))
                                    .selected_text(shown)
                                    .width(90.0)
                                    .show_ui(ui, |ui| {
                                        ui.selectable_value(&mut c.contact, 0, "none");
                                        for (n, name) in &contacts {
                                            ui.selectable_value(&mut c.contact, *n, name);
                                        }
                                    });
                            });
                        });
                        row.col(|ui| {
                            ui.add(egui::DragValue::new(&mut c.squelch).range(0..=21));
                        });
                        row.col(|ui| {
                            ui.checkbox(&mut c.rx_only, "");
                        });
                        row.col(|ui| {
                            if ui.small_button("Delete").clicked() {
                                delete = Some(i);
                            }
                        });
                    });
                });
        });
    if let Some(i) = delete {
        let number = cp.channels[i].number;
        cp.channels.remove(i);
        for z in &mut cp.zones {
            z.channels.retain(|&n| n != number);
        }
    }
}
