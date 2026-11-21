// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux

//! The General tab: the radio's name and DMR ID, the call hang times and the boot text.

use crate::theme;
use crate::widgets::{checked_text, name_edit};
use eframe::egui::{self, RichText, Ui};
use rfcps_core::kv::{self, SigRecord};
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

/// `area_newer`: the radio's settings area is of a version this editor doesn't know, so the values below are not written.
pub fn show(ui: &mut Ui, cp: &mut Codeplug, area_newer: bool) {
    let muted = theme::palette(ui).muted;
    let g = &mut cp.general;
    ui.set_max_width(560.0);
    group(ui, "Radio");
    egui::Grid::new("general_radio")
        .num_columns(2)
        .spacing([16.0, 9.0])
        .show(ui, |ui| {
            let l = ui.label("Radio name (callsign)");
            name_edit(ui, &mut g.radio_name, 8, 140.0).labelled_by(l.id);
            ui.end_row();
            let l = ui.label("DMR ID");
            let id = egui::Id::unique("general_dmr_id");
            let shown = if g.dmr_id == 0 {
                String::new()
            } else {
                g.dmr_id.to_string()
            };
            let (r, v) = checked_text(ui, id, shown, 140.0, |s| {
                let t = s.trim();
                if t.is_empty() {
                    Some(0u32)
                } else {
                    t.parse::<u32>().ok().filter(|v| *v <= 16_777_215)
                }
            });
            r.labelled_by(l.id);
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
                let l = ui.label(label);
                let id = egui::Id::unique(("general_hang", salt));
                let (r, v) = checked_text(ui, id, value.to_string(), 80.0, |s| {
                    s.trim().parse::<u8>().ok()
                });
                r.labelled_by(l.id);
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
            let l = ui.label("Line 1");
            name_edit(ui, &mut g.boot_line1, 16, 240.0).labelled_by(l.id);
            ui.end_row();
            let l = ui.label("Line 2");
            name_edit(ui, &mut g.boot_line2, 16, 240.0).labelled_by(l.id);
            ui.end_row();
        });
    ui.add_space(10.0);
    ui.label(
        RichText::new("Settings the editor does not show stay as they are.")
            .size(13.0)
            .color(muted),
    );
    keyed(ui, &mut g.keys, area_newer);
}

const OUTPUTS: [&str; 5] = [
    "The radio's own sound",
    "Off",
    "Speaker",
    "On air",
    "Speaker and on air",
];

/// A field for a key that may be absent. An empty field is an absent key: the radio keeps its own value.
fn id_field(ui: &mut Ui, label: &str, salt: &str, value: &mut Option<u32>) {
    let l = ui.label(label);
    let id = egui::Id::unique(("general_key", salt));
    let shown = value.map_or_else(String::new, |v| v.to_string());
    let (r, v) = checked_text(ui, id, shown, 140.0, |s| {
        let t = s.trim();
        if t.is_empty() {
            Some(None)
        } else {
            t.parse::<u32>().ok().filter(|v| *v <= kv::ID_MAX).map(Some)
        }
    });
    r.labelled_by(l.id);
    if let Some(v) = v {
        *value = v;
    }
    ui.end_row();
}

/// A tone that is on, off or left to the radio (an absent key).
fn tone_field(ui: &mut Ui, label: &str, salt: &str, value: &mut Option<bool>) {
    let l = ui.label(label);
    let text = match value {
        None => "The radio's own setting",
        Some(true) => "On",
        Some(false) => "Off",
    };
    let r = egui::ComboBox::from_id_salt(("general_tone", salt))
        .selected_text(text)
        .width(200.0)
        .show_ui(ui, |ui| {
            ui.selectable_value(value, None, "The radio's own setting");
            ui.selectable_value(value, Some(true), "On");
            ui.selectable_value(value, Some(false), "Off");
        })
        .response;
    r.labelled_by(l.id);
    ui.end_row();
}

