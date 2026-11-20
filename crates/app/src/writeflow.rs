// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux

//! Writing to a radio, step by step: a list of what will change, the go-ahead, a progress bar, and the result.

use crate::doc::Doc;
use crate::theme;
use crate::worker::{self, WriteMsg};
use eframe::egui;
use rfcps_core::diff::{self, Change, Kind};
use rfcps_core::problems::Table;
use rfcps_core::Codeplug;
use std::sync::mpsc::Receiver;

pub enum State {
    Idle,
    /// The changes, whether the codeplug is from a file or new, whether it is empty (no channels, zones or contacts),
    /// and whether the person ticked the box that allows writing an empty one.
    Confirm(Vec<Change>, bool, bool, bool),
    Running {
        rx: Receiver<WriteMsg>,
        phase: String,
        progress: (usize, usize),
    },
    Done {
        ok: bool,
        text: String,
    },
}

pub struct Flow {
    pub state: State,
}

fn table_name(t: Table) -> &'static str {
    match t {
        Table::General => "General",
        Table::Channels => "Channel",
        Table::Zones => "Zone",
        Table::Contacts => "Contact",
        Table::RxLists => "RX group list",
    }
}

impl Flow {
    pub fn new() -> Self {
        Self { state: State::Idle }
    }

    pub fn busy(&self) -> bool {
        !matches!(self.state, State::Idle)
    }

    /// Opens the confirmation for the edits in `doc`.
    pub fn request(&mut self, doc: &Doc) {
        let old = Codeplug::decode(&doc.base);
        let empty = doc.cp.channels.is_empty() && doc.cp.zones.is_empty() && doc.cp.contacts.is_empty();
        self.state = State::Confirm(diff::changes(&old, &doc.cp), doc.from_file, empty, false);
    }

