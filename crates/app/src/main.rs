// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod channels;
mod cli;
mod contacts;
mod doc;
mod download;
mod radioid_api;
mod exchange;
mod general;
mod iddbtab;
mod lists;
mod aprs;
mod dtmf;
mod systems;
mod theme;
mod widgets;
mod worker;
mod writeflow;
mod zones;

#[cfg(test)]
mod gui_tests;

use doc::Doc;
use eframe::egui;
use rfcps_core::freqs;
use rfcps_core::problems::{self, Level, Problem, Table};
use rfcps_core::Image;
use rfcps_link::PortInfo;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, TryRecvError};
use worker::Msg;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Channels,
    Zones,
    Contacts,
    IdDb,
    Lists,
    Dtmf,
    Aprs,
    Systems,
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
    channel: Option<u16>,
    contact: Option<u16>,
    iddb: iddbtab::IdDbUi,
    /// What the last read learned about the radio (for the ID database).
    radio_facts: worker::RadioFacts,
    list: usize,
    /// The selected system's number (the Systems tab).
    system: Option<u16>,
    dtmf: Option<u8>,
    aprs: Option<u8>,
    flow: writeflow::Flow,
    problems: Vec<Problem>,
    problems_key: (u64, u64),
    doc_serial: u64,
    show_problems: bool,
    notice: Option<(String, Vec<String>)>,
    /// Asking whether to throw away unsaved edits for a new codeplug.
    confirm_new: bool,
    /// Asking whether a saved image may carry the codeplug's secret keys: how many it holds.
    confirm_keys: Option<usize>,
    /// The green or red bar under the toolbar after a write; the x closes it.
    banner: Option<(bool, String)>,
    /// A file chooser that is open (on its own thread, so the window stays alive) and what to do with its answer.
    chooser: Option<(Choose, Receiver<Vec<PathBuf>>)>,
    /// The window's context, for the chooser's thread to repaint it.
    ctx: Option<egui::Context>,
}

/// What a file chooser that is open is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Choose {
    Open,
    /// Save image, with or without the codeplug's secret keys.
    Save(bool),
    ImportChirp,
    ImportQdmr,
    ImportCsv,
    ImportStock,
    ExportQdmr,
    ExportCsv,
    ExportChirp,
}

impl App {
    fn new() -> Self {
        Self::with_ports(rfcps_link::ports())
    }

