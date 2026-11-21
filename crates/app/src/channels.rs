// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux

//! The Channels tab: a compact table, and a form for the selected channel with the fields that apply to its mode
//! (the layout of the web editor's channel form).

use crate::theme;
use crate::widgets::{checked_text, freq_edit, name_edit};
use eframe::egui::{self, RichText, Ui};
use egui_extras::{Column, TableBuilder};
use rfcps_core::fields::parse_tone;
use rfcps_core::freqs::{listen_band, set_rx, set_tx, ListenBand};
use rfcps_core::modes::{self, Params};
use rfcps_core::systems::System;
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
    Some(Channel::new(number, 146_520_000, 146_520_000))
}

struct Choices {
    contacts: Vec<(u16, String)>,
    lists: Vec<(u8, String)>,
}

pub fn show(ui: &mut Ui, cp: &mut Codeplug, selected: &mut Option<u16>) {
    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                cp.channels.len() < MAX_CHANNELS,
                egui::Button::new("Add channel"),
            )
            .clicked()
        {
            if let Some(c) = new_channel(cp) {
                *selected = Some(c.number);
                cp.channels.push(c);
            }
        }
        if ui
            .add_enabled(selected.is_some(), egui::Button::new("Delete channel"))
            .clicked()
        {
            if let Some(n) = selected.take() {
                cp.channels.retain(|c| c.number != n);
                for z in &mut cp.zones {
                    z.channels.retain(|&m| m != n);
                }
            }
        }
        ui.label(
            egui::RichText::new(format!(
                "{} of {} channels",
                cp.channels.len(),
                MAX_CHANNELS
            ))
            .color(theme::palette(ui).muted),
        );
    });
    ui.add_space(4.0);
    if let Some(n) = *selected {
        if !cp.channels.iter().any(|c| c.number == n) {
            *selected = None;
        }
    }
    let choices = Choices {
        contacts: cp
            .contacts
            .iter()
            .map(|c| (c.number, c.name.clone()))
            .collect(),
        lists: cp
            .rx_lists
            .iter()
            .map(|l| (l.number, l.name.clone()))
            .collect(),
    };
    // The table keeps its natural width in a panel on the left and the form fills the rest, so a wide window leaves
    // its spare room at the far right instead of a hole in the middle.
    const TABLE_W: f32 = 960.0;
    let table_w = TABLE_W.min((ui.available_width() - 400.0).max(300.0));
    egui::Panel::left("channel_table")
        .resizable(false)
        .exact_size(table_w)
        .show(ui, |ui| table(ui, cp, selected, &choices));
    egui::CentralPanel::default().show(ui, |ui| {
        egui::ScrollArea::vertical()
            .id_salt("channel_detail")
            .auto_shrink([false, false])
            .show(ui, |ui| detail(ui, cp, selected, &choices));
    });
}

