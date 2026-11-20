// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux

//! `cargo run -p rfcps-link --example read -- /dev/ttyUSB0 image.bin` reads a radio's codeplug (read only).

fn main() {
    let mut args = std::env::args().skip(1);
    let (Some(port), Some(out)) = (args.next(), args.next()) else {
        eprintln!(
            "usage: read PORT OUT.bin   (ports: {:?})",
            rfcps_link::port_names()
        );
        std::process::exit(2);
    };
    let mut radio = rfcps_link::open(&port).unwrap_or_else(|e| die(&e));
    radio.ping().unwrap_or_else(|e| die(&e));
    let info = radio.codeplug_info().unwrap_or_else(|e| die(&e));
    println!("{info:?}");
    let t = std::time::Instant::now();
    let image = radio
        .read_image(&mut |done, total| {
            if done % 8 == 0 || done == total {
                eprint!("\r{done}/{total} blocks");
            }
        })
        .unwrap_or_else(|e| die(&e));
    eprintln!();
    std::fs::write(&out, image.bytes()).unwrap_or_else(|e| die(&e));
    println!(
        "read {} bytes in {:.1} s",
        image.len(),
        t.elapsed().as_secs_f32()
    );
}

fn die(e: &dyn std::error::Error) -> ! {
    eprintln!("error: {e}");
    std::process::exit(1);
}
