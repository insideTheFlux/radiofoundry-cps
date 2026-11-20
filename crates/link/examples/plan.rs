// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux

//! `cargo run -p rfcps-link --example plan -- /dev/ttyUSB0` reads a radio and shows what a write would send, for
//! an unedited codeplug and for one with a channel renamed. It writes nothing.

use rfcps_core::Codeplug;

fn main() {
    let port = std::env::args().nth(1).expect("usage: plan PORT");
    let mut radio = rfcps_link::open(&port).unwrap_or_else(|e| die(&e));
    radio.ping().unwrap_or_else(|e| die(&e));
    let info = radio.codeplug_info().unwrap_or_else(|e| die(&e));
    println!(
        "staged writes: {}, flash writable: {}",
        info.staged_writes, info.flash_writable
    );
    let base = radio.read_image(&mut |_, _| {}).unwrap_or_else(|e| die(&e));

    let cp = Codeplug::decode(&base);
    let same = cp.encode(&base);
    let jobs = radio.plan_write(&same).unwrap_or_else(|e| die(&e));
    println!("unedited: {} blocks to write", jobs.len());
    let changed = radio.verify(&base).unwrap_or_else(|e| die(&e));
    println!(
        "radio differs from the image just read in {} blocks",
        changed.len()
    );

    let mut edited = Codeplug::decode(&base);
    if let Some(c) = edited.channels.first_mut() {
        c.name = format!("{}x", c.name.chars().take(15).collect::<String>());
    }
    let jobs = radio
        .plan_write(&edited.encode(&base))
        .unwrap_or_else(|e| die(&e));
    let at: Vec<String> = jobs.iter().map(|(a, _)| format!("{a:#x}")).collect();
    println!(
        "channel 1 renamed: {} block(s) to write: {}",
        jobs.len(),
        at.join(", ")
    );
    println!("nothing was written");
}

fn die(e: &dyn std::error::Error) -> ! {
    eprintln!("error: {e}");
    std::process::exit(1);
}
