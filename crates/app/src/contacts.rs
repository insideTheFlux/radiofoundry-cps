// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux

use crate::widgets::name_edit;
use eframe::egui::{self, Ui};
use egui_extras::{Column, TableBuilder};
use rfcps_core::records::{CallType, Contact, MAX_CONTACTS};
use rfcps_core::Codeplug;

pub fn show(ui: &mut Ui, cp: &mut Codeplug) {
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
            }
        }
        ui.label(format!(
            "{} of {} contacts",
            cp.contacts.len(),
            MAX_CONTACTS
        ));
    });
    ui.add_space(4.0);
    let mut delete: Option<usize> = None;
    TableBuilder::new(ui)
        .striped(true)
        .resizable(true)
        .column(Column::auto().at_least(34.0))
        .column(Column::auto().at_least(150.0))
        .column(Column::auto().at_least(90.0))
        .column(Column::auto().at_least(80.0))
        .column(Column::auto().at_least(70.0))
        .column(Column::auto().at_least(50.0))
        .header(22.0, |mut h| {
            for t in ["#", "Name", "DMR ID", "Type", "Time slot", ""] {
                h.col(|ui| {
                    ui.strong(t);
                });
            }
        })
        .body(|body| {
            body.rows(24.0, cp.contacts.len(), |mut row| {
                let i = row.index();
                let c = &mut cp.contacts[i];
                let n = c.number;
                row.col(|ui| {
                    ui.label(n.to_string());
                });
                row.col(|ui| {
                    name_edit(ui, &mut c.name, 16, 140.0);
                });
                row.col(|ui| {
                    ui.add_enabled(
                        c.call_type != CallType::All,
                        egui::DragValue::new(&mut c.id).range(1..=16_777_215),
                    );
                });
                row.col(|ui| {
                    let label = |t| match t {
                        CallType::Group => "Group",
                        CallType::Private => "Private",
                        CallType::All => "All call",
                    };
                    egui::ComboBox::from_id_salt(("ctype", n))
                        .selected_text(label(c.call_type))
                        .width(70.0)
                        .show_ui(ui, |ui| {
                            for t in [CallType::Group, CallType::Private, CallType::All] {
                                ui.selectable_value(&mut c.call_type, t, label(t));
                            }
                        });
                });
                row.col(|ui| {
                    let t = |v: u8| {
                        if v == 0 {
                            "either".to_string()
                        } else {
                            v.to_string()
                        }
                    };
                    egui::ComboBox::from_id_salt(("cts", n))
                        .selected_text(t(c.timeslot))
                        .width(60.0)
                        .show_ui(ui, |ui| {
                            for v in 0..=2u8 {
                                ui.selectable_value(&mut c.timeslot, v, t(v));
                            }
                        });
                });
                row.col(|ui| {
                    if ui.small_button("Delete").clicked() {
                        delete = Some(i);
                    }
                });
            });
        });
    if let Some(i) = delete {
        let number = cp.contacts[i].number;
        cp.contacts.remove(i);
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
