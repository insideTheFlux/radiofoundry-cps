// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux

//! `iddb_test PORT BACKUP_DIR USERS.csv` is a WRITE test of the DMR ID database on a connected radio, undone at the end:
//! it saves the codeplug and every database block it will touch, writes a small database (format C, then format N), checks
//! each with a read-back and the radio's own lookup, and puts the saved blocks back and checks they are back.
//! Only run it on a radio you can restore, with a backup you have looked at.

use rfcps_core::iddb::{self, Options};
use rfcps_core::radioid;
use rfcps_core::Image;
use rfcps_link::{Block, Radio};
use std::io::{Read, Write};

const BLOCK: usize = 4096;

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let (port, dir, csv) = match &a[..] {
        [p, d, c, ..] => (p, d, c),
        _ => {
            eprintln!("usage: iddb_test PORT BACKUP_DIR USERS.csv [RECORDS [PREFIX]]");
            std::process::exit(2);
        }
    };
    let limit: usize = a.get(3).map_or(3000, |s| s.parse().expect("RECORDS"));
    let prefixes: Vec<String> = a.get(4).map(|p| vec![p.clone()]).unwrap_or_else(|| vec!["K6".into()]);
    let mut radio = rfcps_link::open(port).unwrap_or_else(|e| die(&e));
    radio.ping().unwrap_or_else(|e| die(&e));
    let info = radio.checked_info().unwrap_or_else(|e| die(&e));
    let features = radio.features().unwrap_or_else(|e| die(&e));
    assert!(features.iter().any(|f| f == "IDDB_COMPACT"), "this radio does not read format C: stopping");
    let before = radio.iddb_lookup(0).unwrap_or_else(|e| die(&e));
    println!("before: format {:?}, {} records", char::from(before.format), before.records);

    // the records: real users, a few thousand
    let users = radioid::parse_users(&std::fs::read_to_string(csv).unwrap());
    let sel = radioid::select(&users, &radioid::Selection { prefixes, ..Default::default() });
    let sel: Vec<&radioid::User> = sel.into_iter().take(limit).collect();
    let recs = radioid::iddb_records(&sel, "{call} {first} {city} {st}", false, 32);
    println!("{} records from {}", recs.len(), csv);

    // 1. the backups: the whole codeplug, then the database blocks every step could touch
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let cp = radio.read_image(&mut |_, _| {}).unwrap_or_else(|e| die(&e));
    let cp_file = format!("{dir}/iddb-test-{stamp}-codeplug.bin");
    std::fs::write(&cp_file, cp.bytes()).unwrap();
    println!("codeplug saved: {cp_file}");
    let mut c_img = Image::blank();
    let wc = iddb::write(&mut c_img, &recs, Options { format: b'C', ..Default::default() }).unwrap();
    let mut n_img = Image::blank();
    let wn = iddb::write(&mut n_img, &recs, Options { format: b'N', text_len: 32, ..Default::default() }).unwrap();
    let mut blocks: Vec<usize> = Vec::new();
    for img in [&c_img, &n_img] {
        for (s, e) in iddb::ranges(img.bytes()) {
            blocks.extend((s / BLOCK * BLOCK..e).step_by(BLOCK));
        }
    }
    blocks.sort_unstable();
    blocks.dedup();
    println!("blocks that may be touched: {} (C reaches {:#x}, N reaches {:#x})", blocks.len(), wc.top, wn.top);
    let mut saved: Vec<(usize, Vec<u8>)> = Vec::new();
    for &a in &blocks {
        let d = match radio.codeplug_read_block(a as u32).unwrap_or_else(|e| die(&e)) {
            Block::Blank => vec![0xFF; BLOCK],
            Block::Data(d) => d,
        };
        saved.push((a, d));
    }
    let mut all = Vec::new();
    for (a, d) in &saved {
        all.extend_from_slice(&(*a as u32).to_le_bytes());
        all.extend_from_slice(d);
    }
    let blk_file = format!("{dir}/iddb-test-{stamp}-blocks.bin");
    std::fs::write(&blk_file, &all).unwrap();
    println!("blocks saved: {blk_file} ({} bytes: address u32 then 4096 bytes, repeated)", all.len());

    // 2. format C, then 3. format N, each checked; whatever happens, the blocks go back
    let result = (|| -> Result<(), String> {
        for (name, img, w) in [("C", &c_img, &wc), ("N", &n_img, &wn)] {
            let rep = radio.write_iddb(img, &mut |_, _| {}).map_err(|e| format!("write {name}: {e}"))?;
            println!("{name}: wrote {} blocks, {} stream bytes, {} records, dropped {}", rep.blocks, rep.stream_bytes, w.count, w.dropped);
            let a = radio.iddb_lookup(0).map_err(|e| e.to_string())?;
            println!("{name}: the radio says format {:?}, {} records", char::from(a.format), a.records);
            if a.format != name.as_bytes()[0] || a.records as usize != w.count {
                return Err(format!("{name}: the radio's own count or format differs"));
            }
            for r in recs.iter().step_by((recs.len() / 25).max(1)) {
                let l = radio.iddb_lookup(r.id).map_err(|e| e.to_string())?;
                let want = iddb::clean(&r.text);
                if !l.found || l.text.trim() != want.trim() {
                    return Err(format!("{name}: ID {} gave found {} {:?}, expected {:?}", r.id, l.found, l.text, want));
                }
            }
            let miss = radio.iddb_lookup(1).map_err(|e| e.to_string())?;
            if miss.found {
                return Err(format!("{name}: ID 1 should not be found"));
            }
            println!("{name}: 25 names and a miss agree with the radio's own lookup");
        }
        Ok(())
    })();
    match &result {
        Ok(()) => println!("the tests passed"),
        Err(e) => println!("FAILED: {e}"),
    }

    // 4. put every saved block back, in the order a database write should go (header's block last), and check
    let _ = radio.codeplug_abort(); // a session a failed write left open
    println!("restoring {} blocks…", saved.len());
    restore(&mut radio, &saved, u32::from(info.data_per_write)).unwrap_or_else(|e| die(&e));
    let mut bad = 0;
    for (a, d) in &saved {
        let now = match radio.codeplug_read_block(*a as u32).unwrap_or_else(|e| die(&e)) {
            Block::Blank => vec![0xFF; BLOCK],
            Block::Data(x) => x,
        };
        if &now != d {
            bad += 1;
        }
    }
    let after = radio.iddb_lookup(0).unwrap_or_else(|e| die(&e));
    println!("after restore: {bad} blocks differ; the radio says format {:?}, {} records", char::from(after.format), after.records);
    let cp2 = radio.read_image(&mut |_, _| {}).unwrap_or_else(|e| die(&e));
    println!("codeplug blocks the editor reads: {}", if cp2.bytes()[..iddb::AREA1] == cp.bytes()[..iddb::AREA1] { "identical to before" } else { "DIFFER" });
    if bad > 0 || result.is_err() {
        std::process::exit(1);
    }
}

fn restore<P: Read + Write>(radio: &mut Radio<P>, saved: &[(usize, Vec<u8>)], per: u32) -> Result<(), rfcps_link::LinkError> {
    radio.codeplug_begin(false)?;
    for (a, d) in saved.iter().rev() {
        radio.codeplug_write_block(*a as u32, d, per as usize)?;
    }
    radio.codeplug_commit(std::time::Duration::from_secs(20))
}

fn die(e: &dyn std::error::Error) -> ! {
    eprintln!("error: {e}");
    std::process::exit(1);
}
