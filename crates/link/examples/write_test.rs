// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux

//! `write_test PORT rename|revert BACKUP_DIR`: the first real write. Reads the radio, saves what it read as a backup,
//! then adds an "x" to channel 3's name (`rename`) or takes it off again (`revert`), writes that one change, and has the
//! radio read back. Run it only with the owner's go-ahead.

use rfcps_core::Codeplug;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.len() != 4 || !["rename", "revert"].contains(&a[2].as_str()) {
        eprintln!("usage: write_test PORT rename|revert BACKUP_DIR");
        std::process::exit(2);
    }
    let (port, mode, dir) = (&a[1], a[2].as_str(), std::path::Path::new(&a[3]));
    let mut radio = rfcps_link::open(port).unwrap_or_else(|e| die(&e));
    radio.ping().unwrap_or_else(|e| die(&e));
    let base = radio.read_image(&mut |_, _| {}).unwrap_or_else(|e| die(&e));

    std::fs::create_dir_all(dir).unwrap_or_else(|e| die(&e));
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let backup = dir.join(format!("codeplug-before-{mode}-{secs}.bin"));
    std::fs::write(&backup, base.bytes()).unwrap_or_else(|e| die(&e));
    println!("backup of the radio's codeplug: {}", backup.display());

    let mut cp = Codeplug::decode(&base);
    let c = cp
        .channels
        .iter_mut()
        .find(|c| c.number == 3)
        .unwrap_or_else(|| die(&"no channel 3"));
    println!("channel 3 is \"{}\"", c.name);
    match mode {
        "rename" if c.name.chars().count() < 16 && !c.name.ends_with('x') => c.name.push('x'),
        "revert" if c.name.ends_with('x') => {
            c.name.pop();
        }
        _ => die(&format!("nothing to {mode}: channel 3 is \"{}\"", c.name)),
    }
    println!("changing it to \"{}\"", c.name);
    let edited = cp.encode(&base);
    let jobs = radio.plan_write(&edited).unwrap_or_else(|e| die(&e));
    println!(
        "blocks to write: {:?}",
        jobs.iter()
            .map(|(a, _)| format!("{a:#x}"))
            .collect::<Vec<_>>()
    );
    let report = radio
        .write_image(&base, &edited, &mut |d, t| println!("  block {d}/{t}"))
        .unwrap_or_else(|e| die(&e));
    println!(
        "wrote {} block(s), {} stream bytes; read back and verified",
        report.blocks, report.stream_bytes
    );
}

fn die(e: &dyn std::fmt::Display) -> ! {
    eprintln!("error: {e}");
    std::process::exit(1);
}
