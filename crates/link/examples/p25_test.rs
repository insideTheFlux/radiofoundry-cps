// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux

//! `p25_test PORT BACKUP_DIR apply SITE:MHZ[,MHZ]...|zone|watch SECONDS|restore BACKUP.bin` follows a live P25 trunked
//! system, RECEIVE ONLY:
//!
//! - `apply SITE:MHZ[,MHZ]...`: saves the codeplug, then adds a LISTEN-ONLY P25 trunked system "ZZ P25 TEST" with the sites
//!   given (a site number and its control channels in MHz, for example `21:161.85,161.9125 22:161.8875`; NAC any) and a
//!   channel "ZZ P25 TEST" on it whose transmit flag is off. Use the sites of a system you can hear, in a band the radio
//!   receives.
//! - `zone`: puts the test channel in a zone of its own, so it can be selected on the radio (a channel in no zone can't be).
//! - `watch SECONDS`: read only; polls the radio's p25_trunk view (debuglink.md, view 37) once a second while the owner has
//!   the channel selected on the radio, and prints what the follower does.
//! - `restore BACKUP.bin`: writes the saved codeplug back.
//!
//! Nothing here keys the transmitter: the system and channel cannot transmit, and `p25t.tx` is checked to read 0.

use rfcps_core::modes::{set_kind, KIND_P25_TRUNK};
use rfcps_core::records::{Channel, Mode, MAX_CHANNELS};
use rfcps_core::systems::{EntryBody, SysEntry, System, SYSTEM_P25_TRUNK};
use rfcps_core::{Codeplug, Image};
use std::collections::BTreeMap;

/// A site as given on the command line: `NUMBER:MHZ[,MHZ]...`, the site number and its control channels in Hz.
fn parse_site(arg: &str) -> Option<(u8, Vec<u32>)> {
    let (site, ccs) = arg.split_once(':')?;
    let ccs: Vec<u32> = ccs
        .split(',')
        .map(|m| m.trim().parse::<f64>().ok().filter(|f| (30.0..=1000.0).contains(f)).map(|f| (f * 1e6).round() as u32))
        .collect::<Option<_>>()?;
    (!ccs.is_empty()).then_some((site.trim().parse().ok()?, ccs))
}

