// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux

//! The Systems tab: the trunked systems channels share (channel-modes.md 7, p25-codeplug.md 3). A list on the left, and
//! for the selected system its identity, how the radio uses it, and its sites, control channels and channel plan. A
//! new system is listen only until the user turns the transmit side on.

use crate::theme;
use crate::widgets::{checked_text, name_edit, parse_mhz};
use eframe::egui::{self, RichText, Ui};
use rfcps_core::systems::{
    self as sys, EntryBody, SysEntry, System, SystemFields, MAX_ENTRIES, MAX_SYSTEMS, SYSTEM_P25_TRUNK,
};
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

pub fn show(ui: &mut Ui, cp: &mut Codeplug, selected: &mut Option<u16>) {
    if selected.is_some_and(|n| cp.system(n).is_none()) {
        *selected = None;
    }
    if selected.is_none() {
        *selected = cp.systems.first().map(|s| s.number);
    }
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(230.0);
            let n_entries: usize = cp.systems.iter().map(|s| s.entries.len()).sum();
            if ui
                .add_enabled(cp.systems.len() < MAX_SYSTEMS, egui::Button::new("Add P25 trunked system"))
                .clicked()
            {
                let mut s = System::new(SYSTEM_P25_TRUNK, &format!("System {}", cp.systems.len() + 1));
                s.name.truncate(16);
                cp.systems.push(s);
                sys::number_systems(&mut cp.systems);
                *selected = cp.systems.last().map(|s| s.number);
            }
            ui.label(format!("{} systems, {n_entries} of {MAX_ENTRIES} entries", cp.systems.len()));
            ui.separator();
            egui::ScrollArea::vertical().id_salt("system_list").show(ui, |ui| {
                for s in &cp.systems {
                    let mode = if s.tx { "" } else { ", listen only" };
                    ui.selectable_value(
                        selected,
                        Some(s.number),
                        format!("{}  {}  ({}{mode})", s.number, s.name, sys::type_name(s.type_id)),
                    );
                }
            });
        });
        ui.separator();
        ui.vertical(|ui| {
            let Some(number) = *selected else {
                ui.label("No systems. A system is what channels of a trunked mode share.");
                return;
            };
            egui::ScrollArea::vertical().id_salt("system_form").auto_shrink([false, false]).show(ui, |ui| {
                form(ui, cp, number, selected);
            });
        });
    });
}

/// A hex field of `digits` digits that may be empty (any); the value is applied only when the text is one.
fn opt_hex<T: Copy + TryFrom<u32> + Into<u32>>(
    ui: &mut Ui,
    id: egui::Id,
    value: &mut Option<T>,
    digits: usize,
    max: u32,
) -> egui::Response {
    let shown = value.map_or_else(String::new, |v| format!("{:0digits$X}", v.into()));
    let (r, v) = checked_text(ui, id, shown, 90.0, |s| {
        let t = s.trim();
        if t.is_empty() {
            Some(None)
        } else {
            u32::from_str_radix(t, 16).ok().filter(|v| *v <= max).and_then(|v| T::try_from(v).ok()).map(Some)
        }
    });
    if let Some(v) = v {
        *value = v;
    }
    r
}

/// A frequency in MHz; empty or 0 is "none" where `zero_ok`.
fn mhz_field(ui: &mut Ui, id: egui::Id, hz: &mut u32, zero_ok: bool) -> egui::Response {
    let shown = if *hz == 0 { String::new() } else { format!("{:.5}", f64::from(*hz) / 1e6) };
    let (r, v) = checked_text(ui, id, shown, 96.0, |s| {
        let t = s.trim();
        if t.is_empty() || t == "0" {
            zero_ok.then_some(0u32)
        } else {
            parse_mhz(t).and_then(|h| u32::try_from(h).ok())
        }
    });
    if let Some(v) = v {
        *hz = v;
    }
    r
}

/// An unsigned number in `0..=max`.
fn num_field<T: Copy + TryFrom<u64> + Into<u64>>(ui: &mut Ui, id: egui::Id, value: &mut T, max: u64, w: f32) -> egui::Response {
    let (r, v) = checked_text(ui, id, Into::<u64>::into(*value).to_string(), w, |s| {
        s.trim().parse::<u64>().ok().filter(|v| *v <= max).and_then(|v| T::try_from(v).ok())
    });
    if let Some(v) = v {
        *value = v;
    }
    r
}