fn keyed(ui: &mut Ui, k: &mut kv::Keys, area_newer: bool) {
    let muted = theme::palette(ui).muted;
    let warn = theme::palette(ui).warn;
    group(ui, "Data services");
    if area_newer {
        ui.colored_label(
            warn,
            "The radio's settings area is from a newer version. Values set here are not written to it.",
        );
    }
    egui::Grid::new("general_data")
        .num_columns(2)
        .spacing([16.0, 9.0])
        .show(ui, |ui| {
            id_field(ui, "ARS server ID", "ars", &mut k.ars_server);
            id_field(ui, "IP gateway ID", "gw", &mut k.ip_gateway);
        });
    ui.label(
        RichText::new("A DMR ID. Empty leaves the radio's own setting; 0 turns it off.")
            .size(13.0)
            .color(muted),
    );

    group(ui, "Tones");
    egui::Grid::new("general_tones")
        .num_columns(2)
        .spacing([16.0, 9.0])
        .show(ui, |ui| {
            tone_field(ui, "Call end tone", "end", &mut k.call_end_tone);
            tone_field(ui, "Transmit time-out warning", "tot", &mut k.tot_warn_tone);
            tone_field(ui, "Low battery tone", "bat", &mut k.battery_tone);
            tone_field(ui, "Transmit denied tone", "deny", &mut k.tx_deny_tone);
        });

    group(ui, "Signaling tones");
    ui.label(
        RichText::new("A tone chain of up to ten steps for the start and end of a transmission and the end of a reception. A frequency is a multiple of 10 Hz up to 3400 (0 is a pause), a length a multiple of 10 ms up to 900; a step of length 0 ends the chain.")
            .size(13.0)
            .color(muted),
    );
    for (i, title) in ["Start of transmission", "End of transmission", "End of reception"]
        .into_iter()
        .enumerate()
    {
        sig_event(ui, i, title, &mut k.sig[i]);
    }
}

fn output_combo(ui: &mut Ui, label: &str, salt: (usize, &str), air: bool, value: &mut u8) {
    let l = ui.label(label);
    let n = if air { OUTPUTS.len() } else { 3 };
    let r = egui::ComboBox::from_id_salt(("general_sig_out", salt))
        .selected_text(OUTPUTS[usize::from(*value).min(OUTPUTS.len() - 1)])
        .width(200.0)
        .show_ui(ui, |ui| {
            for (v, name) in OUTPUTS.iter().enumerate().take(n) {
                ui.selectable_value(value, v as u8, *name);
            }
        })
        .response;
    r.labelled_by(l.id);
}

fn sig_event(ui: &mut Ui, event: usize, title: &str, rec: &mut Option<SigRecord>) {
    ui.add_space(6.0);
    let mut set = rec.is_some();
    if ui.checkbox(&mut set, title).changed() {
        *rec = set.then(SigRecord::default);
    }
    let Some(r) = rec else { return };
    // The end of a reception has no on-air output (kv::sig_valid).
    let air = event != 2;
    ui.indent(("general_sig", event), |ui| {
        ui.horizontal(|ui| {
            output_combo(ui, "In DMR", (event, "dmr"), air, &mut r.dmr);
            output_combo(ui, "In FM", (event, "fm"), air, &mut r.fm);
        });
        egui::Grid::new(("general_sig_steps", event))
            .num_columns(3)
            .spacing([12.0, 5.0])
            .show(ui, |ui| {
                ui.label("Step");
                ui.label("Hz");
                ui.label("ms");
                ui.end_row();
                for i in 0..kv::SIG_STEPS {
                    let l = ui.label(format!("{}", i + 1));
                    for (what, w) in [("Hz", 0), ("ms", 1)] {
                        let id = egui::Id::unique(("general_sig_step", event, i, w));
                        let cur = if w == 0 { r.hz[i] } else { r.ms[i] };
                        let (max, unit) = if w == 0 { (kv::SIG_HZ_MAX, kv::SIG_HZ_UNIT) } else { (kv::SIG_MS_MAX, 10) };
                        let (resp, v) = checked_text(ui, id, cur.to_string(), 70.0, |s| {
                            s.trim().parse::<u16>().ok().filter(|v| *v <= max && v % unit == 0)
                        });
                        resp.labelled_by(l.id);
                        let _ = what;
                        if let Some(v) = v {
                            if w == 0 {
                                r.hz[i] = v;
                            } else {
                                r.ms[i] = v;
                            }
                        }
                    }
                    ui.end_row();
                }
            });
    });
}

