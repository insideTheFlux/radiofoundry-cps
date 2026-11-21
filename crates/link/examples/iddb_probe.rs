// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux

//! `cargo run -p rfcps-link --example iddb_probe -- /dev/ttyUSB0` asks a radio about its DMR ID database (read only):
//! the layout it reports, its firmware's features, and the database header and the radio's own count.

use rfcps_core::iddb;

fn main() {
    let Some(port) = std::env::args().nth(1) else {
        eprintln!("usage: iddb_probe PORT   (ports: {:?})", rfcps_link::port_names());
        std::process::exit(2);
    };
    let mut radio = rfcps_link::open(&port).unwrap_or_else(|e| die(&e));
    radio.ping().unwrap_or_else(|e| die(&e));
    let info = radio.checked_info().unwrap_or_else(|e| die(&e));
    println!("INFO: {info:?}");
    let f = radio.features().unwrap_or_else(|e| die(&e));
    println!("features: {}", f.join(","));
    println!("reads compact: {}", f.iter().any(|x| x == "IDDB_COMPACT"));
    let a = radio.iddb_lookup(0).unwrap_or_else(|e| die(&e));
    println!("the radio's own answer: format {:?}, {} records", char::from(a.format), a.records);
    // the header's block
    let head = iddb::AREA1 / 4096 * 4096;
    let b = radio.codeplug_read_block(head as u32).unwrap_or_else(|e| die(&e));
    let mut img = rfcps_core::Image::from_bytes(&[]);
    if let rfcps_link::Block::Data(d) = b {
        img.bytes_mut()[head..head + 4096].copy_from_slice(&d);
    }
    println!("header on the radio: {:?}", iddb::read_header(img.bytes()));
}

fn die(e: &dyn std::error::Error) -> ! {
    eprintln!("error: {e}");
    std::process::exit(1);
}
