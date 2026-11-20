// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux

//! Lists the serial ports, USB ones first.

fn main() {
    for p in rfcps_link::ports() {
        println!(
            "{:<18} {:<5} {}",
            p.name,
            if p.usb { "USB" } else { "" },
            p.description
        );
    }
}
