// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux

//! `cargo run -p rfcps-core --example problems -- IMAGE.bin` lists what is wrong or doubtful in an image.

use rfcps_core::problems::{validate, Level};
use rfcps_core::{Codeplug, Image};

fn main() {
    let path = std::env::args().nth(1).expect("usage: problems IMAGE.bin");
    let cp = Codeplug::decode(&Image::from_bytes(
        &std::fs::read(path).expect("read image"),
    ));
    let problems = validate(&cp, None);
    for p in &problems {
        println!(
            "{:<8} {:?} {:>4} {:<18} {}",
            if p.level == Level::Error {
                "error"
            } else {
                "warning"
            },
            p.table,
            p.number.map_or(String::new(), |n| n.to_string()),
            p.item,
            p.message
        );
    }
    println!("{} problems", problems.len());
}
