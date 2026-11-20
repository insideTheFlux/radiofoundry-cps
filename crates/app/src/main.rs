// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod channels;
mod contacts;
mod doc;
mod general;
mod theme;
mod widgets;
mod worker;
mod zones;

use doc::Doc;
use eframe::egui;
use rfcps_core::Image;
use rfcps_link::PortInfo;
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
    ports: Vec<PortInfo>,
    show_all_ports: bool,
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
        let ports = rfcps_link::ports();
        let port = ports
            .iter()
            .find(|p| p.usb)
            .map(|p| p.name.clone())
            .unwrap_or_default();
        Self {
            ports,
            show_all_ports: false,
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

    fn visible_ports(&self) -> Vec<&PortInfo> {
        self.ports
            .iter()
            .filter(|p| p.usb || self.show_all_ports)
            .collect()
    }

    fn refresh_ports(&mut self) {
        self.ports = rfcps_link::ports();
        let ok = self.visible_ports().iter().any(|p| p.name == self.port);
        if !ok {
            self.port = self
                .visible_ports()
                .first()
                .map(|p| p.name.clone())
                .unwrap_or_default();
        }
    }

    fn port_label(&self) -> String {
        self.ports.iter().find(|p| p.name == self.port).map_or_else(
            || "no port".to_string(),
            |p| format!("{}  {}", p.name, p.description),
        )
    }

    fn start_read(&mut self, ctx: &egui::Context) {
        self.progress = (0, 0);
        self.reading = Some(worker::read_radio(self.port.clone(), ctx.clone()));
        self.status = format!("Reading {}…", self.port);
    }

    fn open_dialog(&mut self) {
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

    fn save_dialog(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Codeplug image", &["bin"])
            .set_file_name("codeplug.bin")
            .save_file()
        else {
            return;
        };
        if let Some(d) = &mut self.doc {
            let img = d.encoded();
            match std::fs::write(&path, img.bytes()) {
                Ok(()) => {
                    d.mark_saved();
                    self.status = format!("Saved {}.", path.display());
                }
                Err(e) => self.status = format!("Could not save {}: {e}", path.display()),
            }
        }
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        let busy = self.reading.is_some();
        ui.horizontal_wrapped(|ui| {
            ui.label(
                egui::RichText::new("RadioFoundry CPS")
                    .strong()
                    .size(17.0)
                    .color(theme::ACCENT),
            );
            ui.separator();
            ui.label("Radio on");
            let label = self.port_label();
            let mut chosen = self.port.clone();
            egui::ComboBox::from_id_salt("port")
                .selected_text(label)
                .width(300.0)
                .show_ui(ui, |ui| {
                    let list: Vec<(String, String)> = self
                        .visible_ports()
                        .iter()
                        .map(|p| (p.name.clone(), format!("{}  {}", p.name, p.description)))
                        .collect();
                    if list.is_empty() {
                        ui.weak("No USB serial port found. Connect the cable and press Refresh.");
                    }
                    for (name, text) in list {
                        ui.selectable_value(&mut chosen, name, text);
                    }
                });
            self.port = chosen;
            if ui
                .checkbox(&mut self.show_all_ports, "all ports")
                .on_hover_text("Also list built-in and legacy serial ports")
                .changed()
            {
                self.refresh_ports();
            }
            if ui.button("Refresh").clicked() {
                self.refresh_ports();
            }
            let read = egui::Button::new(egui::RichText::new("Read from radio").strong())
                .fill(theme::ACCENT_DARK);
            if ui
                .add_enabled(!busy && !self.port.is_empty(), read)
                .clicked()
            {
                self.start_read(&ui.ctx().clone());
            }
            ui.add_enabled(false, egui::Button::new("Write to radio"))
                .on_disabled_hover_text("Writing to a radio comes next.");
            ui.separator();
            if ui.button("Open image…").clicked() {
                self.open_dialog();
            }
            if ui
                .add_enabled(self.doc.is_some(), egui::Button::new("Save image…"))
                .clicked()
            {
                self.save_dialog();
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

    fn welcome(&mut self, ui: &mut egui::Ui) {
        ui.add_space(ui.available_height() * 0.18);
        ui.vertical_centered(|ui| {
            ui.heading("RadioFoundry CPS");
            ui.label(
                egui::RichText::new("Program your radio's channels, zones and contacts.")
                    .color(ui.visuals().weak_text_color()),
            );
            ui.add_space(22.0);
            ui.horizontal(|ui| {
                let w = 2.0 * 190.0 + ui.spacing().item_spacing.x;
                ui.add_space(((ui.available_width() - w) / 2.0).max(0.0));
                let read =
                    egui::Button::new(egui::RichText::new("Read from radio").strong().size(17.0))
                        .fill(theme::ACCENT_DARK)
                        .min_size(egui::vec2(190.0, 44.0));
                if ui
                    .add_enabled(self.reading.is_none() && !self.port.is_empty(), read)
                    .clicked()
                {
                    self.start_read(&ui.ctx().clone());
                }
                if ui
                    .add(
                        egui::Button::new(egui::RichText::new("Open image…").size(17.0))
                            .min_size(egui::vec2(190.0, 44.0)),
                    )
                    .clicked()
                {
                    self.open_dialog();
                }
            });
            ui.add_space(16.0);
            if self.port.is_empty() {
                ui.colored_label(
                    theme::WARN,
                    "No USB serial port found. Connect the radio's cable, then press Refresh.",
                );
            } else {
                ui.label(
                    egui::RichText::new(format!("Radio on {}", self.port_label()))
                        .color(ui.visuals().weak_text_color()),
                );
            }
        });
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
            if self.doc.is_none() {
                self.welcome(ui);
                return;
            }
            let Some(doc) = &mut self.doc else { return };
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
    // On Linux, use the X11 window system (through XWayland on a Wayland desktop): the desktop then draws the normal
    // title bar and buttons, where a Wayland window gets a plain fallback. RFCPS_WAYLAND=1 keeps Wayland.
    #[cfg(target_os = "linux")]
    if std::env::var_os("WINIT_UNIX_BACKEND").is_none()
        && std::env::var_os("RFCPS_WAYLAND").is_none()
        && std::env::var_os("DISPLAY").is_some()
    {
        std::env::set_var("WINIT_UNIX_BACKEND", "x11");
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1100.0, 640.0])
            .with_title("RadioFoundry CPS"),
        ..Default::default()
    };
    eframe::run_native(
        "RadioFoundry CPS",
        options,
        Box::new(|cc| {
            theme::apply(&cc.egui_ctx);
            Ok(Box::new(App::new()))
        }),
    )
}