    /// The app with a given port list. The GUI tests pass none, so they never see or open a real radio.
    fn with_ports(ports: Vec<PortInfo>) -> Self {
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
            channel: None,
            contact: None,
            iddb: iddbtab::IdDbUi::default(),
            radio_facts: worker::RadioFacts::default(),
            list: 0,
            system: None,
            dtmf: None,
            aprs: None,
            flow: writeflow::Flow::new(),
            problems: Vec::new(),
            problems_key: (u64::MAX, u64::MAX),
            doc_serial: 0,
            show_problems: true,
            notice: None,
            confirm_new: false,
            confirm_keys: None,
            banner: None,
            chooser: None,
            ctx: None,
        }
    }

    fn open_image(&mut self, bytes: &[u8], source: String) {
        self.doc = Some(Doc::from_image(Image::from_bytes(bytes), source));
        self.doc_serial += 1;
        self.zone = 0;
    }

    fn poll_read(&mut self) {
        let Some(rx) = &self.reading else { return };
        let mut done = None;
        while let Ok(m) = rx.try_recv() {
            match m {
                Msg::Facts(f) => self.radio_facts = f,
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
                    self.doc_serial += 1;
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

    /// An empty codeplug to fill in. It did not come from the radio, so writing it replaces what the radio holds.
    fn new_codeplug(&mut self) {
        let mut d = Doc::from_image(Image::blank(), "new codeplug".to_string());
        d.from_file = true;
        self.doc = Some(d);
        self.doc_serial += 1;
        self.zone = 0;
        self.channel = None;
        self.contact = None;
        self.list = 0;
        self.banner = None;
        self.status = "A new, empty codeplug.".into();
    }

    fn ask_new(&mut self) {
        let unsaved = self.doc.as_mut().is_some_and(|d| d.unsaved());
        if unsaved {
            self.confirm_new = true;
        } else {
            self.new_codeplug();
        }
    }

    /// Opens a file chooser without blocking the window; `chosen` takes its answer.
    fn pick(&mut self, what: Choose, run: impl FnOnce(rfd::FileDialog) -> Vec<PathBuf> + Send + 'static) {
        if self.chooser.is_some() {
            return;
        }
        let Some(ctx) = self.ctx.clone() else { return };
        self.chooser = Some((what, widgets::spawn_dialog(ctx, run)));
    }

    /// Takes the answer of a file chooser when it has come.
    fn poll_chooser(&mut self) {
        let Some((what, rx)) = &self.chooser else { return };
        let what = *what;
        match rx.try_recv() {
            Ok(paths) => {
                self.chooser = None;
                self.chosen(what, paths);
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => self.chooser = None,
        }
    }

    fn chosen(&mut self, what: Choose, paths: Vec<PathBuf>) {
        let Some(first) = paths.first().cloned() else { return };
        match what {
            Choose::Open => self.open_path(&first),
            Choose::Save(keys) => self.save_to(&first, keys),
            Choose::ImportChirp => self.run_import("Import CHIRP CSV", "CHIRP import", |cp| exchange::import_chirp_file(cp, &first)),
            Choose::ImportQdmr => self.run_import("Import qdmr codeplug", "qdmr import", |cp| exchange::import_qdmr_file(cp, &first)),
            Choose::ImportCsv => self.run_import("Import CSV tables", "CSV import", |cp| exchange::import_csv_files(cp, &paths)),
            Choose::ImportStock => self.run_import("Import stock DM-32UV codeplug", "stock DM-32UV import", |cp| {
                exchange::import_stock_file(cp, &first)
            }),
            Choose::ExportQdmr | Choose::ExportCsv | Choose::ExportChirp => {
                let Some(doc) = &self.doc else { return };
                let (title, r) = match what {
                    Choose::ExportQdmr => ("Export qdmr codeplug", exchange::export_qdmr_file(&doc.cp, &first)),
                    Choose::ExportCsv => ("Export CSV tables", exchange::export_csv_folder(&doc.cp, &first)),
                    _ => ("Export CHIRP CSV", exchange::export_chirp_file(&doc.cp, &first)),
                };
                self.finish_export(title, r);
            }
        }
    }

    fn open_dialog(&mut self) {
        self.pick(Choose::Open, |d| d.add_filter("Codeplug image", &["bin"]).pick_file().into_iter().collect());
    }

    fn open_path(&mut self, path: &std::path::Path) {
        match std::fs::read(path) {
            Ok(b) => {
                self.open_image(&b, path.display().to_string());
                if let Some(d) = &mut self.doc {
                    d.from_file = true;
                }
                self.status = format!("Opened {}.", path.display());
            }
            Err(e) => self.status = format!("Could not read {}: {e}", path.display()),
        }
    }

    /// Save image: asks first when the codeplug holds secret keys (channel-modes.md 8), which a file leaves out unless
    /// the user says so.
    fn save_clicked(&mut self) {
        let n = self.doc.as_ref().map_or(0, |d| rfcps_core::kv::count_secrets(d.encoded().bytes()));
        if n > 0 {
            self.confirm_keys = Some(n);
        } else {
            self.save_dialog(true);
        }
    }

    fn save_dialog(&mut self, include_keys: bool) {
        self.pick(Choose::Save(include_keys), |d| {
            d.add_filter("Codeplug image", &["bin"]).set_file_name("codeplug.bin").save_file().into_iter().collect()
        });
    }

    fn save_to(&mut self, path: &std::path::Path, include_keys: bool) {
        if let Some(d) = &mut self.doc {
            let mut img = d.encoded();
            let secrets = rfcps_core::kv::count_secrets(img.bytes());
            if !include_keys {
                rfcps_core::kv::without_secrets(img.bytes_mut());
            }
            match std::fs::write(path, img.bytes()) {
                Ok(()) if secrets > 0 && !include_keys => {
                    // The file differs from the codeplug: it is not marked saved.
                    self.status = format!("Saved {} without its {secrets} secret key(s).", path.display());
                }
                Ok(()) => {
                    d.mark_file_saved();
                    self.status = format!("Saved {}.", path.display());
                }
                Err(e) => self.status = format!("Could not save {}: {e}", path.display()),
            }
        }
    }

    /// Runs an import on the open codeplug (a new empty one if none is open) as one undoable edit, and shows what came
    /// in and what could not.
    fn run_import(
        &mut self,
        title: &str,
        source: &str,
        f: impl FnOnce(&mut rfcps_core::Codeplug) -> Result<Vec<String>, String>,
    ) {
        if self.doc.is_none() {
            self.doc = Some(Doc::from_image(Image::blank(), source.to_string()));
            self.doc_serial += 1;
        }
        let Some(doc) = &mut self.doc else { return };
        let before = doc.cp.clone();
        match f(&mut doc.cp) {
            Ok(lines) => {
                doc.note(before, None);
                self.status = format!("{title}: done.");
                self.notice = Some((title.to_string(), lines));
            }
            Err(e) => {
                doc.cp = before;
                self.status = format!("{title}: {e}");
                self.notice = Some((format!("{title}: nothing was imported"), vec![e]));
            }
        }
    }

    fn import_chirp_dialog(&mut self) {
        self.pick(Choose::ImportChirp, |d| d.add_filter("CHIRP CSV", &["csv"]).pick_file().into_iter().collect());
    }

    fn import_qdmr_dialog(&mut self) {
        self.pick(Choose::ImportQdmr, |d| d.add_filter("qdmr codeplug", &["yaml", "yml"]).pick_file().into_iter().collect());
    }

    fn export_qdmr_dialog(&mut self) {
        if self.doc.is_none() {
            return;
        }
        let name = format!("codeplug-{}.yaml", worker::stamp());
        self.pick(Choose::ExportQdmr, move |d| {
            d.add_filter("qdmr codeplug", &["yaml"]).set_file_name(name).save_file().into_iter().collect()
        });
    }

    fn import_csv_dialog(&mut self) {
        self.pick(Choose::ImportCsv, |d| d.add_filter("CSV tables", &["csv"]).pick_files().unwrap_or_default());
    }

    fn import_stock_dialog(&mut self) {
        self.pick(Choose::ImportStock, |d| {
            d.add_filter("Stock DM-32UV backup", &["zip", "bin"]).pick_file().into_iter().collect()
        });
    }

    fn export_csv_dialog(&mut self) {
        if self.doc.is_none() {
            return;
        }
        self.pick(Choose::ExportCsv, |d| {
            d.set_title("Choose a folder for the CSV files").pick_folder().into_iter().collect()
        });
    }

    fn export_chirp_dialog(&mut self) {
        if self.doc.is_none() {
            return;
        }
        self.pick(Choose::ExportChirp, |d| {
            d.add_filter("CHIRP CSV", &["csv"]).set_file_name("channels-chirp.csv").save_file().into_iter().collect()
        });
    }

    fn finish_export(&mut self, title: &str, r: Result<Vec<String>, String>) {
        match r {
            Ok(lines) => {
                self.status = format!("{title}: done.");
                self.notice = Some((title.to_string(), lines));
            }
            Err(e) => {
                self.status = format!("{title}: {e}");
                self.notice = Some((format!("{title}: failed"), vec![e]));
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
                    .color(theme::palette(ui).accent),
            );
            ui.separator();
            ui.label("Radio on");
            let label = self.port_label();
            let mut chosen = self.port.clone();
            let mut show_all = self.show_all_ports;
            let list: Vec<(String, String)> = self
                .visible_ports()
                .iter()
                .map(|p| (p.name.clone(), format!("{}  {}", p.name, p.description)))
                .collect();
            egui::ComboBox::from_id_salt("port")
                .selected_text(label)
                .width(300.0)
                .show_ui(ui, |ui| {
                    if list.is_empty() {
                        ui.weak("No USB serial port found. Connect the cable and press Refresh.");
                    }
                    for (name, text) in list {
                        ui.selectable_value(&mut chosen, name, text);
                    }
                    ui.separator();
                    ui.checkbox(&mut show_all, "Show built-in and legacy ports too");
                })
                .response
                .on_hover_text("Serial port of the radio's programming cable");
            self.port = chosen;
            if show_all != self.show_all_ports {
                self.show_all_ports = show_all;
                self.refresh_ports();
            }
            if ui.button("Refresh").clicked() {
                self.refresh_ports();
            }
            let read = theme::primary(ui, "Read from radio", 15.0);
            if ui
                .add_enabled(!busy && !self.port.is_empty(), read)
                .clicked()
            {
                self.start_read(&ui.ctx().clone());
            }
            let errors = problems::error_count(&self.problems);
            let dirty = self.doc.as_mut().is_some_and(|d| d.dirty() || d.from_file || d.iddb_pending);
            let can_write = self.doc.is_some()
                && errors == 0
                && dirty
                && !busy
                && !self.flow.busy()
                && !self.port.is_empty();
            let why = if self.doc.is_none() {
                "Read from a radio or open an image first."
            } else if errors > 0 {
                "Fix the errors in the Problems list first."
            } else if !dirty {
                "Nothing has changed since the codeplug was read."
            } else {
                "Choose the radio's port first."
            };
            let write = ui
                .add_enabled(can_write, egui::Button::new("Write to radio"))
                .on_disabled_hover_text(why);
            if write.clicked() {
                if let Some(d) = &self.doc {
                    self.flow.request(d);
                }
            }
        });
        // File and edit buttons on a row of their own, so a window resize never splits them from each other
        ui.horizontal_wrapped(|ui| {
            if ui.button("New").clicked() {
                self.ask_new();
            }
            if ui.button("Open image…").clicked() {
                self.open_dialog();
            }
            if ui
                .add_enabled(self.doc.is_some(), egui::Button::new("Save image…"))
                .clicked()
            {
                self.save_clicked();
            }
            ui.separator();
            ui.menu_button("Import", |ui| {
                if ui.button("CHIRP CSV…").clicked() {
                    ui.close();
                    self.import_chirp_dialog();
                }
                if ui.button("qdmr codeplug (YAML)…").clicked() {
                    ui.close();
                    self.import_qdmr_dialog();
                }
                if ui.button("CSV tables…").clicked() {
                    ui.close();
                    self.import_csv_dialog();
                }
                if ui.button("Stock DM-32UV backup…").clicked() {
                    ui.close();
                    self.import_stock_dialog();
                }
            });
            ui.add_enabled_ui(self.doc.is_some(), |ui| {
                ui.menu_button("Export", |ui| {
                    if ui.button("CSV tables (a folder)…").clicked() {
                        ui.close();
                        self.export_csv_dialog();
                    }
                    if ui.button("qdmr codeplug (YAML)…").clicked() {
                        ui.close();
                        self.export_qdmr_dialog();
                    }
                    if ui.button("CHIRP CSV…").clicked() {
                        ui.close();
                        self.export_chirp_dialog();
                    }
                });
            });
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
                    theme::primary(ui, "Read from radio", 17.0).min_size(egui::vec2(190.0, 44.0));
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
            ui.add_space(10.0);
            if ui.link("Start a new, empty codeplug").clicked() {
                self.new_codeplug();
            }
            ui.add_space(16.0);
            if self.port.is_empty() {
                ui.colored_label(
                    theme::palette(ui).warn,
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
        // egui leaves its root window node unnamed; a screen reader then announces an empty window
        ui.ctx().accesskit_node_builder(egui::accesskit_root_id(), |n| n.set_label("RadioFoundry CPS"));
        self.ctx = Some(ui.ctx().clone());
        self.poll_read();
        self.poll_chooser();
        if let Some(d) = &self.doc {
            let key = (self.doc_serial, d.generation);
            if key != self.problems_key {
                self.problems = problems::validate(&d.cp, None);
                self.problems.extend(problems::area_problems(&d.cp, &d.base));
                self.problems_key = key;
            }
        } else {
            self.problems.clear();
            self.problems_key = (u64::MAX, u64::MAX);
        }
        if let Some((title, lines)) = self.notice.clone() {
            let mut close = false;
            let m = egui::Modal::new(egui::Id::unique("notice")).show(ui.ctx(), |ui| {
                ui.set_width(560.0);
                ui.heading(&title);
                ui.add_space(6.0);
                egui::ScrollArea::vertical()
                    .max_height(320.0)
                    .show(ui, |ui| {
                        for l in &lines {
                            ui.label(l);
                        }
                    });
                ui.add_space(10.0);
                if ui.add(theme::primary(ui, "OK", 15.0)).clicked() {
                    close = true;
                }
            });
            if close || m.should_close() {
                self.notice = None;
            }
        }
        if let Some(n) = self.confirm_keys {
            let (mut without, mut with, mut cancel) = (false, false, false);
            let m = egui::Modal::new(egui::Id::unique("confirm_keys")).show(ui.ctx(), |ui| {
                ui.set_width(480.0);
                ui.heading("Save the secret keys too?");
                ui.add_space(6.0);
                ui.label(format!(
                    "This codeplug holds {n} secret key(s). A saved file can be copied or shared, so by default it leaves them out."
                ));
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui.add(theme::primary(ui, "Save without keys", 15.0)).clicked() {
                        without = true;
                    }
                    if ui.button("Include the keys").clicked() {
                        with = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                });
            });
            if without || with || cancel || m.should_close() {
                self.confirm_keys = None;
            }
            if without {
                self.save_dialog(false);
            } else if with {
                self.save_dialog(true);
            }
        }
        if self.confirm_new {
            let (mut go, mut cancel) = (false, false);
            let m = egui::Modal::new(egui::Id::unique("confirm_new")).show(ui.ctx(), |ui| {
                ui.set_width(440.0);
                ui.heading("Start a new codeplug?");
                ui.add_space(6.0);
                ui.label("The edits in this codeplug have not been saved to a file or written to the radio. A new codeplug replaces it.");
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui.add(theme::primary(ui, "Start new", 15.0)).clicked() {
                        go = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                });
            });
            if go {
                self.new_codeplug();
            }
            if go || cancel || m.should_close() {
                self.confirm_new = false;
            }
        }
        let ctx = ui.ctx().clone();
        let port = self.port.clone();
        if let Some(msg) = self.flow.show(&ctx, &port, self.doc.as_mut()) {
            self.banner = Some((!msg.starts_with("Write failed"), msg.clone()));
            self.status = msg;
        }
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
        if let Some((ok, text)) = self.banner.clone() {
            let p = theme::palette(ui);
            let colour = if ok { p.ok } else { p.bad };
            let mut close = false;
            egui::Panel::top("banner").show(ui, |ui| {
                egui::Frame::new()
                    .fill(colour.gamma_multiply(0.16))
                    .stroke(egui::Stroke::new(1.0, colour))
                    .corner_radius(4.0)
                    .inner_margin(egui::Margin::symmetric(10, 6))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(text.as_str());
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.small_button("x").on_hover_text("Close").clicked() {
                                    close = true;
                                }
                            });
                        });
                    });
            });
            if close {
                self.banner = None;
            }
        }
        egui::Panel::bottom("status").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(&self.status);
                if let Some(d) = &mut self.doc {
                    ui.separator();
                    ui.label(
                        std::path::Path::new(&d.source)
                            .file_name()
                            .map_or_else(|| d.source.clone(), |n| n.to_string_lossy().into_owned()),
                    );
                    if d.unsaved() {
                        ui.colored_label(theme::palette(ui).warn, "Unsaved changes");
                    } else {
                        ui.label(egui::RichText::new("No unsaved changes").color(theme::palette(ui).muted));
                    }
                    let (e, w) = (
                        problems::error_count(&self.problems),
                        self.problems
                            .iter()
                            .filter(|p| p.level == Level::Warning)
                            .count(),
                    );
                    let p = theme::palette(ui);
                    let text = if e + w == 0 {
                        "No problems".to_string()
                    } else {
                        format!("{e} error(s), {w} warning(s)")
                    };
                    let colour = if e > 0 {
                        p.bad
                    } else if w > 0 {
                        p.warn
                    } else {
                        p.ok
                    };
                    if ui
                        .add(egui::Button::new(egui::RichText::new(text).color(colour)).small())
                        .on_hover_text("Show or hide the Problems list")
                        .clicked()
                    {
                        self.show_problems = !self.show_problems;
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let mut pref = ui.ctx().options(|o| o.theme_preference);
                    // Text labels: the symbol glyphs egui uses are not in the bundled font.
                    for (value, label) in [
                        (egui::ThemePreference::System, "Auto"),
                        (egui::ThemePreference::Dark, "Dark"),
                        (egui::ThemePreference::Light, "Light"),
                    ] {
                        ui.selectable_value(&mut pref, value, label);
                    }
                    ui.ctx().set_theme(pref);
                });
            });
        });
        if self.show_problems && !self.problems.is_empty() {
            let mut jump: Option<(Table, Option<u16>)> = None;
            let mut fix_weather = false;
            let weather = self
                .doc
                .as_ref()
                .map_or(0, |d| d.cp.channels.iter().filter(|c| freqs::is_weather(c.rx_hz) && !c.rx_only).count());
            egui::Panel::bottom("problems")
                .resizable(true)
                .default_size(140.0)
                .show(ui, |ui| {
                    let p = theme::palette(ui);
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Problems").strong().color(p.accent));
                        if weather > 0
                            && ui
                                .button(format!("Set RX only on {weather} weather channel(s)"))
                                .on_hover_text("North American weather frequencies: the channel keeps its frequencies and can no longer transmit")
                                .clicked()
                        {
                            fix_weather = true;
                        }
                    });
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            for pr in &self.problems {
                                let (mark, colour) = if pr.level == Level::Error {
                                    ("error", p.bad)
                                } else {
                                    ("warning", p.warn)
                                };
                                let place = match (pr.table, pr.number) {
                                    (Table::General, _) => "General".to_string(),
                                    (t, Some(n)) => format!(
                                        "{} {n}{}",
                                        problem_table(t),
                                        if pr.item.is_empty() {
                                            String::new()
                                        } else {
                                            format!(" {}", pr.item)
                                        }
                                    ),
                                    (t, None) => problem_table(t).to_string(),
                                };
                                ui.horizontal(|ui| {
                                    ui.colored_label(colour, mark);
                                    if ui.link(format!("{place}: {}", pr.message)).clicked() {
                                        jump = Some((pr.table, pr.number));
                                    }
                                });
                            }
                        });
                });
            if fix_weather {
                if let Some(d) = &mut self.doc {
                    let before = d.cp.clone();
                    let n = freqs::set_weather_rx_only(&mut d.cp);
                    d.note(before, None);
                    self.status = format!("Set RX only on {n} weather channel(s).");
                    // The toolbar was drawn before this edit: draw again so Write to radio lights up at once.
                    ui.ctx().request_repaint();
                }
            }
            if let Some((t, n)) = jump {
                match t {
                    Table::Channels => {
                        self.tab = Tab::Channels;
                        self.channel = n;
                    }
                    Table::Zones => {
                        self.tab = Tab::Zones;
                        self.zone = n.map_or(0, |n| usize::from(n).saturating_sub(1));
                    }
                    Table::Contacts => {
                        self.tab = Tab::Contacts;
                        self.contact = n;
                    }
                    Table::General => self.tab = Tab::General,
                    Table::Systems => {
                        self.tab = Tab::Systems;
                        self.system = n;
                    }
                    Table::Dtmf => {
                        self.tab = Tab::Dtmf;
                        self.dtmf = n.map(|n| n as u8);
                    }
                    Table::Aprs => {
                        self.tab = Tab::Aprs;
                        self.aprs = n.map(|n| n as u8);
                    }
                    Table::RxLists => {
                        self.tab = Tab::Lists;
                        self.list = self
                            .doc
                            .as_ref()
                            .and_then(|d| {
                                d.cp.rx_lists
                                    .iter()
                                    .position(|l| Some(u16::from(l.number)) == n)
                            })
                            .unwrap_or(0);
                    }
                }
            }
        }
        egui::CentralPanel::default().show(ui, |ui| {
            if self.doc.is_none() {
                self.welcome(ui);
                return;
            }
            let Some(doc) = &mut self.doc else { return };
            ui.horizontal(|ui| {
                let cp = &doc.cp;
                ui.selectable_value(
                    &mut self.tab,
                    Tab::Channels,
                    format!("Channels  {}", cp.channels.len()),
                );
                ui.selectable_value(&mut self.tab, Tab::Zones, format!("Zones  {}", cp.zones.len()));
                ui.selectable_value(
                    &mut self.tab,
                    Tab::Contacts,
                    format!("Contacts  {}", cp.contacts.len()),
                );
                ui.selectable_value(
                    &mut self.tab,
                    Tab::IdDb,
                    format!("ID Database{}", cp.iddb.as_ref().map_or(String::new(), |h| format!("  {}", h.count))),
                );
                ui.selectable_value(
                    &mut self.tab,
                    Tab::Lists,
                    format!("RX Group Lists  {}", cp.rx_lists.len()),
                );
                ui.selectable_value(
                    &mut self.tab,
                    Tab::Dtmf,
                    format!("DTMF  {}", cp.dtmf_contacts.len()),
                );
                ui.selectable_value(&mut self.tab, Tab::Aprs, format!("APRS  {}", cp.aprs.len()));
                ui.selectable_value(
                    &mut self.tab,
                    Tab::Systems,
                    format!("Systems  {}", cp.systems.len()),
                );
                ui.selectable_value(&mut self.tab, Tab::General, "General");
            });
            ui.separator();
            let before = doc.cp.clone();
            let gen_before = doc.generation;
            match self.tab {
                Tab::Channels => channels::show(ui, &mut doc.cp, &mut self.channel),
                Tab::Zones => zones::show(ui, &mut doc.cp, &mut self.zone),
                Tab::Contacts => {
                    if contacts::show(ui, &mut doc.cp, &mut self.contact) {
                        self.tab = Tab::IdDb;
                        self.iddb.show_actions();
                    }
                }
                Tab::IdDb => iddbtab::show(ui, doc, &mut self.iddb, self.radio_facts),
                Tab::Lists => lists::show(ui, &mut doc.cp, &mut self.list),
                Tab::Dtmf => dtmf::show(ui, &mut doc.cp, &mut self.dtmf),
                Tab::Aprs => aprs::show(ui, &mut doc.cp, &mut self.aprs),
                Tab::Systems => systems::show(ui, &mut doc.cp, &mut self.system),
                Tab::General => {
                    let newer = rfcps_core::kv::decode_area(doc.base.bytes()).state
                        == rfcps_core::kv::AreaState::Newer;
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| general::show(ui, &mut doc.cp, newer));
                }
            }
            let focus = ui.memory(|m| m.focused());
            let generation = gen_before;
            // a change of the ID database made its own history step
            if doc.generation == gen_before {
                doc.note(before, focus);
            }
            if doc.generation != generation {
                // Same reason: the toolbar above shows the state from before this edit.
                ui.ctx().request_repaint();
            }
        });
    }
}

