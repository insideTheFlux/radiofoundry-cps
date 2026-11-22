// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux

//! `sigtone_test PORT BACKUP_DIR show|apply|defaults|remove` is a WRITE test of the three signaling tone records in the
//! settings area (codeplug-settings.md 3.3), in steps the owner watches on the radio's menu:
//!
//! - `show`: read only; prints the three records the codeplug holds.
//! - `apply`: saves the codeplug, then writes a start, end and idle tone that sound on the SPEAKER only (output 2): nothing
//!   goes on air, whatever the radio does with them.
//! - `defaults`: writes the three records with the radio's own sound (output 0, no chain), which puts the radio's copy back.
//! - `remove`: takes the three keys out again, so the area is as it was. (A removed key leaves the radio's own copy at its last
//!   value, which is why `defaults` comes first.)

use rfcps_core::kv::SigRecord;
use rfcps_core::Codeplug;

fn rec(dmr: u8, fm: u8, steps: &[(u16, u16)]) -> SigRecord {
    let mut r = SigRecord { dmr, fm, ..SigRecord::default() };
    for (i, &(hz, ms)) in steps.iter().enumerate() {
        r.hz[i] = hz;
        r.ms[i] = ms;
    }
    r
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let (port, dir, step) = match &a[..] {
        [p, d, s] if ["show", "apply", "defaults", "remove"].contains(&s.as_str()) => (p, std::path::Path::new(d), s.as_str()),
        _ => {
            eprintln!("usage: sigtone_test PORT BACKUP_DIR show|apply|defaults|remove");
            std::process::exit(2);
        }
    };
    let mut radio = rfcps_link::open(port).unwrap_or_else(|e| die(&e));
    radio.ping().unwrap_or_else(|e| die(&e));
    let base = radio.read_image(&mut |_, _| {}).unwrap_or_else(|e| die(&e));
    let mut cp = Codeplug::decode(&base);
    let show = |cp: &Codeplug| {
        for (i, n) in ["start", "end", "idle"].iter().enumerate() {
            println!("{n} tone: {:?}", cp.general.keys.sig[i]);
        }
    };
    println!("the codeplug holds:");
    show(&cp);
    if step == "show" {
        return;
    }
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let backup = dir.join(format!("sigtone-before-{step}-{secs}.bin"));
    std::fs::write(&backup, base.bytes()).unwrap_or_else(|e| die(&e));
    println!("backup: {}", backup.display());
    // outputs: 2 is the speaker, so nothing is sent on air; steps are (Hz, ms), Hz 0 a pause
    let want: [Option<SigRecord>; 3] = match step {
        "apply" => [
            Some(rec(2, 2, &[(1000, 100), (0, 50), (1500, 100)])),
            Some(rec(2, 2, &[(800, 200)])),
            Some(rec(2, 2, &[(600, 100)])),
        ],
        "defaults" => [Some(rec(0, 0, &[])), Some(rec(0, 0, &[])), Some(rec(0, 0, &[]))],
        _ => [None, None, None],
    };
    cp.general.keys.sig = want.clone();
    let edited = cp.encode(&base);
    let report = radio.write_image(&base, &edited, &mut |d, t| println!("  block {d}/{t}")).unwrap_or_else(|e| die(&e));
    println!("wrote {} block(s), read back and verified", report.blocks);
    let now = Codeplug::decode(&radio.read_image(&mut |_, _| {}).unwrap_or_else(|e| die(&e)));
    println!("the radio's codeplug now holds:");
    show(&now);
    if now.general.keys.sig != want {
        println!("SOMETHING DIFFERS from what was written");
        std::process::exit(1);
    }
    println!("read back: as written");
}

fn die(e: &dyn std::fmt::Display) -> ! {
    eprintln!("error: {e}");
    std::process::exit(1);
}