fn table(ui: &mut Ui, cp: &mut Codeplug, selected: &mut Option<u16>, choices: &Choices) {
    let height = ui.available_height();
    egui::ScrollArea::horizontal()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            TableBuilder::new(ui)
                .striped(true)
                .resizable(false)
                .sense(egui::Sense::click())
                .max_scroll_height(height)
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                .column(Column::exact(40.0))
                .column(Column::exact(180.0))
                .column(Column::exact(100.0))
                .column(Column::exact(100.0))
                .column(Column::exact(78.0))
                .column(Column::exact(88.0))
                .column(Column::exact(80.0))
                .column(Column::exact(80.0))
                .column(Column::exact(130.0))
                .header(24.0, |mut h| {
                    for t in [
                        "#",
                        "Name",
                        "RX MHz",
                        "TX MHz",
                        "Mode",
                        "Power",
                        "RX Tone",
                        "TX Tone",
                        "TX Contact",
                    ] {
                        h.col(|ui| {
                            ui.strong(t);
                        });
                    }
                })
                .body(|body| {
                    body.rows(26.0, cp.channels.len(), |mut row| {
                        let i = row.index();
                        let number = cp.channels[i].number;
                        row.set_selected(*selected == Some(number));
                        let c = &mut cp.channels[i];
                        let mut hit = false;
                        row.col(|ui| {
                            ui.label(number.to_string());
                        });
                        row.col(|ui| {
                            let r = name_edit(ui, &mut c.name, 16, 166.0);
                            hit |= r.gained_focus();
                        });
                        row.col(|ui| {
                            let listen = listen_band(c.rx_hz).is_some();
                            let (r, v) =
                                freq_edit(ui, ui.scope_id().with(("rx_hz", number)), c.rx_hz, true);
                            hit |= r.gained_focus();
                            if let Some(v) = v {
                                set_rx(c, v);
                            }
                            let _ = listen;
                        });
                        row.col(|ui| {
                            let (r, v) = freq_edit(
                                ui,
                                ui.scope_id().with(("tx_hz", number)),
                                c.tx_hz,
                                listen_band(c.rx_hz).is_none(),
                            );
                            hit |= r.gained_focus();
                            if let Some(v) = v {
                                set_tx(c, v);
                            }
                        });
                        row.col(|ui| {
                            hit |= mode_combo(ui, c, &cp.systems, ("mode", number)).clicked();
                        });
                        row.col(|ui| {
                            hit |= power_combo(ui, c, ("power", number)).clicked();
                        });
                        row.col(|ui| {
                            hit |= tone_field(
                                ui,
                                ui.scope_id().with(("rx_tone", number)),
                                &mut c.rx_tone,
                                70.0,
                            );
                        });
                        row.col(|ui| {
                            hit |= tone_field(
                                ui,
                                ui.scope_id().with(("tx_tone", number)),
                                &mut c.tx_tone,
                                70.0,
                            );
                        });
                        row.col(|ui| {
                            ui.add_enabled_ui(c.mode == Mode::Dmr, |ui| {
                                hit |= contact_combo(ui, c, &choices.contacts, ("contact", number))
                                    .clicked();
                            });
                        });
                        if hit || row.response().clicked() {
                            *selected = Some(number);
                        }
                    });
                });
        });
}

/// The mode as the radio shows it: an FM channel in the air band is received in AM (the web editor says the same). A
/// channel of a kind (P25, NXDN, ...) shows the kind; choosing a mode makes the channel that mode.
fn mode_combo(ui: &mut Ui, c: &mut Channel, systems: &[System], salt: (&str, u16)) -> egui::Response {
    let band = if c.mode == Mode::Fm && c.tagged.is_none() {
        listen_band(c.rx_hz)
    } else {
        None
    };
    let kind = c.kind();
    let shown = match (kind, c.mode, band) {
        (0, Mode::Dmr, _) => "DMR".to_string(),
        (0, _, Some(ListenBand::Air)) => "AM".to_string(),
        (0, _, _) => "FM".to_string(),
        (k, _, _) if modes::kind_name(k) != "?" => modes::kind_name(k).to_string(),
        (k, _, _) => format!("Mode {k}"),
    };
    let r = egui::ComboBox::from_id_salt(salt)
        .selected_text(shown)
        .width(58.0)
        .show_ui(ui, |ui| {
            if ui.selectable_label(kind == 0 && c.mode == Mode::Fm, "FM").clicked() {
                modes::set_kind(c, 0, Mode::Fm, systems);
            }
            if ui.selectable_label(kind == 0 && c.mode == Mode::Dmr, "DMR").clicked() {
                modes::set_kind(c, 0, Mode::Dmr, systems);
            }
            for k in modes::EDITABLE_KINDS {
                if ui.selectable_label(kind == k, modes::kind_name(k)).clicked() {
                    modes::set_kind(c, k, Mode::Dmr, systems);
                }
            }
        })
        .response;
    match band {
        Some(ListenBand::Air) => r.on_hover_text(
            "AM (air band): received in AM, receive only. The channel is set to FM; the radio receives this band in AM.",
        ),
        Some(ListenBand::Broadcast) => {
            r.on_hover_text("FM broadcast band: receive only.")
        }
        None => r,
    }
}

