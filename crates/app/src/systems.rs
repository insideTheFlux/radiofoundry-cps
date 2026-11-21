// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux

//! The Systems tab: the trunked systems channels share (channel-modes.md 7, p25-codeplug.md 3). A list on the left, and
//! for the selected system its identity, how the radio uses it, and its sites, control channels and channel plan. A
//! new system is listen only until the user turns the transmit side on.

use crate::theme;
use crate::widgets::{checked_text, name_edit, parse_mhz};
use eframe::egui::{self, RichText, Ui};
use rfcps_core::systems::{
    self as sys, EntryBody, SysEntry, System, SystemFields, T3Key, T3System, MAX_ENTRIES, MAX_SYSTEMS, SYSTEM_P25_TRUNK,
    SYSTEM_T3,
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
            for (label, type_id) in [("Add P25 trunked system", SYSTEM_P25_TRUNK), ("Add DMR Tier III system", SYSTEM_T3)] {
                if ui.add_enabled(cp.systems.len() < MAX_SYSTEMS, egui::Button::new(label)).clicked() {
                    let mut s = System::new(type_id, &format!("System {}", cp.systems.len() + 1));
                    s.name.truncate(16);
                    cp.systems.push(s);
                    sys::number_systems(&mut cp.systems);
                    *selected = cp.systems.last().map(|s| s.number);
                }
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
            SYSTEM_T3 => "DMR Tier III".to_string(),
            t => format!("Type {t}"),
        });
        ui.end_row();
        let l = ui.label("Transmit side");
        let text = match (s.tx, matches!(&s.fields, SystemFields::P25Trunk(f) if f.no_reg)) {
            (false, _) => "Listen only",
            (true, _) if s.type_id == SYSTEM_T3 => "Transmit",
            (true, false) => "Transmit and register",
            (true, true) => "Transmit without registering",
        };
        let mut choice = 0u8;
        let r = egui::ComboBox::from_id_salt(("sys_tx", number))
            .selected_text(text)
            .width(240.0)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut choice, 1, "Listen only");
                ui.selectable_value(&mut choice, 2, if s.type_id == SYSTEM_T3 { "Transmit" } else { "Transmit and register" });
                if s.type_id == SYSTEM_P25_TRUNK {
                    ui.selectable_value(&mut choice, 3, "Transmit without registering");
                }
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
        RichText::new("Listen only: the radio never registers or transmits on the system, whatever a channel says. A new P25 trunked system is listen only; a new Tier III system can transmit, as in the web editor. An empty ID uses the radio's own.")
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

    if let SystemFields::T3(f) = &mut s.fields {
        t3_form(ui, f, number);
    }
    if s.type_id == SYSTEM_P25_TRUNK {
        entries(ui, s, number);
    } else if s.type_id == SYSTEM_T3 {
        t3_channels(ui, s, number);
    }

    group(ui, "Secret");
    if s.type_id == SYSTEM_T3 {
        let l = ui.label("Authenticates (needs its key)");
        ui.checkbox(&mut s.needs_key, "").labelled_by(l.id);
        key_section(ui, s, number);
    } else if s.secret.is_some() || s.needs_key {
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

const MODELS: [&str; 4] = ["Tiny", "Small", "Large", "Huge"];
const SEPARATIONS: [&str; 8] = ["5 kHz", "6.25 kHz", "10 kHz", "12.5 kHz", "15 kHz", "20 kHz", "25 kHz", "30 kHz"];

fn combo<T: PartialEq + Copy>(ui: &mut Ui, salt: (&str, u16), label: &str, shown: &str, value: &mut T, items: &[(T, String)]) {
    let l = ui.label(label);
    egui::ComboBox::from_id_salt(salt)
        .selected_text(shown)
        .width(200.0)
        .show_ui(ui, |ui| {
            for (v, name) in items {
                ui.selectable_value(value, *v, name);
            }
        })
        .response
        .labelled_by(l.id);
    ui.end_row();
}

fn check(ui: &mut Ui, label: &str, value: &mut bool) {
    let l = ui.label(label);
    ui.checkbox(value, "").labelled_by(l.id);
    ui.end_row();
}

/// A DMR Tier III system's fields (dmr-trunking-codeplug.md 1).
fn t3_form(ui: &mut Ui, f: &mut T3System, number: u16) {
    let muted = theme::palette(ui).muted;
    group(ui, "Network");
    egui::Grid::new(("t3_net", number)).num_columns(2).spacing([14.0, 9.0]).show(ui, |ui| {
        let items: Vec<(u8, String)> = MODELS.iter().enumerate().map(|(i, n)| (i as u8, (*n).to_string())).collect();
        combo(ui, ("t3_model", number), "Network model", MODELS[usize::from(f.model & 3)], &mut f.model, &items);
        let l = ui.label("NET");
        let max = sys::net_max(f.model);
        num_field(ui, egui::Id::unique(("t3_netno", number)), &mut f.net, u64::from(max), 80.0).labelled_by(l.id);
        ui.end_row();
        let items = [(false, "A".to_string()), (true, "B".to_string())];
        combo(ui, ("t3_cat", number), "Category", if f.category_b { "B" } else { "A" }, &mut f.category_b, &items);
        let items: Vec<(u8, String)> = (0..16u8).map(|n| (n, n.to_string())).collect();
        combo(ui, ("t3_cc", number), "Color code", &f.color_code.to_string(), &mut f.color_code, &items);
    });
    ui.label(RichText::new(format!("NET goes up to {} in this network model.", sys::net_max(f.model))).size(13.0).color(muted));

    group(ui, "Radio");
    egui::Grid::new(("t3_radio", number)).num_columns(2).spacing([14.0, 9.0]).show(ui, |ui| {
        check(ui, "Register with the talkgroups", &mut f.register_talkgroups);
        check(ui, "IP gateway", &mut f.ip_gateway);
        let items: Vec<(u8, String)> =
            (0..8u8).map(|n| (n, if n == 0 { "Off".to_string() } else { format!("1:{}", 1u32 << n) })).collect();
        let shown = if f.power_save == 0 { "Off".to_string() } else { format!("1:{}", 1u32 << f.power_save) };
        combo(ui, ("t3_ps", number), "Power save", &shown, &mut f.power_save, &items);
    });

    group(ui, "Calls");
    egui::Grid::new(("t3_calls", number)).num_columns(2).spacing([14.0, 9.0]).show(ui, |ui| {
        check(ui, "Follow all-calls", &mut f.all_calls);
        check(ui, "Follow emergency calls on any talkgroup", &mut f.emergency_any);
        check(ui, "Follow private calls", &mut f.private_calls);
    });

    group(ui, "Roaming");
    egui::Grid::new(("t3_roam", number)).num_columns(2).spacing([14.0, 9.0]).show(ui, |ui| {
        check(ui, "No roaming", &mut f.no_roaming);
        let l = ui.label("Change site below (dBm)");
        let id = egui::Id::unique(("t3_leave", number));
        let (r, v) = checked_text(ui, id, f.leave_dbm.to_string(), 70.0, |t| t.trim().parse::<i8>().ok());
        r.labelled_by(l.id);
        if let Some(v) = v {
            f.leave_dbm = v;
        }
        ui.end_row();
    });
    ui.label(RichText::new("Sites the radio may use (empty: any). A site value goes up to the number the network model allows.").size(13.0).color(muted));
    let smax = sys::site_max(f.model);
    let mut remove = None;
    for (i, v) in f.sites.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            let l = ui.label(format!("Site {}", i + 1));
            num_field(ui, egui::Id::unique(("t3_site", number, i)), v, u64::from(smax), 80.0).labelled_by(l.id);
            if ui.small_button("Remove").clicked() {
                remove = Some(i);
            }
        });
    }
    if let Some(i) = remove {
        f.sites.remove(i);
    }
    if ui.add_enabled(f.sites.len() < sys::T3_SITES, egui::Button::new("Add site value")).clicked() {
        f.sites.push(0);
    }

    group(ui, "Fallback");
    egui::Grid::new(("t3_fb", number)).num_columns(2).spacing([14.0, 9.0]).show(ui, |ui| {
        let l = ui.label("Hold on the fallback channel (s)");
        num_field(ui, egui::Id::unique(("t3_hold", number)), &mut f.fallback_hold, 255, 70.0).labelled_by(l.id);
        ui.end_row();
    });
    ui.label(RichText::new("0 seconds uses 30.").size(13.0).color(muted));

    group(ui, "Commands from the network");
    egui::Grid::new(("t3_cmd", number)).num_columns(2).spacing([14.0, 9.0]).show(ui, |ui| {
        check(ui, "Accept Ambient Listening", &mut f.accept_als);
        check(ui, "Accept a kill", &mut f.accept_kill);
        check(ui, "Accept a stun", &mut f.accept_stun);
    });

    group(ui, "Fixed channel plan");
    egui::Grid::new(("t3_plan", number)).num_columns(2).spacing([14.0, 9.0]).show(ui, |ui| {
        let l = ui.label("First channel's transmit MHz");
        mhz_field(ui, egui::Id::unique(("t3_pbase", number)), &mut f.plan_base_hz, true).labelled_by(l.id);
        ui.end_row();
        let l = ui.label("First channel's LCN");
        num_field(ui, egui::Id::unique(("t3_plcn", number)), &mut f.plan_first_lcn, u64::from(sys::T3_LCN), 80.0).labelled_by(l.id);
        ui.end_row();
        let items: Vec<(u8, String)> = SEPARATIONS.iter().enumerate().map(|(i, n)| (i as u8, (*n).to_string())).collect();
        combo(ui, ("t3_psep", number), "Separation", SEPARATIONS[usize::from(f.plan_sep & 7)], &mut f.plan_sep, &items);
        let l = ui.label("Duplex split MHz");
        mhz_field(ui, egui::Id::unique(("t3_psplit", number)), &mut f.plan_split_hz, true).labelled_by(l.id);
        ui.end_row();
        check(ui, "Transmit below receive", &mut f.plan_tx_low);
        check(ui, "Don't learn channels from the control channel", &mut f.plan_no_learn);
    });
    ui.label(RichText::new("A base frequency of 0 means no fixed plan.").size(13.0).color(muted));
}

