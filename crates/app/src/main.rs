// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod channels;
mod contacts;
mod doc;
mod general;
mod widgets;
mod worker;
mod zones;

use doc::Doc;
use eframe::egui;
use rfcps_core::Image;
use std::sync::mpsc::Receiver;
use worker::Msg;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Channels,
    Zones,
    Contacts,
    General,
}

struct App {
    ports: Vec<String>,
    port: String,
    doc: Option<Doc>,
    reading: Option<Receiver<Msg>>,
    progress: (usize, usize),
    status: String,
    tab: Tab,
    zone: usize,
}

impl App {
    fn new() -> Self {
        let ports = rfcps_link::port_names();
        let port = ports.first().cloned().unwrap_or_default();
        Self {
            ports,
            port,
            doc: None,
            reading: None,
            progress: (0, 0),
            status: "Open an image file, or read from a radio.".into(),
            tab: Tab::Channels,
            zone: 0,
        }
    }

    fn open_image(&mut self, bytes: &[u8], source: String) {
        self.doc = Some(Doc::from_image(Image::from_bytes(bytes), source));
        self.zone = 0;
    }

    fn poll_read(&mut self) {
        let Some(rx) = &self.reading else { return };
        let mut done = None;
        while let Ok(m) = rx.try_recv() {
            match m {
                Msg::Progress(d, t) => self.progress = (d, t),
                Msg::Done(r) => done = Some(r),
            }
        }
        if let Some(r) = done {
            self.reading = None;
            match r {
                Ok(img) => {
                    let n = img.len();
                    self.doc = Some(Doc::from_image(img, format!("radio on {}", self.port)));
                    self.zone = 0;
                    self.status = format!("Read the codeplug from {} ({} bytes).", self.port, n);
                }
                Err(e) => self.status = format!("Reading failed: {e}"),
            }
        }
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        let busy = self.reading.is_some();
        ui.horizontal_wrapped(|ui| {
            ui.label("Port");
            egui::ComboBox::from_id_salt("port")
                .selected_text(if self.port.is_empty() {
                    "none"
                } else {
                    &self.port
                })
                .show_ui(ui, |ui| {
                    for p in &self.ports {
                        ui.selectable_value(&mut self.port, p.clone(), p);
                    }
                });
            if ui.button("Refresh").clicked() {
                self.ports = rfcps_link::port_names();
                if !self.ports.contains(&self.port) {
                    self.port = self.ports.first().cloned().unwrap_or_default();
                }
            }
            if ui
                .add_enabled(
                    !busy && !self.port.is_empty(),
                    egui::Button::new("Read from radio"),
                )
                .clicked()
            {
                self.progress = (0, 0);
                self.reading = Some(worker::read_radio(self.port.clone(), ui.ctx().clone()));
                self.status = format!("Reading {}…", self.port);
            }
            ui.add_enabled(false, egui::Button::new("Write to radio"))
                .on_disabled_hover_text("Writing to a radio comes next.");
            ui.separator();
            if ui.button("Open image…").clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("Codeplug image", &["bin"])
                    .pick_file()
                {
                    match std::fs::read(&path) {
                        Ok(b) => {
                            self.open_image(&b, path.display().to_string());
                            self.status = format!("Opened {}.", path.display());
                        }
                        Err(e) => self.status = format!("Could not read {}: {e}", path.display()),
                    }
                }
            }
            let has_doc = self.doc.is_some();
            if ui
                .add_enabled(has_doc, egui::Button::new("Save image…"))
                .clicked()
            {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("Codeplug image", &["bin"])
                    .set_file_name("codeplug.bin")
                    .save_file()
                {
                    if let Some(d) = &mut self.doc {
                        let img = d.encoded();
                        match std::fs::write(&path, img.bytes()) {
                            Ok(()) => {
                                d.mark_saved();
                                self.status = format!("Saved {}.", path.display());
                            }
                            Err(e) => {
                                self.status = format!("Could not save {}: {e}", path.display())
                            }
                        }
                    }
                }
            }
            ui.separator();
            let (undo, redo) = self
                .doc
                .as_ref()
                .map_or((false, false), |d| (d.can_undo(), d.can_redo()));
            if ui.add_enabled(undo, egui::Button::new("Undo")).clicked() {
                if let Some(d) = &mut self.doc {
                    d.undo();
                }
            }
            if ui.add_enabled(redo, egui::Button::new("Redo")).clicked() {
                if let Some(d) = &mut self.doc {
                    d.redo();
                }
            }
        });
        if busy {
            let (d, t) = self.progress;
            ui.add(
                egui::ProgressBar::new(if t == 0 { 0.0 } else { d as f32 / t as f32 })
                    .text(format!("{d}/{t} blocks")),
            );
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.poll_read();
        // Ctrl+Z / Ctrl+Y (or Ctrl+Shift+Z) outside a text field
        let (z, y, shift) = ui.input(|i| {
            (
                i.modifiers.command && i.key_pressed(egui::Key::Z),
                i.modifiers.command && i.key_pressed(egui::Key::Y),
                i.modifiers.shift,
            )
        });
        if !ui.ctx().egui_wants_keyboard_input() {
            if let Some(d) = &mut self.doc {
                if z && shift || y {
                    d.redo();
                } else if z {
                    d.undo();
                }
            }
        }

        egui::Panel::top("top").show(ui, |ui| {
            ui.add_space(4.0);
            self.top_bar(ui);
            ui.add_space(4.0);
        });
        egui::Panel::bottom("status").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(&self.status);
                if let Some(d) = &mut self.doc {
                    ui.separator();
                    ui.label(&d.source);
                    if d.dirty() {
                        ui.colored_label(
                            egui::Color32::from_rgb(210, 140, 30),
                            "edited, not saved",
                        );
                    }
                }
            });
        });
        egui::CentralPanel::default().show(ui, |ui| {
            let Some(doc) = &mut self.doc else {
                ui.add_space(40.0);
                ui.vertical_centered(|ui| {
                    ui.heading("RadioFoundry CPS");
                    ui.label("Read the codeplug from a radio, or open an image file.");
                });
                return;
            };
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.tab, Tab::Channels, "Channels");
                ui.selectable_value(&mut self.tab, Tab::Zones, "Zones");
                ui.selectable_value(&mut self.tab, Tab::Contacts, "Contacts");
                ui.selectable_value(&mut self.tab, Tab::General, "General");
            });
            ui.separator();
            let before = doc.cp.clone();
            match self.tab {
                Tab::Channels => channels::show(ui, &mut doc.cp),
                Tab::Zones => zones::show(ui, &mut doc.cp, &mut self.zone),
                Tab::Contacts => contacts::show(ui, &mut doc.cp),
                Tab::General => general::show(ui, &mut doc.cp),
            }
            let focus = ui.memory(|m| m.focused());
            doc.note(before, focus);
        });
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1100.0, 640.0])
            .with_title("RadioFoundry CPS"),
        ..Default::default()
    };
    eframe::run_native(
        "RadioFoundry CPS",
        options,
        Box::new(|_cc| Ok(Box::new(App::new()))),
    )
}
