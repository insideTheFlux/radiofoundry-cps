// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux

//! `rssi_scan PORT SECONDS` (read only): while a P25 trunked channel hunts, polls the radio's core view (RSSI) and the
//! follower's view (which control channel it is on) and prints the strongest and the typical RSSI per frequency.

use std::collections::BTreeMap;

fn kv(text: &str) -> BTreeMap<String, String> {
    text.split_whitespace().filter_map(|t| t.split_once('=')).map(|(k, v)| (k.to_string(), v.to_string())).collect()
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let [port, secs] = &a[..] else {
        eprintln!("usage: rssi_scan PORT SECONDS");
        std::process::exit(2);
    };
    let secs: u64 = secs.parse().expect("SECONDS");
    let mut radio = rfcps_link::open(port).unwrap_or_else(|e| die(&e));
    radio.ping().unwrap_or_else(|e| die(&e));
    let mut per: BTreeMap<u64, Vec<i64>> = BTreeMap::new();
    let t0 = std::time::Instant::now();
    let mut samples = 0;
    while t0.elapsed().as_secs() < secs {
        let f = kv(&radio.view(37).unwrap_or_else(|e| die(&e)));
        let c = kv(&radio.view(0).unwrap_or_else(|e| die(&e)));
        let hz: u64 = f.get("p25t.cc_hz").and_then(|x| x.parse().ok()).unwrap_or(0);
        let rx: u64 = c.get("rx_hz").and_then(|x| x.parse().ok()).unwrap_or(0);
        if let Some(r) = c.get("rssi_dbm").and_then(|x| x.parse::<i64>().ok()) {
            per.entry(if hz != 0 { hz } else { rx }).or_default().push(r);
            samples += 1;
        }
    }
    println!("{samples} samples; the radio's receive frequency and its RSSI (dBm) there:");
    for (hz, v) in &per {
        let mut s = v.clone();
        s.sort_unstable();
        println!("  {:>11.4} MHz: n={:<4} min {:>4}  median {:>4}  max {:>4}", *hz as f64 / 1e6, s.len(), s[0], s[s.len() / 2], s[s.len() - 1]);
    }
}

fn die(e: &dyn std::fmt::Display) -> ! {
    eprintln!("error: {e}");
    std::process::exit(1);
}
