// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux

//! `iddb_check PORT USERS.csv [PREFIX]` (read only): asks the radio's own lookup for a sample of the users with that call
//! sign prefix and compares its answers with the text the editor builds ("{call} {first} {city} {st}", 32 characters).

use rfcps_core::{iddb, radioid};

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let [port, csv, rest @ ..] = &a[..] else {
        eprintln!("usage: iddb_check PORT USERS.csv [PREFIX]");
        std::process::exit(2);
    };
    let prefix = rest.first().cloned().unwrap_or_else(|| "K6".into());
    let mut radio = rfcps_link::open(port).unwrap_or_else(|e| die(&e));
    radio.ping().unwrap_or_else(|e| die(&e));
    let head = radio.iddb_lookup(0).unwrap_or_else(|e| die(&e));
    println!("the radio says: format {:?}, {} records", char::from(head.format), head.records);
    let users = radioid::parse_users(&std::fs::read_to_string(csv).unwrap());
    let sel = radioid::select(&users, &radioid::Selection { prefixes: vec![prefix], ..Default::default() });
    let (mut ok, mut bad, mut checked) = (0, 0, 0);
    for u in sel.iter().step_by((sel.len() / 40).max(1)) {
        let want = iddb::clean(&radioid::format(u, "{call} {first} {city} {st}", false, 32));
        let got = radio.iddb_lookup(u.id).unwrap_or_else(|e| die(&e));
        checked += 1;
        if got.found && got.text.trim() == want.trim() {
            ok += 1;
        } else {
            bad += 1;
            println!("ID {}: found {} {:?}, expected {:?}", u.id, got.found, got.text, want);
        }
    }
    let miss = radio.iddb_lookup(1).unwrap_or_else(|e| die(&e));
    println!("{ok} of {checked} sampled names agree ({bad} differ); ID 1 {}", if miss.found { "FOUND (unexpected)" } else { "not found, as expected" });
    if let Some(u) = sel.first() {
        let g = radio.iddb_lookup(u.id).unwrap_or_else(|e| die(&e));
        println!("first user {} -> {:?}", u.id, g.text);
    }
}

fn die(e: &dyn std::error::Error) -> ! {
    eprintln!("error: {e}");
    std::process::exit(1);
}
