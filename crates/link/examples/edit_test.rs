// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux

//! `edit_test PORT BACKUP_DIR apply|delete|restore [BACKUP.bin]` is a WRITE test of the table edits on a connected
//! radio, in three steps the owner watches:
//!
//! - `apply`: saves the radio's codeplug, then adds one test channel, zone, group contact, private contact and RX group
//!   list (all named "ZZ TEST ..."), and changes boot text line 2, in one write, and checks them by reading back.
//! - `delete`: takes the test records away again (the bitmaps), in a second write, and checks.
//! - `restore BACKUP.bin`: writes the saved codeplug back and checks the radio holds it again.
//!
//! The test channel receives only. Run it only with the owner's go-ahead and a backup that has been looked at.

use rfcps_core::records::{CallType, Channel, Contact, RxList, Zone, MAX_CHANNELS, MAX_CONTACTS, MAX_RX_LISTS};
use rfcps_core::{Codeplug, Image};
use rfcps_link::{fake::FakeRadio, Radio};
use std::io::{Read, Write};
use std::path::Path;

const TAG: &str = "ZZ TEST";

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let (port, dir, step) = match &a[..] {
        [p, d, s, ..] if ["apply", "delete", "restore", "rehearse"].contains(&s.as_str()) => (p, std::path::Path::new(d), s.as_str()),
        _ => {
            eprintln!("usage: edit_test PORT BACKUP_DIR apply|delete|restore [BACKUP.bin]\n       edit_test fake:IMAGE.bin DIR rehearse   (all three steps on a software radio)");
            std::process::exit(2);
        }
    };
    if let Some(img) = port.strip_prefix("fake:") {
        let bytes = std::fs::read(img).unwrap_or_else(|e| die(&e));
        let mut radio = Radio::new(FakeRadio::new(Image::from_bytes(&bytes).bytes()));
        let backup = dir.join("rehearsal-before.bin");
        std::fs::write(&backup, Image::from_bytes(&bytes).bytes()).unwrap();
        let b = backup.display().to_string();
        for s in ["apply", "delete", "restore"] {
            println!("--- {s}");
            run(&mut radio, dir, s, &[String::new(), String::new(), String::new(), b.clone()]);
        }
        return;
    }
    let mut radio = rfcps_link::open(port).unwrap_or_else(|e| die(&e));
    radio.ping().unwrap_or_else(|e| die(&e));
    run(&mut radio, dir, step, &a);
}