/// A Tier III system's channels: the LCN, the frequencies, a color code and whether it is a control channel. The
/// control channels are tried in the order listed.
fn t3_channels(ui: &mut Ui, s: &mut System, number: u16) {
    let muted = theme::palette(ui).muted;
    let at_limit = s.entries.len() >= MAX_ENTRIES;
    let mut remove: Option<usize> = None;
    let mut swap: Option<(usize, usize)> = None;
    group(ui, "Channels");
    ui.label(RichText::new("The control channels are hunted in the order listed; the system uses the first 4 of its first 8 channels.").size(13.0).color(muted));
    let idx: Vec<usize> = s.entries.iter().enumerate().filter(|(_, e)| matches!(e.body, EntryBody::T3Channel { .. })).map(|(i, _)| i).collect();
    egui::Grid::new(("t3_chans", number)).num_columns(8).spacing([12.0, 6.0]).show(ui, |ui| {
        for t in ["Control", "LCN", "RX MHz", "TX MHz", "Color code", "", "", ""] {
            ui.label(t);
        }
        ui.end_row();
        for (k, &i) in idx.iter().enumerate() {
            let e = &mut s.entries[i];
            let SysEntry { control, body: EntryBody::T3Channel { lcn, rx_hz, tx_hz, cc }, .. } = e else { continue };
            let l = ui.label(format!("{}", k + 1));
            ui.checkbox(control, "").labelled_by(l.id);
            num_field(ui, egui::Id::unique(("t3c_lcn", number, i)), lcn, u64::from(sys::T3_LCN), 60.0);
            mhz_field(ui, egui::Id::unique(("t3c_rx", number, i)), rx_hz, false);
            mhz_field(ui, egui::Id::unique(("t3c_tx", number, i)), tx_hz, true);
            let shown = cc.map_or("System's".to_string(), |c| c.to_string());
            egui::ComboBox::from_id_salt(("t3c_cc", number, i))
                .selected_text(shown)
                .width(90.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(cc, None, "System's");
                    for n in 0..16u8 {
                        ui.selectable_value(cc, Some(n), n.to_string());
                    }
                });
            ui.horizontal(|ui| {
                if ui.small_button("Up").clicked() && k > 0 {
                    swap = Some((i, idx[k - 1]));
                }
                if ui.small_button("Down").clicked() && k + 1 < idx.len() {
                    swap = Some((i, idx[k + 1]));
                }
            });
            if ui.small_button("Remove").clicked() {
                remove = Some(i);
            }
            ui.end_row();
        }
    });
    if ui.add_enabled(!at_limit, egui::Button::new("Add channel")).clicked() {
        s.entries.push(SysEntry {
            cls: 0,
            group: 0xFF,
            control: true,
            flags: 0,
            body: EntryBody::T3Channel { lcn: 1, rx_hz: 0, tx_hz: 0, cc: None },
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

/// The key: shown only as set or not set until the user asks (channel-modes.md 8). A key typed in is masked.
fn key_section(ui: &mut Ui, s: &mut System, number: u16) {
    let muted = theme::palette(ui).muted;
    let reveal_id = egui::Id::unique(("t3_reveal", number));
    // A key shown for one system is hidden again when another is selected.
    let last_id = egui::Id::unique("t3_reveal_for");
    if ui.data(|d| d.get_temp::<u16>(last_id)) != Some(number) {
        ui.data_mut(|d| {
            d.insert_temp(last_id, number);
            d.insert_temp(reveal_id, false);
        });
    }
    let revealed = ui.data(|d| d.get_temp::<bool>(reveal_id)).unwrap_or(false);
    let key = s.secret.as_deref().and_then(sys::decode_t3_key);
    match (&s.secret, &key) {
        (None, _) => {
            ui.label(RichText::new("No key.").color(muted));
        }
        (Some(_), None) => {
            ui.label("The stored secret is shorter than 16 octets: it is not a key.");
        }
        (Some(_), Some(k)) => {
            ui.label("A key is set. It is not shown unless you ask.");
            ui.horizontal(|ui| {
                if ui.button(if revealed { "Hide key" } else { "Show key" }).clicked() {
                    ui.data_mut(|d| d.insert_temp(reveal_id, !revealed));
                }
                if ui.button("Remove the key").clicked() {
                    s.secret = None;
                }
            });
            if revealed {
                ui.monospace(format!("Key  {}", sys::key_hex(&k.key)));
                ui.monospace(format!("PSN  {}", if k.psn == 0 { "none".to_string() } else { k.psn.to_string() }));
            }
        }
    }
    // Setting a key: 32 hex digits and an optional PSN, applied with the button.
    let hex_id = egui::Id::unique(("t3_key_hex", number));
    let psn_id = egui::Id::unique(("t3_key_psn", number));
    let mut hex: String = ui.data(|d| d.get_temp(hex_id)).unwrap_or_default();
    let mut psn: String = ui.data(|d| d.get_temp(psn_id)).unwrap_or_default();
    egui::Grid::new(("t3_keyset", number)).num_columns(2).spacing([14.0, 9.0]).show(ui, |ui| {
        let l = ui.label("New key (32 hex digits)");
        ui.add(egui::TextEdit::singleline(&mut hex).password(true).desired_width(300.0)).labelled_by(l.id);
        ui.end_row();
        let l = ui.label("PSN (empty for none)");
        ui.add(egui::TextEdit::singleline(&mut psn).desired_width(100.0)).labelled_by(l.id);
        ui.end_row();
    });
    let parsed = sys::parse_key_hex(&hex);
    let psn_v: Option<u32> = if psn.trim().is_empty() { Some(0) } else { psn.trim().parse::<u32>().ok().filter(|v| *v <= 0xFF_FFFF) };
    ui.horizontal(|ui| {
        let ok = parsed.is_some() && psn_v.is_some();
        if ui.add_enabled(ok, egui::Button::new("Set the key")).clicked() {
            if let (Some(k), Some(p)) = (parsed, psn_v) {
                s.secret = Some(sys::encode_t3_key(&T3Key { key: k, psn: p }, s.secret.as_deref()));
                hex.clear();
                psn.clear();
            }
        }
        if !hex.is_empty() && parsed.is_none() {
            ui.label(RichText::new("32 hex digits").color(theme::palette(ui).warn));
        }
        if psn_v.is_none() {
            ui.label(RichText::new("The PSN is a number up to 16777215").color(theme::palette(ui).warn));
        }
    });
    ui.data_mut(|d| {
        d.insert_temp(hex_id, hex);
        d.insert_temp(psn_id, psn);
    });
}