/// Whether to run on X11 although a Wayland session is there: an X server (`DISPLAY`, XWayland on a Wayland desktop) is
/// reachable, the person did not ask for Wayland (`RFCPS_WAYLAND`), and there is a Wayland session to step around.
#[cfg(any(target_os = "linux", test))]
fn prefer_x11(env: impl Fn(&str) -> Option<std::ffi::OsString>) -> bool {
    let set = |k: &str| env(k).is_some_and(|v| !v.is_empty());
    set("DISPLAY") && !set("RFCPS_WAYLAND") && (set("WAYLAND_DISPLAY") || set("WAYLAND_SOCKET"))
}

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(code) = cli::run(&args) {
        std::process::exit(code);
    }
    // On Linux, use the X11 window system (through XWayland on a Wayland desktop): the desktop then draws the normal
    // title bar and buttons, where a Wayland window gets a plain fallback frame. winit no longer reads
    // WINIT_UNIX_BACKEND (removed in 0.29): it takes Wayland when WAYLAND_DISPLAY is set, so hide that from this
    // process. RFCPS_WAYLAND=1 keeps Wayland.
    #[cfg(target_os = "linux")]
    if prefer_x11(|k| std::env::var_os(k)) {
        std::env::remove_var("WAYLAND_DISPLAY");
        std::env::remove_var("WAYLAND_SOCKET");
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1240.0, 680.0])
            .with_title("RadioFoundry CPS")
            .with_app_id("rfcps")
            .with_icon(
                eframe::icon_data::from_png_bytes(include_bytes!("../../../packaging/rfcps.png"))
                    .expect("bundled icon"),
            ),
        ..Default::default()
    };
    eframe::run_native(
        "RadioFoundry CPS",
        options,
        Box::new(|cc| {
            theme::apply(&cc.egui_ctx);
            let mut app = App::new();
            app.iddb.keep_choices_in(worker::data_dir().join("id-database-settings.json"));
            // `rfcps file.bin` opens that image at start.
            if let Some(path) = std::env::args_os().nth(1) {
                match std::fs::read(&path) {
                    Ok(b) => {
                        app.open_image(&b, path.to_string_lossy().into_owned());
                        if let Some(d) = &mut app.doc {
                            d.from_file = true;
                        }
                    }
                    Err(e) => app.status = format!("Could not read {}: {e}", path.to_string_lossy()),
                }
            }
            Ok(Box::new(app))
        }),
    )
}

