// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod channels;
mod contacts;
mod doc;
mod exchange;
mod general;
mod lists;
mod theme;
mod widgets;
mod worker;
mod writeflow;
mod zones;

use doc::Doc;
use eframe::egui;
use rfcps_core::freqs;
use rfcps_core::problems::{self, Level, Problem, Table};
use rfcps_core::Image;
use rfcps_link::PortInfo;
use std::sync::mpsc::Receiver;
use worker::Msg;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Channels,
    Zones,
    Contacts,
    Lists,
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
    list: usize,
    flow: writeflow::Flow,
    problems: Vec<Problem>,
    problems_key: (u64, u64),
    doc_serial: u64,
    show_problems: bool,
    notice: Option<(String, Vec<String>)>,
    /// The green or red bar under the toolbar after a write; the x closes it.
    banner: Option<(bool, String)>,
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
            channel: None,
            contact: None,
            list: 0,
            flow: writeflow::Flow::new(),
            problems: Vec::new(),
            problems_key: (u64::MAX, u64::MAX),
            doc_serial: 0,
            show_problems: true,
            notice: None,
            banner: None,
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
        if let Some(p) = rfd::FileDialog::new()
            .add_filter("CHIRP CSV", &["csv"])
            .pick_file()
        {
            self.run_import("Import CHIRP CSV", "CHIRP import", |cp| {
                exchange::import_chirp_file(cp, &p)
            });
        }
    }

    fn import_csv_dialog(&mut self) {
        if let Some(p) = rfd::FileDialog::new()
            .add_filter("CSV tables", &["csv"])
            .pick_files()
        {
            self.run_import("Import CSV tables", "CSV import", |cp| {
                exchange::import_csv_files(cp, &p)
            });
        }
    }

    fn import_stock_dialog(&mut self) {
        if let Some(p) = rfd::FileDialog::new()
            .add_filter("Stock DM-32UV backup", &["zip", "bin"])
            .pick_file()
        {
            self.run_import(
                "Import stock DM-32UV codeplug",
                "stock DM-32UV import",
                |cp| exchange::import_stock_file(cp, &p),
            );
        }
    }

    fn export_csv_dialog(&mut self) {
        let (Some(doc), Some(dir)) = (
            &self.doc,
            rfd::FileDialog::new()
                .set_title("Choose a folder for the CSV files")
                .pick_folder(),
        ) else {
            return;
        };
        let r = exchange::export_csv_folder(&doc.cp, &dir);
        self.finish_export("Export CSV tables", r);
    }

    fn export_chirp_dialog(&mut self) {
        let (Some(doc), Some(p)) = (
            &self.doc,
            rfd::FileDialog::new()
                .add_filter("CHIRP CSV", &["csv"])
                .set_file_name("channels-chirp.csv")
                .save_file(),
        ) else {
            return;
        };
        let r = exchange::export_chirp_file(&doc.cp, &p);
        self.finish_export("Export CHIRP CSV", r);
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
                });
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
            let dirty = self.doc.as_mut().is_some_and(|d| d.dirty());
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
            ui.menu_button("Import", |ui| {
                if ui.button("CHIRP CSV…").clicked() {
                    ui.close();
                    self.import_chirp_dialog();
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
        self.poll_read();
        if let Some(d) = &self.doc {
            let key = (self.doc_serial, d.generation);
            if key != self.problems_key {
                self.problems = problems::validate(&d.cp, None);
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
                    if d.dirty() {
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
                    Tab::Lists,
                    format!("RX Group Lists  {}", cp.rx_lists.len()),
                );
                ui.selectable_value(&mut self.tab, Tab::General, "General");
            });
            ui.separator();
            let before = doc.cp.clone();
            match self.tab {
                Tab::Channels => channels::show(ui, &mut doc.cp, &mut self.channel),
                Tab::Zones => zones::show(ui, &mut doc.cp, &mut self.zone),
                Tab::Contacts => contacts::show(ui, &mut doc.cp, &mut self.contact),
                Tab::Lists => lists::show(ui, &mut doc.cp, &mut self.list),
                Tab::General => general::show(ui, &mut doc.cp),
            }
            let focus = ui.memory(|m| m.focused());
            let generation = doc.generation;
            doc.note(before, focus);
            if doc.generation != generation {
                // Same reason: the toolbar above shows the state from before this edit.
                ui.ctx().request_repaint();
            }
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
            // `rfcps file.bin` opens that image at start.
            if let Some(path) = std::env::args_os().nth(1) {
                match std::fs::read(&path) {
                    Ok(b) => app.open_image(&b, path.to_string_lossy().into_owned()),
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
    }
}
