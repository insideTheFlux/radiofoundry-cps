// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux

//! Serial work on a thread of its own, so the window stays alive while a radio is read.

use eframe::egui;
use rfcps_core::Image;
use std::sync::mpsc::{channel, Receiver};

pub enum Msg {
    Progress(usize, usize),
    Done(Result<Image, String>),
}

/// Reads a radio's codeplug on a new thread. The receiver gets progress and then the result.
pub fn read_radio(port: String, ctx: egui::Context) -> Receiver<Msg> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let result = (|| {
            let mut radio = rfcps_link::open(&port).map_err(|e| e.to_string())?;
            radio.ping().map_err(|e| {
                format!("{e} (is it a RadioFoundry radio, switched on, on this port?)")
            })?;
            radio
                .read_image(&mut |done, total| {
                    let _ = tx.send(Msg::Progress(done, total));
                    ctx.request_repaint();
                })
                .map_err(|e| e.to_string())
        })();
        let _ = tx.send(Msg::Done(result));
        ctx.request_repaint();
    });
    rx
}