fn run<P: Read + Write>(radio: &mut Radio<P>, dir: &Path, step: &str, a: &[String]) {
    let base = radio.read_image(&mut |_, _| {}).unwrap_or_else(|e| die(&e));
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let mut cp = Codeplug::decode(&base);

    let edited: Image = match step {
        "apply" => {
            if has_test(&cp) {
                die(&"the radio already holds ZZ TEST records: run delete or restore first");
            }
            let backup = dir.join(format!("edit-test-before-{secs}.bin"));
            std::fs::write(&backup, base.bytes()).unwrap_or_else(|e| die(&e));
            println!("backup of the radio's codeplug: {}", backup.display());
            let free = |used: Vec<usize>, max: usize| (1..=max).find(|n| !used.contains(n)).unwrap_or_else(|| die(&"no free number"));
            let ch = free(cp.channels.iter().map(|c| usize::from(c.number)).collect(), MAX_CHANNELS);
            let mut c = Channel::new(ch as u16, 146_520_000, 146_520_000);
            c.name = format!("{TAG} CH");
            c.rx_only = true;
            cp.channels.push(c);
            let free_c = |cp: &Codeplug| free(cp.contacts.iter().map(|c| usize::from(c.number)).collect(), MAX_CONTACTS) as u16;
            let tg = free_c(&cp);
            cp.contacts.push(Contact { number: tg, name: format!("{TAG} TG"), id: 9_999_001, call_type: CallType::Group, timeslot: 0 });
            let pc = free_c(&cp);
            cp.contacts.push(Contact { number: pc, name: format!("{TAG} PC"), id: 9_999_002, call_type: CallType::Private, timeslot: 0 });
            cp.zones.push(Zone { slot: cp.zones.len(), name: format!("{TAG} ZN"), channels: vec![ch as u16] });
            let ln = free(cp.rx_lists.iter().map(|l| usize::from(l.number)).collect(), MAX_RX_LISTS) as u8;
            cp.rx_lists.push(RxList { number: ln, name: format!("{TAG} RX"), contacts: vec![tg] });
            cp.general.boot_line2 = TAG.to_string();
            println!("adding channel {ch}, contacts {tg} and {pc}, a zone, RX list {ln}, and boot line 2 \"{TAG}\"");
            cp.encode(&base)
        }
        "delete" => {
            if !has_test(&cp) {
                die(&"no ZZ TEST records on the radio");
            }
            let gone: Vec<u16> = cp.channels.iter().filter(|c| c.name.starts_with(TAG)).map(|c| c.number).collect();
            let gone_contacts: Vec<u16> = cp.contacts.iter().filter(|c| c.name.starts_with(TAG)).map(|c| c.number).collect();
            cp.channels.retain(|c| !c.name.starts_with(TAG));
            for z in &mut cp.zones {
                z.channels.retain(|n| !gone.contains(n));
            }
            cp.zones.retain(|z| !z.name.starts_with(TAG));
            cp.contacts.retain(|c| !c.name.starts_with(TAG));
            cp.rx_lists.retain(|l| !l.name.starts_with(TAG));
            for l in &mut cp.rx_lists {
                l.contacts.retain(|n| !gone_contacts.contains(n));
            }
            println!("deleting the ZZ TEST channel, zone, contacts and RX list (boot line 2 stays until restore)");
            cp.encode(&base)
        }
        _ => {
            let Some(file) = a.get(3) else { die(&"restore needs the backup file") };
            let want = Image::from_bytes(&std::fs::read(file).unwrap_or_else(|e| die(&e)));
            println!("restoring {file}");
            want
        }
    };
    let to_send = radio.image_for_write(&base, &edited).unwrap_or_else(|e| die(&e));
    let jobs = radio.plan_write(&to_send).unwrap_or_else(|e| die(&e));
    println!("blocks to write: {:?}", jobs.iter().map(|(a, _)| format!("{a:#x}")).collect::<Vec<_>>());
    let report = radio.write_image(&base, &edited, &mut |d, t| println!("  block {d}/{t}")).unwrap_or_else(|e| die(&e));
    println!("wrote {} block(s), read back and verified", report.blocks);

    // read the radio again and say what it holds now
    let now = radio.read_image(&mut |_, _| {}).unwrap_or_else(|e| die(&e));
    let after = Codeplug::decode(&now);
    println!(
        "the radio now holds: ZZ TEST records {}, boot line 2 \"{}\", {} channels, {} zones, {} contacts, {} RX lists",
        if has_test(&after) { "PRESENT" } else { "absent" },
        after.general.boot_line2,
        after.channels.len(),
        after.zones.len(),
        after.contacts.len(),
        after.rx_lists.len()
    );
    if step == "apply" {
        let ok = after.channels.iter().any(|c| c.name == format!("{TAG} CH") && c.rx_only && c.rx_hz == 146_520_000)
            && after.zones.iter().any(|z| z.name == format!("{TAG} ZN") && z.channels.len() == 1)
            && after.contacts.iter().any(|c| c.name == format!("{TAG} TG") && c.id == 9_999_001)
            && after.contacts.iter().any(|c| c.name == format!("{TAG} PC") && c.call_type == CallType::Private)
            && after.rx_lists.iter().any(|l| l.name == format!("{TAG} RX") && l.contacts.len() == 1)
            && after.general.boot_line2 == TAG;
        println!("read back: {}", if ok { "every test record is there as written" } else { "SOMETHING DIFFERS" });
        if !ok {
            std::process::exit(1);
        }
    }
    if step == "delete" && has_test(&after) {
        println!("SOMETHING REMAINS");
        std::process::exit(1);
    }
    if step == "restore" {
        let want = Image::from_bytes(&std::fs::read(&a[3]).unwrap());
        let n = (0x80..0xB000).filter(|i| !(0x7590..0x75C8).contains(i)).filter(|i| !(0xE000..0xF000).contains(i)).filter(|&i| now.bytes()[i] != want.bytes()[i]).count();
        println!("bytes of the EEPROM regions (VFO A and the settings area aside) that differ from the backup: {n}");
        let flash: usize = (0x20000..0xB0000).filter(|&i| now.bytes()[i] != want.bytes()[i]).count();
        println!("bytes of the flash regions that differ from the backup: {flash}");
    }
}

fn has_test(cp: &Codeplug) -> bool {
    cp.channels.iter().any(|c| c.name.starts_with(TAG))
        || cp.zones.iter().any(|z| z.name.starts_with(TAG))
        || cp.contacts.iter().any(|c| c.name.starts_with(TAG))
        || cp.rx_lists.iter().any(|l| l.name.starts_with(TAG))
}

fn die(e: &dyn std::fmt::Display) -> ! {
    eprintln!("error: {e}");
    std::process::exit(1);
}