fn power_combo(ui: &mut Ui, c: &mut Channel, salt: (&str, u16)) -> egui::Response {
    egui::ComboBox::from_id_salt(salt)
        .selected_text(power_name(c.power))
        .width(70.0)
        .show_ui(ui, |ui| {
            for (v, n) in POWERS {
                ui.selectable_value(&mut c.power, v, n);
            }
        })
        .response
}

fn contact_combo(
    ui: &mut Ui,
    c: &mut Channel,
    contacts: &[(u16, String)],
    salt: (&str, u16),
) -> egui::Response {
    let shown = if c.contact == 0 {
        "none".to_string()
    } else {
        contacts
            .iter()
            .find(|(n, _)| *n == c.contact)
            .map_or(format!("#{}", c.contact), |(_, n)| n.clone())
    };
    egui::ComboBox::from_id_salt(salt)
        .selected_text(shown)
        .width(120.0)
        .truncate()
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut c.contact, 0, "none");
            for (n, name) in contacts {
                ui.selectable_value(&mut c.contact, *n, name);
            }
        })
        .response
}

/// A tone field; true when it took focus.
fn tone_field(ui: &mut Ui, id: egui::Id, value: &mut String, width: f32) -> bool {
    let (r, v) = checked_text(ui, id, value.clone(), width, parse_tone);
    if let Some(v) = v {
        *value = v;
    }
    r.gained_focus()
}

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

