// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux

//! `cargo run -p rfcps-core --example dump -- IMAGE.bin` prints what the image holds.

use rfcps_core::{records::Mode, Codeplug, Image};

fn main() {
    let path = std::env::args().nth(1).expect("usage: dump IMAGE.bin");
    let cp = Codeplug::decode(&Image::from_bytes(
        &std::fs::read(path).expect("read image"),
    ));
    println!(
        "{} channels, {} zones, {} contacts",
        cp.channels.len(),
        cp.zones.len(),
        cp.contacts.len()
    );
    println!("\nzones:");
    for z in &cp.zones {
        println!("  {:<18} {} channels", z.name, z.channels.len());
    }
    println!("\n  #  name              rx MHz     tx MHz    mode  rx tone  tx tone");
    for c in &cp.channels {
        println!(
            "{:>3}  {:<16} {:>9.4}  {:>9.4}  {:<4}  {:<7}  {}",
            c.number,
            c.name,
            c.rx_hz as f64 / 1e6,
            c.tx_hz as f64 / 1e6,
            if c.mode == Mode::Dmr { "DMR" } else { "FM" },
            c.rx_tone,
            c.tx_tone
        );
    }
}