const NAME: &str = "ZZ P25 TEST";

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let (port, dir, step) = match &a[..] {
        [p, d, s, ..] if ["apply", "zone", "watch", "restore"].contains(&s.as_str()) => (p, std::path::Path::new(d), s.as_str()),
        _ => {
            eprintln!("usage: p25_test PORT BACKUP_DIR apply SITE:MHZ[,MHZ]...|zone|watch SECONDS|restore BACKUP.bin");
            std::process::exit(2);
        }
    };
    let mut radio = rfcps_link::open(port).unwrap_or_else(|e| die(&e));
    radio.ping().unwrap_or_else(|e| die(&e));

    if step == "watch" {
        let secs: u64 = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(120);
        let keys = ["p25t.built", "p25t.on", "p25t.state", "cc_hz", "voice_hz", "nac", "wacn", "sysid", "rfss", "site", "idens", "blocks", "crc_fail", "grants", "follows", "hunts", "cc_lost", "p25t.tx", "reg", "talk_hz"];
        let mut last = String::new();
        let t0 = std::time::Instant::now();
        let (mut max_state, mut max_blocks) = (0u64, 0u64);
        while t0.elapsed().as_secs() < secs {
            let v = parse(&radio.view(37).unwrap_or_else(|e| die(&e)));
            let line = v.iter().filter(|(k, _)| keys.iter().any(|w| k.ends_with(*w)) || k.starts_with("p25t.")).map(|(k, x)| format!("{k}={x}")).collect::<Vec<_>>().join(" ");
            if line != last {
                println!("[{:>3}s] {line}", t0.elapsed().as_secs());
                last = line;
            }
            if v.get("p25t.tx").is_some_and(|x| x != "0") {
                println!("p25t.tx is not 0: STOP, the transmit side is on");
                std::process::exit(1);
            }
            max_state = max_state.max(v.get("p25t.state").and_then(|x| x.parse().ok()).unwrap_or(0));
            max_blocks = max_blocks.max(v.get("blocks").and_then(|x| x.parse().ok()).unwrap_or(0));
            std::thread::sleep(std::time::Duration::from_millis(1000));
        }
        println!("highest follower state seen: {max_state} (0 off, 1 hunting, 2 on a control channel, 3 on a voice channel); control channel blocks decoded: {max_blocks}");
        return;
    }

    let base = radio.read_image(&mut |_, _| {}).unwrap_or_else(|e| die(&e));
    let edited: Image = if step == "apply" {
        let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        let backup = dir.join(format!("p25-test-before-{secs}.bin"));
        std::fs::write(&backup, base.bytes()).unwrap_or_else(|e| die(&e));
        println!("backup of the radio's codeplug: {}", backup.display());
        let sites: Vec<(u8, Vec<u32>)> = a[3..].iter().map(|x| parse_site(x).unwrap_or_else(|| die(&format!("not a site (NUMBER:MHZ[,MHZ]...): {x}")))).collect();
        if sites.is_empty() {
            die(&"apply needs at least one site, for example 21:161.85,161.9125");
        }
        let mut cp = Codeplug::decode(&base);
        if cp.systems.iter().any(|s| s.name == NAME) || cp.channels.iter().any(|c| c.name == NAME) {
            die(&"a ZZ P25 TEST system or channel is already there: restore first");
        }
        let mut sys = System::new(SYSTEM_P25_TRUNK, NAME);
        assert!(!sys.tx, "a new P25 trunked system must be listen only");
        for (i, (site, ccs)) in sites.iter().enumerate() {
            sys.entries.push(SysEntry { cls: 1, group: i as u8, control: false, flags: 0, body: EntryBody::P25Site { rfss: Some(1), site: Some(*site) }, raw: Vec::new() });
            for &hz in ccs {
                sys.entries.push(SysEntry { cls: 0, group: i as u8, control: true, flags: 0, body: EntryBody::P25Control { iden: 0, chan_no: 0, rx_hz: hz, tx_hz: 0 }, raw: Vec::new() });
            }
        }
        sys.number = (1..=255).find(|n| !cp.systems.iter().any(|s| s.number == *n)).unwrap_or_else(|| die(&"no free system number"));
        cp.systems.push(sys);
        let n = (1..=MAX_CHANNELS as u16).find(|n| !cp.channels.iter().any(|c| c.number == *n)).unwrap_or_else(|| die(&"no free channel"));
        let mut c = Channel::new(n, sites[0].1[0].into(), sites[0].1[0].into());
        c.name = NAME.to_string();
        set_kind(&mut c, KIND_P25_TRUNK, Mode::Dmr, &cp.systems);
        assert!(c.tagged.as_ref().is_some_and(|t| !t.tx), "the channel must not transmit");
        cp.channels.push(c);
        println!("adding the listen-only system and channel {n} \"{NAME}\" (transmit off)");
        cp.encode(&base)
    } else if step == "zone" {
        let mut cp = Codeplug::decode(&base);
        let n = cp.channels.iter().find(|c| c.name == NAME).map(|c| c.number).unwrap_or_else(|| die(&"no test channel: run apply first"));
        if cp.zones.iter().any(|z| z.name == NAME) {
            die(&"the test zone is already there");
        }
        cp.zones.push(rfcps_core::records::Zone { slot: cp.zones.len(), name: NAME.to_string(), channels: vec![n] });
        println!("adding a zone \"{NAME}\" holding channel {n}");
        cp.encode(&base)
    } else {
        let Some(file) = a.get(3) else { die(&"restore needs the backup file") };
        println!("restoring {file}");
        Image::from_bytes(&std::fs::read(file).unwrap_or_else(|e| die(&e)))
    };
    let to_send = radio.image_for_write(&base, &edited).unwrap_or_else(|e| die(&e));
    let jobs = radio.plan_write(&to_send).unwrap_or_else(|e| die(&e));
    println!("blocks to write: {:?}", jobs.iter().map(|(a, _)| format!("{a:#x}")).collect::<Vec<_>>());
    let report = radio.write_image(&base, &edited, &mut |d, t| println!("  block {d}/{t}")).unwrap_or_else(|e| die(&e));
    println!("wrote {} block(s), read back and verified", report.blocks);
    let now = Codeplug::decode(&radio.read_image(&mut |_, _| {}).unwrap_or_else(|e| die(&e)));
    let have = now.systems.iter().any(|s| s.name == NAME);
    println!("the radio now holds: the test system {}, {} systems, {} channels", if have { "PRESENT" } else { "absent" }, now.systems.len(), now.channels.len());
    if step == "apply" {
        let s = now.systems.iter().find(|s| s.name == NAME);
        let want: usize = a[3..].iter().filter_map(|x| parse_site(x)).map(|(_, c)| c.len()).sum();
        let ok = s.is_some_and(|s| !s.tx && s.entries.iter().filter(|e| e.cls == 0 && e.control).count() == want)
            && now.channels.iter().any(|c| c.name == NAME && c.kind() == KIND_P25_TRUNK && c.tagged.as_ref().is_some_and(|t| !t.tx));
        println!("read back: {}", if ok { format!("listen-only system with {want} control channels, and a receive-only channel on it") } else { "SOMETHING DIFFERS".to_string() });
        if !ok {
            std::process::exit(1);
        }
    }
}

/// `key=value` pairs of a view's text.
fn parse(text: &str) -> BTreeMap<String, String> {
    text.split_whitespace().filter_map(|t| t.split_once('=')).map(|(k, v)| (k.to_string(), v.to_string())).collect()
}

fn die(e: &dyn std::fmt::Display) -> ! {
    eprintln!("error: {e}");
    std::process::exit(1);
}

#[cfg(test)]
mod tests {
    use super::parse_site;

    #[test]
    fn a_site_is_a_number_and_its_control_channels_in_mhz() {
        assert_eq!(parse_site("21:161.85,161.9125"), Some((21, vec![161_850_000, 161_912_500])));
        assert_eq!(parse_site(" 3 : 851.0125 "), Some((3, vec![851_012_500])));
        assert_eq!(parse_site("21"), None);
        assert_eq!(parse_site("21:"), None);
        assert_eq!(parse_site("300:161.85"), None);
        assert_eq!(parse_site("1:abc"), None);
        assert_eq!(parse_site("1:5"), None);
    }
}