fn detail(ui: &mut Ui, cp: &mut Codeplug, selected: &mut Option<u16>, choices: &Choices) {
    let Some(i) = selected.and_then(|n| cp.channels.iter().position(|c| c.number == n)) else {
        ui.add_space(20.0);
        ui.label(RichText::new("Select a channel to edit it.").color(theme::palette(ui).muted));
        return;
    };
    let c = &mut cp.channels[i];
    let number = c.number;
    let listen = listen_band(c.rx_hz);
    ui.label(RichText::new(format!("Channel {number}")).heading());

    group(ui, "Channel");
    egui::Grid::new(("basic", number))
        .num_columns(2)
        .spacing([14.0, 9.0])
        .show(ui, |ui| {
            ui.label("Name");
            name_edit(ui, &mut c.name, 16, 200.0);
            ui.end_row();
            let l = ui.label("Mode");
            mode_combo(ui, c, &cp.systems, ("dmode", number)).labelled_by(l.id);
            ui.end_row();
            ui.label("Power");
            power_combo(ui, c, ("dpower", number));
            ui.end_row();
            ui.label("RX MHz");
            // A channel of a kind that runs on a system takes its system's control channel's frequencies.
            let own = c.tagged.as_ref().is_none_or(|t| modes::own_freqs(t.kind));
            let (_, v) = freq_edit(ui, egui::Id::unique(("d_rx", number)), c.rx_hz, own);
            if let Some(v) = v {
                set_rx(c, v);
            }
            ui.end_row();
            ui.label("TX MHz");
            let (_, v) = freq_edit(
                ui,
                egui::Id::unique(("d_tx", number)),
                c.tx_hz,
                listen.is_none() && own,
            );
            if let Some(v) = v {
                set_tx(c, v);
            }
            ui.end_row();
        });
    let p = theme::palette(ui);
    if listen.is_some() {
        ui.label(
            RichText::new("Listen-only band: the radio receives here, and TX follows RX.")
                .size(13.0)
                .color(p.muted),
        );
    }
    ui.horizontal_wrapped(|ui| {
        let editable = listen.is_none();
        if ui
            .add_enabled(editable, egui::Button::new("Copy RX to TX"))
            .clicked()
        {
            c.tx_hz = c.rx_hz;
        }
        if ui
            .add_enabled(editable, egui::Button::new("Swap RX and TX"))
            .clicked()
        {
            std::mem::swap(&mut c.rx_hz, &mut c.tx_hz);
        }
        if ui
            .add_enabled(editable, egui::Button::new("Copy TX to RX"))
            .clicked()
        {
            c.rx_hz = c.tx_hz;
        }
    });
    egui::Grid::new(("basic2", number))
        .num_columns(2)
        .spacing([14.0, 9.0])
        .show(ui, |ui| {
            ui.label("TX Timeout (s)");
            let id = egui::Id::unique(("d_tot", number));
            let shown = if c.tot == 0 {
                String::new()
            } else {
                c.tot.to_string()
            };
            let (_, v) = checked_text(ui, id, shown, 100.0, |s| {
                let t = s.trim();
                if t.is_empty() {
                    return Some(0u16);
                }
                t.parse::<u16>()
                    .ok()
                    .filter(|v| *v <= 3825)
                    .map(|v| v.div_ceil(15) * 15)
            });
            if let Some(v) = v {
                c.tot = v;
            }
            if c.tot == 0 {
                ui.label(RichText::new("Radio Default").color(p.muted));
            }
            ui.end_row();
            let l = ui.label("RX Only");
            if let Some(t) = &mut c.tagged {
                // A channel of a kind keeps its record receive only; whether it transmits is the tag's flag.
                let mut rx_only = !t.tx;
                if ui.checkbox(&mut rx_only, "").labelled_by(l.id).changed() {
                    t.tx = !rx_only;
                }
            } else {
                let mut rx_only = c.rx_only || listen.is_some();
                if ui
                    .add_enabled(listen.is_none(), egui::Checkbox::new(&mut rx_only, ""))
                    .changed()
                {
                    c.rx_only = rx_only;
                }
            }
            ui.end_row();
            ui.label("Zone Scan Skip");
            ui.checkbox(&mut c.zone_skip, "");
            ui.end_row();
            ui.label("All-Channel Scan Skip");
            ui.checkbox(&mut c.all_skip, "");
            ui.end_row();
        });

    if c.tagged.is_some() {
        mode_group(ui, c, &cp.systems, choices);
    } else if c.mode == Mode::Fm {
        group(ui, "FM");
        egui::Grid::new(("fm", number))
            .num_columns(2)
            .spacing([14.0, 9.0])
            .show(ui, |ui| {
                ui.label("Bandwidth kHz");
                egui::ComboBox::from_id_salt(("dbw", number))
                    .selected_text(if c.bw == 25.0 { "25 kHz" } else { "12.5 kHz" })
                    .width(100.0)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut c.bw, 12.5, "12.5 kHz");
                        ui.selectable_value(&mut c.bw, 25.0, "25 kHz");
                    });
                ui.end_row();
                ui.label("RX Tone");
                tone_field(
                    ui,
                    egui::Id::unique(("d_rxt", number)),
                    &mut c.rx_tone,
                    100.0,
                );
                ui.end_row();
                ui.label("TX Tone");
                tone_field(
                    ui,
                    egui::Id::unique(("d_txt", number)),
                    &mut c.tx_tone,
                    100.0,
                );
                ui.end_row();
                ui.label("Squelch");
                egui::ComboBox::from_id_salt(("dsq", number))
                    .selected_text(if c.squelch == 0 {
                        "Radio Default".to_string()
                    } else {
                        c.squelch.to_string()
                    })
                    .width(120.0)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut c.squelch, 0, "Radio Default");
                        for n in 1..=21u8 {
                            ui.selectable_value(&mut c.squelch, n, n.to_string());
                        }
                    });
                ui.end_row();
                ui.label("VOX Allowed");
                ui.checkbox(&mut c.vox, "");
                ui.end_row();
                ui.label("1750 Hz Tone Burst");
                ui.checkbox(&mut c.tone_burst, "");
                ui.end_row();
            });
        ui.label(
            RichText::new("Tones are CTCSS (88.5) or DCS (D023N, D023I). Leave empty for none.")
                .size(13.0)
                .color(p.muted),
        );
    } else {
        group(ui, "DMR");
        egui::Grid::new(("dmr", number))
            .num_columns(2)
            .spacing([14.0, 9.0])
            .show(ui, |ui| {
                ui.label("Color Code");
                egui::ComboBox::from_id_salt(("dcc", number))
                    .selected_text(c.color_code.to_string())
                    .width(70.0)
                    .show_ui(ui, |ui| {
                        for n in 0..=15u8 {
                            ui.selectable_value(&mut c.color_code, n, n.to_string());
                        }
                    });
                ui.end_row();
                ui.label("Timeslot");
                egui::ComboBox::from_id_salt(("dts", number))
                    .selected_text(c.timeslot.to_string())
                    .width(70.0)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut c.timeslot, 1, "1");
                        ui.selectable_value(&mut c.timeslot, 2, "2");
                    });
                ui.end_row();
                ui.label("TX Contact");
                contact_combo(ui, c, &choices.contacts, ("dcontact", number));
                ui.end_row();
                ui.label("RX Group List");
                let shown = if c.rx_list == 0 {
                    "none".to_string()
                } else {
                    choices
                        .lists
                        .iter()
                        .find(|(n, _)| *n == c.rx_list)
                        .map_or(format!("#{}", c.rx_list), |(_, n)| n.clone())
                };
                egui::ComboBox::from_id_salt(("dlist", number))
                    .selected_text(shown)
                    .width(160.0)
                    .truncate()
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut c.rx_list, 0, "none");
                        for (n, name) in &choices.lists {
                            ui.selectable_value(&mut c.rx_list, *n, name);
                        }
                    });
                ui.end_row();
                ui.label("Channel DMR ID");
                let id = egui::Id::unique(("d_id", number));
                let shown = if c.dmr_id == 0 {
                    String::new()
                } else {
                    c.dmr_id.to_string()
                };
                let (_, v) = checked_text(ui, id, shown, 120.0, |s| {
                    let t = s.trim();
                    if t.is_empty() {
                        Some(0u32)
                    } else {
                        t.parse::<u32>().ok().filter(|v| *v <= 16_777_215)
                    }
                });
                if let Some(v) = v {
                    c.dmr_id = v;
                }
                ui.end_row();
                ui.label("Force Direct Mode");
                ui.checkbox(&mut c.dmo, "");
                ui.end_row();
            });
    }
}