/// An optional byte: empty is "not known".
fn opt_byte(ui: &mut Ui, id: egui::Id, value: &mut Option<u8>, max: u8) -> egui::Response {
    let shown = value.map_or_else(String::new, |v| v.to_string());
    let (r, v) = checked_text(ui, id, shown, 60.0, |s| {
        let t = s.trim();
        if t.is_empty() {
            Some(None)
        } else {
            t.parse::<u8>().ok().filter(|v| *v <= max).map(Some)
        }
    });
    if let Some(v) = v {
        *value = v;
    }
    r
}

fn form(ui: &mut Ui, cp: &mut Codeplug, number: u16, selected: &mut Option<u16>) {
    let muted = theme::palette(ui).muted;
    let on_it = cp
        .channels
        .iter()
        .filter(|c| c.tagged.as_ref().is_some_and(|t| t.system != 0xFF && u16::from(t.system) + 1 == number))
        .count();
    let Some(s) = cp.systems.iter_mut().find(|s| s.number == number) else { return };
    ui.label(RichText::new(format!("System {number}")).heading());

    group(ui, "System");
    egui::Grid::new(("sys_basic", number)).num_columns(2).spacing([14.0, 9.0]).show(ui, |ui| {
        let l = ui.label("Name");
        name_edit(ui, &mut s.name, 16, 200.0).labelled_by(l.id);
        ui.end_row();
        ui.label("Type");
        ui.label(match s.type_id {
            SYSTEM_P25_TRUNK => "P25 trunked".to_string(),
            t => format!("Type {t}"),
        });
        ui.end_row();
        let l = ui.label("Transmit side");
        let text = match (s.tx, matches!(&s.fields, SystemFields::P25Trunk(f) if f.no_reg)) {
            (false, _) => "Listen only",
            (true, false) => "Transmit and register",
            (true, true) => "Transmit without registering",
        };
        let mut choice = 0u8;
        let r = egui::ComboBox::from_id_salt(("sys_tx", number))
            .selected_text(text)
            .width(240.0)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut choice, 1, "Listen only");
                ui.selectable_value(&mut choice, 2, "Transmit and register");
                ui.selectable_value(&mut choice, 3, "Transmit without registering");
            })
            .response;
        r.labelled_by(l.id);
        if choice != 0 {
            s.tx = choice != 1;
            if let SystemFields::P25Trunk(f) = &mut s.fields {
                f.no_reg = choice == 3;
            }
        }
        ui.end_row();
        let l = ui.label("This radio's ID on the system");
        let id = egui::Id::unique(("sys_own", number));
        let shown = if s.own_id == 0 { String::new() } else { s.own_id.to_string() };
        let (r, v) = checked_text(ui, id, shown, 140.0, |t| {
            let t = t.trim();
            if t.is_empty() {
                Some(0u32)
            } else {
                t.parse::<u32>().ok().filter(|v| (1..=0xFF_FFFE).contains(v))
            }
        });
        r.labelled_by(l.id);
        if let Some(v) = v {
            s.own_id = v;
        }
        ui.end_row();
    });
    ui.label(
        RichText::new("A new system is listen only: the radio never registers or transmits on it, whatever a channel says. An empty ID uses the radio's P25 unit ID.")
            .size(13.0)
            .color(muted),
    );

    if let SystemFields::P25Trunk(f) = &mut s.fields {
        group(ui, "Identity");
        egui::Grid::new(("sys_ident", number)).num_columns(2).spacing([14.0, 9.0]).show(ui, |ui| {
            let l = ui.label("WACN");
            opt_hex(ui, egui::Id::unique(("sys_wacn", number)), &mut f.wacn, 5, 0xF_FFFF).labelled_by(l.id);
            ui.end_row();
            let l = ui.label("System ID");
            opt_hex(ui, egui::Id::unique(("sys_sysid", number)), &mut f.sysid, 3, 0xFFF).labelled_by(l.id);
            ui.end_row();
            let l = ui.label("Control channel NAC");
            opt_hex(ui, egui::Id::unique(("sys_ccnac", number)), &mut f.cc_nac, 3, 0xFFF).labelled_by(l.id);
            ui.end_row();
        });
        ui.label(RichText::new("Hex. Empty means any.").size(13.0).color(muted));

        group(ui, "Calls and roaming");
        egui::Grid::new(("sys_calls", number)).num_columns(2).spacing([14.0, 9.0]).show(ui, |ui| {
            let l = ui.label("Skip encrypted calls");
            ui.checkbox(&mut f.skip_enc, "").labelled_by(l.id);
            ui.end_row();
            let l = ui.label("Hold on the voice channel (s)");
            num_field(ui, egui::Id::unique(("sys_hold", number)), &mut f.hold_s, 255, 70.0).labelled_by(l.id);
            ui.end_row();
            let l = ui.label("Use only the programmed channel plan");
            ui.checkbox(&mut f.plan_only, "").labelled_by(l.id);
            ui.end_row();
            let l = ui.label("Change site below (dBm)");
            let id = egui::Id::unique(("sys_roam", number));
            let (r, v) = checked_text(ui, id, f.roam_dbm.to_string(), 70.0, |t| t.trim().parse::<i8>().ok());
            r.labelled_by(l.id);
            if let Some(v) = v {
                f.roam_dbm = v;
            }
            ui.end_row();
        });
        ui.label(RichText::new("0 seconds goes back to the control channel at once. 0 dBm uses the radio's own threshold.").size(13.0).color(muted));
    }

    if s.type_id == SYSTEM_P25_TRUNK {
        entries(ui, s, number);
    }

    group(ui, "Secret");
    if s.secret.is_some() || s.needs_key {
        ui.label(if s.secret.is_some() {
            "This system has a secret. It is kept as it is and never shown here."
        } else {
            "This system needs a secret but has none."
        });
        if s.secret.is_some() && ui.button("Remove the secret").clicked() {
            s.secret = None;
        }
    } else {
        ui.label(RichText::new("No secret. P25 trunked systems don't use one yet.").color(muted));
    }

    group(ui, "Delete");
    let confirm = egui::Id::unique(("sys_delete", number));
    let asking = ui.data(|d| d.get_temp::<bool>(confirm)).unwrap_or(false);
    if asking {
        ui.label(if on_it > 0 {
            format!("Delete this system? {on_it} channel(s) on it become plain receive-only DMR channels.")
        } else {
            "Delete this system?".to_string()
        });
        ui.horizontal(|ui| {
            if ui.button("Yes, delete it").clicked() {
                ui.data_mut(|d| d.remove::<bool>(confirm));
                cp.remove_system(number);
                *selected = None;
            } else if ui.button("Cancel").clicked() {
                ui.data_mut(|d| d.remove::<bool>(confirm));
            }
        });
    } else if ui.button("Delete system…").clicked() {
        ui.data_mut(|d| d.insert_temp(confirm, true));
    }
}