    /// Draws whatever dialog the flow is at, and moves it on. Returns a status line when the write ends.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        port: &str,
        doc: Option<&mut Doc>,
    ) -> Option<String> {
        let mut status = None;
        let mut next: Option<State> = None;
        match &mut self.state {
            State::Idle => {}
            State::Confirm(changes, from_file, empty, ack) => {
                let mut go = false;
                let mut cancel = false;
                let m = egui::Modal::new(egui::Id::unique("write_confirm")).show(ctx, |ui| {
                    ui.set_width(520.0);
                    ui.heading("Write to radio");
                    ui.add_space(6.0);
                    ui.label(format!("Radio on {port}."));
                    ui.label("First, the radio's current codeplug is saved as a backup. Then only the changed blocks are written, and the radio is read back to check them.");
                    ui.add_space(8.0);
                    if *from_file {
                        ui.label("This codeplug was not read from the radio (it is from a file, or new). The radio's codeplug will be replaced by it: whatever differs on the radio is overwritten (only those blocks are written).");
                    } else if changes.is_empty() {
                        ui.label("No records changed.");
                    } else {
                        ui.strong(format!("{} change(s):", changes.len()));
                        egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                            for c in changes.iter() {
                                let (mark, colour) = match c.kind {
                                    Kind::Added => ("+", theme::palette(ui).ok),
                                    Kind::Changed => ("~", theme::palette(ui).warn),
                                    Kind::Removed => ("-", theme::palette(ui).bad),
                                };
                                ui.horizontal(|ui| {
                                    ui.colored_label(colour, mark);
                                    let who = match c.number {
                                        Some(n) => format!("{} {n} {}", table_name(c.table), c.name),
                                        None => table_name(c.table).to_string(),
                                    };
                                    ui.label(if c.detail.is_empty() { who } else { format!("{who}: {}", c.detail) });
                                });
                            }
                        });
                    }
                    if *empty {
                        ui.add_space(8.0);
                        ui.colored_label(
                            theme::palette(ui).bad,
                            "This codeplug is empty: no channels, zones or contacts. Writing it erases all of them from the radio.",
                        );
                        ui.checkbox(ack, "I want to erase the radio's channels, zones and contacts");
                    }
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        let allowed = !*empty || *ack;
                        if ui
                            .add_enabled(allowed, theme::primary(ui, "Write to radio", 15.0))
                            .clicked()
                        {
                            go = true;
                        }
                        if ui.button("Cancel").clicked() {
                            cancel = true;
                        }
                    });
                });
                if cancel || m.should_close() {
                    next = Some(State::Idle);
                } else if go {
                    if let Some(d) = doc {
                        let rx = worker::write_radio(
                            port.to_string(),
                            if d.from_file { None } else { Some(d.base.clone()) },
                            d.encoded(),
                            ctx.clone(),
                        );
                        next = Some(State::Running {
                            rx,
                            phase: "Starting…".into(),
                            progress: (0, 0),
                        });
                    } else {
                        next = Some(State::Idle);
                    }
                }
            }
            State::Running {
                rx,
                phase,
                progress,
            } => {
                let mut done = None;
                while let Ok(m) = rx.try_recv() {
                    match m {
                        WriteMsg::Phase(p) => {
                            *phase = p;
                            *progress = (0, 0);
                        }
                        WriteMsg::Progress(d, t) => *progress = (d, t),
                        WriteMsg::Done(r) => done = Some(r),
                    }
                }
                egui::Modal::new(egui::Id::unique("write_running")).show(ctx, |ui| {
                    ui.set_width(420.0);
                    ui.heading("Writing to the radio");
                    ui.add_space(6.0);
                    ui.label(phase.as_str());
                    let (d, t) = *progress;
                    ui.add(
                        egui::ProgressBar::new(if t == 0 { 0.0 } else { d as f32 / t as f32 })
                            .text(if t == 0 {
                                String::new()
                            } else {
                                format!("{d}/{t} blocks")
                            }),
                    );
                    ui.add_space(6.0);
                    ui.label(
                        egui::RichText::new("Don't unplug the radio or turn it off.")
                            .color(theme::palette(ui).warn),
                    );
                });
                if let Some(r) = done {
                    next = Some(match r {
                        Ok(w) => {
                            if let Some(d) = doc {
                                d.mark_saved();
                            }
                            let text = if w.blocks == 0 {
                                "The radio already holds this codeplug; nothing needed writing, so no backup was made.".to_string()
                            } else {
                                format!("The radio now holds this codeplug ({} block(s) written and read back to verify). Backup of the old codeplug: {}", w.blocks, w.backup.display())
                            };
                            status = Some(text.clone());
                            State::Done { ok: true, text }
                        }
                        Err(e) => {
                            status = Some(format!("Write failed: {e}"));
                            State::Done { ok: false, text: e }
                        }
                    });
                }
            }
            State::Done { ok, text } => {
                let mut close = false;
                let m = egui::Modal::new(egui::Id::unique("write_done")).show(ctx, |ui| {
                    ui.set_width(520.0);
                    ui.heading(if *ok {
                        "Written"
                    } else {
                        "The write did not complete"
                    });
                    ui.add_space(6.0);
                    ui.label(text.as_str());
                    ui.add_space(12.0);
                    if ui.button("Close").clicked() {
                        close = true;
                    }
                });
                if close || m.should_close() {
                    next = Some(State::Idle);
                }
            }
        }
        if let Some(n) = next {
            self.state = n;
        }
        status
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rfcps_core::Image;

    #[test]
    fn an_empty_codeplug_needs_the_box_ticked_before_it_can_be_written() {
        let mut f = Flow { state: State::Idle };
        let mut d = Doc::from_image(Image::blank(), "new".into());
        f.request(&d);
        assert!(matches!(f.state, State::Confirm(_, _, true, false)));
        d.cp.channels.push(rfcps_core::records::Channel::new(1, 146_520_000, 146_520_000));
        f.request(&d);
        assert!(matches!(f.state, State::Confirm(_, _, false, false)));
    }
}
