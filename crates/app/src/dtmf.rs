// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux

//! The DTMF tab: a table of DTMF contacts (a name and a code of up to 16 digits).

use crate::theme;
use crate::widgets::{checked_text, name_edit};
use eframe::egui::{self, Ui};
use egui_extras::{Column, TableBuilder};
use rfcps_core::records::{valid_dtmf, DtmfContact, MAX_DTMF};
use rfcps_core::Codeplug;

pub fn show(ui: &mut Ui, cp: &mut Codeplug, selected: &mut Option<u8>) {
    ui.horizontal(|ui| {
        if ui
            .add_enabled(cp.dtmf_contacts.len() < MAX_DTMF, egui::Button::new("Add DTMF contact"))
            .clicked()
        {
            if let Some(number) = (1..=MAX_DTMF as u8).find(|n| !cp.dtmf_contacts.iter().any(|d| d.number == *n)) {
                cp.dtmf_contacts.push(DtmfContact { number, name: format!("DTMF {number}"), code: "1".into() });
                *selected = Some(number);
            }
        }
        if ui.add_enabled(selected.is_some(), egui::Button::new("Delete DTMF contact")).clicked() {
            if let Some(n) = selected.take() {
                cp.dtmf_contacts.retain(|d| d.number != n);
            }
        }
        ui.label(
            egui::RichText::new(format!("{} of {} DTMF contacts", cp.dtmf_contacts.len(), MAX_DTMF))
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
        .column(Column::exact(240.0))
        .header(24.0, |mut h| {
            for t in ["#", "Name", "Code (0-9, A-D, *, #)"] {
                h.col(|ui| {
                    ui.strong(t);
                });
            }
        })
        .body(|body| {
            body.rows(26.0, cp.dtmf_contacts.len(), |mut row| {
                let i = row.index();
                let n = cp.dtmf_contacts[i].number;
                row.set_selected(*selected == Some(n));
                let d = &mut cp.dtmf_contacts[i];
                let mut hit = false;
                row.col(|ui| {
                    ui.label(n.to_string());
                });
                row.col(|ui| {
                    hit |= name_edit(ui, &mut d.name, 16, 176.0)
                        .accessible_name(format!("Name, DTMF contact {n}"))
                        .gained_focus();
                });
                row.col(|ui| {
                    let id = ui.scope_id().with(("dtmf_code", n));
                    let (r, v) = checked_text(ui, id, d.code.clone(), 220.0, |s| {
                        let t = s.trim().to_uppercase();
                        valid_dtmf(&t).then_some(t)
                    });
                    let r = r.accessible_name(format!("Code, DTMF contact {n}"));
                    hit |= r.gained_focus();
                    if let Some(v) = v {
                        d.code = v;
                    }
                });
                if hit || row.response().clicked() {
                    *selected = Some(n);
                }
            });
        });
}