/// A hex field of up to three digits (a NAC); the value is applied only when the text is one.
fn hex_field(ui: &mut Ui, id: egui::Id, value: &mut u16, max: u16) -> egui::Response {
    let (r, v) = checked_text(ui, id, format!("{:03X}", *value), 70.0, |s| {
        u16::from_str_radix(s.trim(), 16).ok().filter(|v| *v <= max)
    });
    if let Some(v) = v {
        *value = v;
    }
    r
}

/// A number field in `0..=max`.
fn num_field(ui: &mut Ui, id: egui::Id, value: &mut u8, max: u8, width: f32) -> egui::Response {
    let (r, v) = checked_text(ui, id, value.to_string(), width, |s| {
        s.trim().parse::<u8>().ok().filter(|v| *v <= max)
    });
    if let Some(v) = v {
        *value = v;
    }
    r
}

/// The channel's unit ID: its own, or empty for the radio-wide one.
fn unit_field(ui: &mut Ui, c: &mut Channel, number: u16) -> egui::Response {
    let id = egui::Id::unique(("d_unit", number));
    let shown = if c.dmr_id == 0 { String::new() } else { c.dmr_id.to_string() };
    let (r, v) = checked_text(ui, id, shown, 120.0, |s| {
        let t = s.trim();
        if t.is_empty() {
            Some(0u32)
        } else {
            t.parse::<u32>().ok().filter(|v| *v <= 16_777_215)
        }
    });
    if let Some(v) = v {
        c.dmr_id = v;
    }
    r
}