/// Sites, control channels and the channel plan of a P25 trunked system.
fn entries(ui: &mut Ui, s: &mut System, number: u16) {
    let muted = theme::palette(ui).muted;
    let at_limit = s.entries.len() >= MAX_ENTRIES;
    let mut remove: Option<usize> = None;
    let mut swap: Option<(usize, usize)> = None;

    group(ui, "Sites");
    egui::Grid::new(("sys_sites", number)).num_columns(5).spacing([12.0, 6.0]).show(ui, |ui| {
        ui.label("Site");
        ui.label("RFSS");
        ui.label("Site ID");
        ui.label("");
        ui.label("");
        ui.end_row();
        for (i, e) in s.entries.iter_mut().enumerate() {
            let SysEntry { group, body: EntryBody::P25Site { rfss, site }, .. } = e else { continue };
            let l = ui.label(format!("{group}"));
            num_field(ui, egui::Id::unique(("site_no", number, i)), group, 254, 60.0).labelled_by(l.id);
            opt_byte(ui, egui::Id::unique(("site_rfss", number, i)), rfss, 254);
            opt_byte(ui, egui::Id::unique(("site_id", number, i)), site, 254);
            if ui.small_button("Remove").clicked() {
                remove = Some(i);
            }
            ui.end_row();
        }
    });
    if ui.add_enabled(!at_limit, egui::Button::new("Add site")).clicked() {
        let next = s.entries.iter().filter(|e| e.cls == 1).map(|e| e.group).max().map_or(0, |m| m.saturating_add(1).min(254));
        s.entries.push(SysEntry { cls: 1, group: next, control: false, flags: 0, body: EntryBody::P25Site { rfss: None, site: None }, raw: Vec::new() });
    }
    ui.label(RichText::new("The site number is for the list; RFSS and site ID are for display and roaming. Empty means not known.").size(13.0).color(muted));

    group(ui, "Control channels");
    ui.label(RichText::new("In the order tried: the first is the primary. The TX frequency is empty when it comes from the plan's transmit offset.").size(13.0).color(muted));
    let ctl: Vec<usize> = s.entries.iter().enumerate().filter(|(_, e)| e.cls == 0 && e.control).map(|(i, _)| i).collect();
    egui::Grid::new(("sys_ctl", number)).num_columns(7).spacing([12.0, 6.0]).show(ui, |ui| {
        for t in ["Site", "RX MHz", "TX MHz", "Identifier", "Channel", "", ""] {
            ui.label(t);
        }
        ui.end_row();
        for (k, &i) in ctl.iter().enumerate() {
            let e = &mut s.entries[i];
            let SysEntry { group, body: EntryBody::P25Control { iden, chan_no, rx_hz, tx_hz }, .. } = e else { continue };
            let l = ui.label(format!("{}", k + 1));
            num_field(ui, egui::Id::unique(("ctl_site", number, i)), group, 254, 50.0).labelled_by(l.id);
            mhz_field(ui, egui::Id::unique(("ctl_rx", number, i)), rx_hz, false);
            mhz_field(ui, egui::Id::unique(("ctl_tx", number, i)), tx_hz, true);
            num_field(ui, egui::Id::unique(("ctl_iden", number, i)), iden, 15, 50.0);
            num_field(ui, egui::Id::unique(("ctl_chan", number, i)), chan_no, 4095, 60.0);
            ui.horizontal(|ui| {
                if ui.small_button("Up").clicked() && k > 0 {
                    swap = Some((i, ctl[k - 1]));
                }
                if ui.small_button("Down").clicked() && k + 1 < ctl.len() {
                    swap = Some((i, ctl[k + 1]));
                }
            });
            if ui.small_button("Remove").clicked() {
                remove = Some(i);
            }
            ui.end_row();
        }
    });
    if ui.add_enabled(!at_limit, egui::Button::new("Add control channel")).clicked() {
        let site = s.entries.iter().rev().find(|e| e.cls == 0).map_or(0, |e| e.group);
        s.entries.push(SysEntry { cls: 0, group: site, control: true, flags: 0, body: EntryBody::P25Control { iden: 0, chan_no: 0, rx_hz: 0, tx_hz: 0 }, raw: Vec::new() });
    }

    group(ui, "Channel plan");
    ui.label(RichText::new("One row per channel identifier. An identifier not programmed here is learned from the control channel.").size(13.0).color(muted));
    egui::Grid::new(("sys_plan", number)).num_columns(8).spacing([12.0, 6.0]).show(ui, |ui| {
        for t in ["Identifier", "Base MHz", "Spacing Hz", "TX offset Hz", "Bandwidth Hz", "TDMA", "Slots", ""] {
            ui.label(t);
        }
        ui.end_row();
        for (i, e) in s.entries.iter_mut().enumerate() {
            let SysEntry { group, body: EntryBody::P25Plan { base_hz, spacing_hz, tx_offset_hz, bw_hz, tdma, slots }, .. } = e else { continue };
            let l = ui.label(format!("{group}"));
            num_field(ui, egui::Id::unique(("plan_id", number, i)), group, 15, 50.0).labelled_by(l.id);
            mhz_field(ui, egui::Id::unique(("plan_base", number, i)), base_hz, false);
            num_field(ui, egui::Id::unique(("plan_sp", number, i)), spacing_hz, 1_000_000, 80.0);
            let id = egui::Id::unique(("plan_off", number, i));
            let (r, v) = checked_text(ui, id, tx_offset_hz.to_string(), 100.0, |t| t.trim().parse::<i32>().ok());
            let _ = r;
            if let Some(v) = v {
                *tx_offset_hz = v;
            }
            num_field(ui, egui::Id::unique(("plan_bw", number, i)), bw_hz, 1_000_000, 80.0);
            ui.checkbox(tdma, "");
            ui.add_enabled_ui(*tdma, |ui| {
                let mut v = *slots;
                num_field(ui, egui::Id::unique(("plan_slots", number, i)), &mut v, 8, 40.0);
                if (1..=8).contains(&v) {
                    *slots = v;
                }
            });
            if ui.small_button("Remove").clicked() {
                remove = Some(i);
            }
            ui.end_row();
        }
    });
    if ui.add_enabled(!at_limit, egui::Button::new("Add plan row")).clicked() {
        let next = s.entries.iter().filter(|e| e.cls == 2).map(|e| e.group).max().map_or(0, |m| m.saturating_add(1).min(15));
        s.entries.push(SysEntry {
            cls: 2,
            group: next,
            control: false,
            flags: 0,
            body: EntryBody::P25Plan { base_hz: 0, spacing_hz: 12_500, tx_offset_hz: 0, bw_hz: 12_500, tdma: false, slots: 2 },
            raw: Vec::new(),
        });
    }

    if let Some((a, b)) = swap {
        s.entries.swap(a, b);
    }
    if let Some(i) = remove {
        s.entries.remove(i);
    }
}
