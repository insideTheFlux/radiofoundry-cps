// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui;

struct App {
    ports: Vec<String>,
}

impl App {
    fn new() -> Self {
        Self { ports: rfcps_link::port_names() }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ui, |ui| {
            ui.heading("RadioFoundry CPS");
            ui.label("Codeplug editor (early scaffold, nothing reads a radio yet).");
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("Serial ports:");
                if ui.button("Refresh").clicked() {
                    self.ports = rfcps_link::port_names();
                }
            });
            if self.ports.is_empty() {
                ui.label("none found");
            }
            for p in &self.ports {
                ui.monospace(p);
            }
        });
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([720.0, 480.0]),
        ..Default::default()
    };
    eframe::run_native("RadioFoundry CPS", options, Box::new(|_cc| Ok(Box::new(App::new()))))
}