/// The section of a channel of a kind: P25 and NXDN conventional (p25-codeplug.md 2, nxdn-codeplug.md 2); any other
/// kind is kept as it is.
fn mode_group(ui: &mut Ui, c: &mut Channel, systems: &[System], choices: &Choices) {
    let number = c.number;
    let muted = theme::palette(ui).muted;
    let Some(kind) = c.tagged.as_ref().map(|t| t.kind) else { return };
    let params = c.tagged.as_ref().map(|t| t.params.clone());
    match params {
        Some(Params::P25(mut p)) => {
            group(ui, "P25");
            egui::Grid::new(("p25", number)).num_columns(2).spacing([14.0, 9.0]).show(ui, |ui| {
                let l = ui.label("Squelch");
                squelch_combo(ui, ("p25sq", number), &mut p.squelch, "Normal (every call of the NAC)").labelled_by(l.id);
                ui.end_row();
                let l = ui.label("RX NAC");
                hex_field(ui, egui::Id::unique(("p25_rx", number)), &mut p.rx_nac, 0xFFF).labelled_by(l.id);
                ui.end_row();
                let l = ui.label("TX NAC");
                hex_field(ui, egui::Id::unique(("p25_tx", number)), &mut p.tx_nac, 0xFFF).labelled_by(l.id);
                ui.end_row();
                let l = ui.label("TX Contact (talk group)");
                contact_combo(ui, c, &choices.contacts, ("p25contact", number)).labelled_by(l.id);
                ui.end_row();
                let l = ui.label("Channel Unit ID");
                unit_field(ui, c, number).labelled_by(l.id);
                ui.end_row();
                let l = ui.label("Talkaround");
                ui.checkbox(&mut c.dmo, "").labelled_by(l.id);
                ui.end_row();
            });
            ui.label(
                RichText::new("A NAC is three hex digits (293 is the default, F7E any NAC). The TX NAC follows the RX NAC when it is the same. An empty unit ID uses the radio's.")
                    .size(13.0)
                    .color(muted),
            );
            if let Some(t) = &mut c.tagged {
                t.params = Params::P25(p);
            }
        }
        Some(Params::Nxdn(mut p)) => {
            group(ui, "NXDN");
            egui::Grid::new(("nxdn", number)).num_columns(2).spacing([14.0, 9.0]).show(ui, |ui| {
                let l = ui.label("Rate");
                egui::ComboBox::from_id_salt(("nxdnrate", number))
                    .selected_text(if p.rate9600 { "9600 bit/s" } else { "4800 bit/s" })
                    .width(120.0)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut p.rate9600, true, "9600 bit/s");
                        ui.selectable_value(&mut p.rate9600, false, "4800 bit/s");
                    })
                    .response
                    .labelled_by(l.id);
                ui.end_row();
                let l = ui.label("Squelch");
                squelch_combo(ui, ("nxdnsq", number), &mut p.squelch, "RAN (every call of the RAN)").labelled_by(l.id);
                ui.end_row();
                let l = ui.label("RX RAN");
                num_field(ui, egui::Id::unique(("nxdn_rx", number)), &mut p.rx_ran, modes::RAN_MAX, 70.0).labelled_by(l.id);
                ui.end_row();
                let l = ui.label("TX RAN");
                num_field(ui, egui::Id::unique(("nxdn_tx", number)), &mut p.tx_ran, modes::RAN_MAX, 70.0).labelled_by(l.id);
                ui.end_row();
                let l = ui.label("TX Contact (group)");
                contact_combo(ui, c, &choices.contacts, ("nxdncontact", number)).labelled_by(l.id);
                ui.end_row();
                let l = ui.label("Channel Unit ID");
                unit_field(ui, c, number).labelled_by(l.id);
                ui.end_row();
            });
            ui.label(
                RichText::new("A RAN is 0 to 63; 0 receives every RAN. The TX RAN follows the RX RAN (RAN 1 when that is 0) when it is the same. An empty unit ID uses the radio's.")
                    .size(13.0)
                    .color(muted),
            );
            if let Some(t) = &mut c.tagged {
                t.params = Params::Nxdn(p);
            }
        }
        Some(Params::P25Trunk(mut p)) => {
            trunk_group(ui, c, systems, choices, "P25 trunked", rfcps_core::systems::SYSTEM_P25_TRUNK, Some(&mut p.follow_all));
            if let Some(t) = &mut c.tagged {
                t.params = Params::P25Trunk(p);
            }
        }
        Some(Params::T3) => {
            trunk_group(ui, c, systems, choices, "DMR Tier III", rfcps_core::systems::SYSTEM_T3, None);
        }
        _ => {
            group(ui, modes::kind_name(kind));
            ui.label(
                RichText::new(format!(
                    "This channel is of a mode ({}) this editor does not edit. It is kept as it is.",
                    if modes::kind_name(kind) == "?" { format!("kind {kind}") } else { modes::kind_name(kind).to_string() }
                ))
                .color(muted),
            );
        }
    }
}

