// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux

//! The RX Group Lists tab: named lists of contacts (talkgroups) a DMR channel listens to.

use crate::theme;
use crate::widgets::name_edit;
use eframe::egui::{self, RichText, Ui};
use rfcps_core::records::{RxList, MAX_RX_LISTS};
use rfcps_core::Codeplug;

pub fn show(ui: &mut Ui, cp: &mut Codeplug, selected: &mut usize) {
    if *selected >= cp.rx_lists.len() {
        *selected = cp.rx_lists.len().saturating_sub(1);
    }
    let muted = theme::palette(ui).muted;
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(220.0);
            if ui.add_enabled(cp.rx_lists.len() < MAX_RX_LISTS, egui::Button::new("Add list")).clicked() {
                if let Some(number) = (1..=MAX_RX_LISTS as u8).find(|n| !cp.rx_lists.iter().any(|l| l.number == *n)) {
                    cp.rx_lists.push(RxList { number, name: format!("List {number}"), contacts: Vec::new() });
                    *selected = cp.rx_lists.len() - 1;
                }
            }
            ui.label(RichText::new(format!("{} of {} lists", cp.rx_lists.len(), MAX_RX_LISTS)).color(muted));
            ui.separator();
            egui::ScrollArea::vertical().id_salt("list_list").show(ui, |ui| {
                for (i, l) in cp.rx_lists.iter().enumerate() {
                    ui.selectable_value(selected, i, format!("{}  ({})", l.name, l.contacts.len()));
                }
            });
        });
        ui.separator();
        ui.vertical(|ui| {
            let Some(list) = cp.rx_lists.get(*selected).cloned() else {
                ui.label(RichText::new("No RX group lists. A DMR channel can name one to listen to its talkgroups.").color(muted));
                return;
            };
            let names: Vec<(u16, String)> = cp.contacts.iter().map(|c| (c.number, c.name.clone())).collect();
            let i = *selected;
            let number = list.number;
            ui.horizontal(|ui| {
                let l = ui.label("Name");
                name_edit(ui, &mut cp.rx_lists[i].name, 16, 160.0).labelled_by(l.id);
                if ui.button("Delete list").clicked() {
                    cp.rx_lists.remove(i);
                    for c in &mut cp.channels {
                        if c.rx_list == number {
                            c.rx_list = 0;
                        }
                    }
                }
            });
            ui.separator();
            if i >= cp.rx_lists.len() {
                return;
            }
            let full = list.contacts.len() >= 32;
            ui.horizontal(|ui| {
                let add = egui::ComboBox::from_id_salt("add_list_member").selected_text("Add a contact…").width(240.0).show_ui(ui, |ui| {
                    for (n, name) in names.iter().filter(|(n, _)| !list.contacts.contains(n)) {
                        if ui.add_enabled(!full, egui::Button::new(format!("{n}  {name}"))).clicked() {
                            cp.rx_lists[i].contacts.push(*n);
                        }
                    }
                });
                add.response.accessible_name("Add a contact to the list");
                ui.label(RichText::new(format!("{} of 32 contacts", list.contacts.len())).color(muted));
            });
            ui.add_space(4.0);
            let mut act: Option<(usize, i32)> = None;
            egui::ScrollArea::vertical().id_salt("list_members").show(ui, |ui| {
                for (k, n) in list.contacts.iter().enumerate() {
                    ui.horizontal(|ui| {
                        let name = names.iter().find(|(m, _)| m == n).map_or("(missing contact)", |(_, s)| s.as_str());
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
                let members = &mut cp.rx_lists[i].contacts;
                match d {
                    0 => {
                        members.remove(k);
                    }
                    -1 if k > 0 => members.swap(k, k - 1),
                    1 if k + 1 < members.len() => members.swap(k, k + 1),
                    _ => {}
                }
            }
        });
    });
}