fn problem_table(t: Table) -> &'static str {
    match t {
        Table::General => "General",
        Table::Channels => "Channel",
        Table::Zones => "Zone",
        Table::Contacts => "Contact",
        Table::RxLists => "RX group list",
        Table::Systems => "System",
        Table::Dtmf => "DTMF contact",
        Table::Aprs => "APRS configuration",
    }
}

#[cfg(test)]
mod display_tests {
    use super::prefer_x11;
    use std::ffi::OsString;

    fn with(vars: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<OsString> {
        move |k| vars.iter().find(|(n, _)| *n == k).map(|(_, v)| OsString::from(v))
    }

    #[test]
    fn a_wayland_desktop_with_xwayland_runs_on_x11() {
        assert!(prefer_x11(with(&[("DISPLAY", ":0"), ("WAYLAND_DISPLAY", "wayland-0")])));
        assert!(prefer_x11(with(&[("DISPLAY", ":1"), ("WAYLAND_SOCKET", "3")])));
    }

    #[test]
    fn nothing_changes_without_xwayland_without_wayland_or_when_wayland_is_asked_for() {
        assert!(!prefer_x11(with(&[("WAYLAND_DISPLAY", "wayland-0")])), "no X server to fall back on");
        assert!(!prefer_x11(with(&[("DISPLAY", ":0")])), "an X11 session already");
        assert!(!prefer_x11(with(&[("DISPLAY", ":0"), ("WAYLAND_DISPLAY", "wayland-0"), ("RFCPS_WAYLAND", "1")])));
        assert!(!prefer_x11(with(&[("DISPLAY", ""), ("WAYLAND_DISPLAY", "wayland-0")])), "an empty DISPLAY is none");
        assert!(!prefer_x11(with(&[])));
    }
}
