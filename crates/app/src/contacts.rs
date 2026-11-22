// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux

//! The Contacts tab: a table of talkgroups, private calls and the all call.

use crate::theme;
use crate::widgets::{checked_text, name_edit};
use eframe::egui::{self, Ui};
use egui_extras::{Column, TableBuilder};
use rfcps_core::records::{CallType, Contact, MAX_CONTACTS};
use rfcps_core::Codeplug;

fn type_label(t: CallType) -> &'static str {
    match t {
        CallType::Group => "Group",
        CallType::Private => "Private",
        CallType::All => "All",
    }
}

/// Returns true when the person pressed "Add from RadioID" (the ID Database tab is where that is done).
pub fn show(ui: &mut Ui, cp: &mut Codeplug, selected: &mut Option<u16>) -> bool {
    let mut to_radioid = false;
    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                cp.contacts.len() < MAX_CONTACTS,
                egui::Button::new("Add contact"),
            )
            .clicked()
        {
            if let Some(number) =
                (1..=MAX_CONTACTS as u16).find(|n| !cp.contacts.iter().any(|c| c.number == *n))
            {
                cp.contacts.push(Contact {
                    number,
                    name: format!("Contact {number}"),
                    id: 1,
                    call_type: CallType::Group,
                    timeslot: 0,
                });
                *selected = Some(number);
            }
        }
        if ui
            .add_enabled(selected.is_some(), egui::Button::new("Delete contact"))
            .clicked()
        {
            if let Some(number) = selected.take() {
                cp.contacts.retain(|c| c.number != number);
                for c in &mut cp.channels {
                    if c.contact == number {
                        c.contact = 0;
                    }
                }
                for l in &mut cp.rx_lists {
                    l.contacts.retain(|&n| n != number);
                }
            }
        }
        if ui.button("Add from RadioID").clicked() {
            to_radioid = true;
        }
        ui.label(
            egui::RichText::new(format!(
                "{} of {} contacts",
                cp.contacts.len(),
                MAX_CONTACTS
            ))
            .color(theme::palette(ui).muted),
        );
    });
    ui.add_space(4.0);
    let height = ui.available_height();
    TableBuilder::new(ui)
        .striped(true)
        .resizable(false)
        .sense(egui::Sense::click())
        .max_scroll_height(height)
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .column(Column::exact(46.0))
        .column(Column::exact(190.0))
        .column(Column::exact(120.0))
        .column(Column::exact(100.0))
        .column(Column::exact(92.0))
        .header(24.0, |mut h| {
            for t in ["#", "Name", "DMR ID", "Call Type", "Timeslot"] {
                h.col(|ui| {
                    ui.strong(t);
                });
            }
        })
        .body(|body| {
            body.rows(26.0, cp.contacts.len(), |mut row| {
                let i = row.index();
                let n = cp.contacts[i].number;
                row.set_selected(*selected == Some(n));
                let c = &mut cp.contacts[i];
                let mut hit = false;
                row.col(|ui| {
                    ui.label(n.to_string());
                });
                row.col(|ui| {
                    hit |= name_edit(ui, &mut c.name, 16, 176.0).gained_focus();
                });
                row.col(|ui| {
                    let id = ui.scope_id().with(("cid", n));
                    ui.add_enabled_ui(c.call_type != CallType::All, |ui| {
                        let (r, v) = checked_text(ui, id, c.id.to_string(), 100.0, |s| {
                            s.trim()
                                .parse::<u32>()
                                .ok()
                                .filter(|v| (1..=16_777_215).contains(v))
                        });
                        hit |= r.gained_focus();
                        if let Some(v) = v {
                            c.id = v;
                        }
                    });
                });
                row.col(|ui| {
                    hit |= egui::ComboBox::from_id_salt(("ctype", n))
                        .selected_text(type_label(c.call_type))
                        .width(80.0)
                        .show_ui(ui, |ui| {
                            for t in [CallType::Group, CallType::Private, CallType::All] {
                                ui.selectable_value(&mut c.call_type, t, type_label(t));
                            }
                        })
                        .response
                        .clicked();
                });
                row.col(|ui| {
                    let t = |v: u8| {
                        if v == 0 {
                            "either".to_string()
                        } else {
                            v.to_string()
                        }
                    };
                    hit |= egui::ComboBox::from_id_salt(("cts", n))
                        .selected_text(t(c.timeslot))
                        .width(70.0)
                        .show_ui(ui, |ui| {
                            for v in 0..=2u8 {
                                ui.selectable_value(&mut c.timeslot, v, t(v));
                            }
                        })
                        .response
                        .clicked();
                });
                if hit || row.response().clicked() {
                    *selected = Some(n);
                }
            });
        });
    to_radioid
}