/// Squelch: 0 plays every call of the NAC or RAN, 1 plays only the calls for this radio (selective).
fn squelch_combo(ui: &mut Ui, salt: (&str, u16), value: &mut u8, open: &str) -> egui::Response {
    egui::ComboBox::from_id_salt(salt)
        .selected_text(if *value == 1 { "Selective" } else { open })
        .width(240.0)
        .show_ui(ui, |ui| {
            ui.selectable_value(value, 0, open);
            ui.selectable_value(value, 1, "Selective (this radio's calls)");
        })
        .response
}

/// The section of a channel that runs on a system (P25 trunked, DMR Tier III): the system, whether it follows every
/// group (P25 trunked), the talk target, the RX group list and no fallback.
fn trunk_group(
    ui: &mut Ui,
    c: &mut Channel,
    systems: &[System],
    choices: &Choices,
    title: &str,
    sys_type: u8,
    follow_all: Option<&mut bool>,
) {
    let number = c.number;
    let muted = theme::palette(ui).muted;
    group(ui, title);
    let mut system = c.tagged.as_ref().map_or(0xFF, |t| t.system);
    let mut no_fallback = c.tagged.as_ref().is_some_and(|t| t.no_fallback);
    egui::Grid::new(("trunk", number)).num_columns(2).spacing([14.0, 9.0]).show(ui, |ui| {
        let l = ui.label("System");
        let shown = systems
            .iter()
            .find(|s| system != 0xFF && s.number == u16::from(system) + 1)
            .map_or_else(
                || if system == 0xFF { "none".to_string() } else { format!("#{} (missing)", u16::from(system) + 1) },
                |s| format!("{}  {}", s.number, s.name),
            );
        egui::ComboBox::from_id_salt(("trunksys", number))
            .selected_text(shown)
            .width(220.0)
            .truncate()
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut system, 0xFF, "none");
                for s in systems.iter().filter(|s| s.type_id == sys_type) {
                    ui.selectable_value(&mut system, (s.number - 1) as u8, format!("{}  {}", s.number, s.name));
                }
            })
            .response
            .labelled_by(l.id);
        ui.end_row();
        if let Some(f) = follow_all {
            let l = ui.label("Follow every group");
            ui.checkbox(f, "").labelled_by(l.id);
            ui.end_row();
        }
        let l = ui.label("TX Contact (talk group)");
        contact_combo(ui, c, &choices.contacts, ("trunkcontact", number)).labelled_by(l.id);
        ui.end_row();
        let l = ui.label("RX Group List");
        let shown = if c.rx_list == 0 {
            "none".to_string()
        } else {
            choices.lists.iter().find(|(n, _)| *n == c.rx_list).map_or(format!("#{}", c.rx_list), |(_, n)| n.clone())
        };
        egui::ComboBox::from_id_salt(("trunklist", number))
            .selected_text(shown)
            .width(160.0)
            .truncate()
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut c.rx_list, 0, "none");
                for (n, name) in &choices.lists {
                    ui.selectable_value(&mut c.rx_list, *n, name);
                }
            })
            .response
            .labelled_by(l.id);
        ui.end_row();
        let l = ui.label("No fallback on this channel");
        ui.checkbox(&mut no_fallback, "").labelled_by(l.id);
        ui.end_row();
    });
    ui.label(
        RichText::new("The channel's frequencies are the system's first control channel's. It transmits only when its RX Only is off and the system allows it.")
            .size(13.0)
            .color(muted),
    );
    if let Some(t) = &mut c.tagged {
        t.system = system;
        t.no_fallback = no_fallback;
    }
}
