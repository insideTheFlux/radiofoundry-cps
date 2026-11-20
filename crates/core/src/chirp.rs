// SPDX-License-Identifier: MIT
// Copyright (c) 2026 The RadioFoundry Authors
// Copyright (c) 2026 insideTheFlux
//
// CHIRP's CSV channel list (Location, Name, Frequency, Duplex, Offset, Tone, ...) into the codeplug, and back. An import
// replaces the codeplug's channels and zones with the file's FM channels as one change; rows it can't hold are counted
// in the notes. The column layout is the one CHIRP's documentation shows for its CSV files, not CHIRP's source.
// Ported from RadioFoundry's `site/static/js/codeplug/chirp.js`.

use crate::csv::parse;
use crate::fields::parse_tone;
use crate::model::Codeplug;
use crate::records::{Channel, Mode, Zone, MAX_CHANNELS, MAX_ZONES};
use std::collections::HashMap;

const REQUIRED: [&str; 7] = [
    "location",
    "name",
    "frequency",
    "duplex",
    "offset",
    "tone",
    "mode",
];
const LOW: u8 = 3;
const MID: u8 = 6;
const HIGH: u8 = 9;

fn header(rows: &[Vec<String>]) -> Vec<String> {
    rows.first()
        .map(|r| r.iter().map(|h| h.trim().to_lowercase()).collect())
        .unwrap_or_default()
}

/// True when the file's header row is CHIRP's.
pub fn is_chirp(text: &str) -> bool {
    let head = header(&parse(text));
    REQUIRED.iter().all(|k| head.iter().any(|h| h == k))
}

fn mhz(s: &str) -> Option<u64> {
    let v: f64 = s.trim().parse().ok()?;
    (v.is_finite() && v > 0.0).then(|| (v * 1e5).round() as u64 * 10)
}

/// CHIRP writes power as "5.0W" or a level name; 0 when there is none to use (the radio's setting).
fn power(s: &str) -> u8 {
    let s = s.trim().to_lowercase();
    if s.is_empty() {
        return 0;
    }
    let digits: String = s
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    if !digits.is_empty() && s[digits.len()..].trim_start().starts_with('w') {
        let w: f64 = digits.parse().unwrap_or(0.0);
        return if w <= 2.0 {
            LOW
        } else if w <= 5.0 {
            MID
        } else {
            HIGH
        };
    }
    if ["l", "low", "min"].iter().any(|p| s.starts_with(p)) {
        LOW
    } else if ["m", "mid", "med"].iter().any(|p| s.starts_with(p)) {
        MID
    } else if ["h", "high", "max"].iter().any(|p| s.starts_with(p)) {
        HIGH
    } else {
        0
    }
}

type Row = HashMap<String, String>;

fn get<'a>(r: &'a Row, k: &str) -> &'a str {
    r.get(k).map_or("", |s| s.as_str())
}

/// The radio's TX and RX tones for a row; `None` for a tone the radio can't hold.
fn tones(r: &Row) -> Result<(String, String), ()> {
    let ctcss = |k: &str| -> Option<String> {
        let v: f64 = get(r, k).trim().parse().unwrap_or(0.0);
        parse_tone(&format!("{v:.1}"))
    };
    let code = |k: &str, pol: char| -> Option<String> {
        let c = get(r, k).trim();
        parse_tone(&format!("D{c:0>3}{}", if pol == 'R' { 'I' } else { pol }))
    };
    let polarity: Vec<char> = {
        let p = if get(r, "dtcspolarity").is_empty() {
            "NN".to_string()
        } else {
            get(r, "dtcspolarity").to_uppercase()
        };
        let mut v: Vec<char> = p.chars().collect();
        while v.len() < 2 {
            v.push('N');
        }
        v
    };
    let mode = get(r, "tone").trim();
    let cross = get(r, "crossmode").trim();
    let (tx, rx): (Option<String>, Option<String>) = match mode {
        "Tone" => (ctcss("rtonefreq"), Some(String::new())),
        "TSQL" => {
            let t = ctcss("ctonefreq");
            (t.clone(), t)
        }
        "DTCS" => (code("dtcscode", polarity[0]), code("dtcscode", polarity[1])),
        "Cross" => {
            let (a, b) = cross.split_once("->").unwrap_or((cross, ""));
            let one = |x: &str, tonekey: &str, codekey: &str, pol: char| -> Option<String> {
                match x {
                    "Tone" => ctcss(tonekey),
                    "DTCS" => code(codekey, pol),
                    _ => Some(String::new()),
                }
            };
            let rxkey = if get(r, "rxdtcscode").is_empty() {
                "dtcscode"
            } else {
                "rxdtcscode"
            };
            (
                one(a, "rtonefreq", "dtcscode", polarity[0]),
                one(b, "ctonefreq", rxkey, polarity[1]),
            )
        }
        "" => (Some(String::new()), Some(String::new())),
        _ => return Err(()), // DTCS-R and TSQL-R (reverse tone squelch) among them
    };
    match (tx, rx) {
        (Some(t), Some(r)) => Ok((t, r)),
        _ => Err(()),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Note {
    Unreadable,
    Mode,
    BadFrequency,
    BadTone,
    Shortened,
    TooManyChannels,
    TooManyZones,
    Dmr,
}

impl Note {
    pub fn text(self, n: usize) -> String {
        match self {
            Note::Unreadable => format!("{n} row(s) were unreadable and were left out."),
            Note::Mode => format!("{n} row(s) were not FM or NFM and were left out."),
            Note::BadFrequency => format!("{n} row(s) had a frequency that could not be used."),
            Note::BadTone => format!("{n} tone(s) the radio can't hold were left out."),
            Note::Shortened => {
                format!("{n} name(s) were longer than 16 characters and were shortened.")
            }
            Note::TooManyChannels => format!("{n} channel(s) did not fit and were left out."),
            Note::TooManyZones => format!("{n} zone(s) did not fit and were left out."),
            Note::Dmr => format!("{n} DMR channel(s) were left out: a CHIRP file has no DMR."),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Report {
    pub channels: usize,
    pub zones: usize,
    pub notes: Vec<(Note, usize)>,
}

fn add(notes: &mut Vec<(Note, usize)>, n: Note) {
    match notes.iter_mut().find(|(k, _)| *k == n) {
        Some((_, c)) => *c += 1,
        None => notes.push((n, 1)),
    }
}

/// Replaces the codeplug's channels and zones with the file's FM channels, in zones named `zone_name` ("name", then
/// "name 2", ... at 80 channels each). Contacts and the rest stay. Nothing is changed when the file holds no channel.
pub fn import_chirp(cp: &mut Codeplug, text: &str, zone_name: &str) -> Result<Report, String> {
    let rows = parse(text);
    let head = header(&rows);
    if !REQUIRED.iter().all(|k| head.iter().any(|h| h == k)) {
        return Err(
            "That file is not a CHIRP channel list (its header row lacks CHIRP's columns).".into(),
        );
    }
    let mode_at = head.iter().position(|h| h == "mode").unwrap_or(0);
    let mut notes = Vec::new();
    let mut good: Vec<Row> = Vec::new();
    for cells in rows.iter().skip(1) {
        if cells.len() > mode_at && !cells.iter().any(|c| c.contains(['\r', '\n'])) {
            good.push(
                head.iter()
                    .enumerate()
                    .map(|(i, h)| {
                        (
                            h.clone(),
                            cells.get(i).map_or(String::new(), |c| c.trim().to_string()),
                        )
                    })
                    .collect(),
            );
        } else {
            add(&mut notes, Note::Unreadable);
        }
    }

    let mut made: Vec<Channel> = Vec::new();
    for r in &good {
        if get(r, "frequency").is_empty() && get(r, "name").is_empty() {
            continue;
        }
        let mode = get(r, "mode").to_uppercase();
        if mode != "FM" && mode != "NFM" {
            add(&mut notes, Note::Mode);
            continue;
        }
        let Some(rx_hz) = mhz(get(r, "frequency")) else {
            add(&mut notes, Note::BadFrequency);
            continue;
        };
        let mut tx_hz = rx_hz;
        let off = mhz(get(r, "offset"));
        // A repeater channel without a usable transmit frequency is receive only: simplex would transmit on its output
        let duplex = get(r, "duplex");
        let mut rx_only = duplex == "off";
        if matches!(duplex, "+" | "-" | "split") {
            // split: the offset column holds the transmit frequency
            let tx = off.and_then(|o| match duplex {
                "split" => Some(o),
                "+" => Some(rx_hz + o),
                _ => rx_hz.checked_sub(o),
            });
            match tx {
                Some(t) if t > 0 => tx_hz = t,
                _ => {
                    add(&mut notes, Note::BadFrequency);
                    rx_only = true;
                }
            }
        }
        let (tx_tone, rx_tone) = match tones(r) {
            Ok(t) => t,
            Err(()) => {
                add(&mut notes, Note::BadTone);
                (String::new(), String::new())
            }
        };
        let mut name = if get(r, "name").is_empty() {
            format!("{:.4}", rx_hz as f64 / 1e6)
        } else {
            get(r, "name").to_string()
        };
        if name.chars().count() > 16 {
            name = name.chars().take(16).collect();
            add(&mut notes, Note::Shortened);
        }
        let mut c = Channel::new(0, rx_hz, tx_hz);
        c.name = name;
        c.power = power(get(r, "power"));
        c.rx_tone = rx_tone;
        c.tx_tone = tx_tone;
        c.bw = if mode == "NFM" { 12.5 } else { 25.0 };
        c.rx_only = rx_only;
        c.all_skip = get(r, "skip") == "S";
        c.color_code = 0;
        made.push(c);
    }
    // Nothing to bring in: the codeplug stays as it is rather than losing its channels to an empty list
    if made.is_empty() {
        return Err("That CHIRP file has no FM channel to import.".into());
    }
    if made.len() > MAX_CHANNELS {
        for _ in MAX_CHANNELS..made.len() {
            add(&mut notes, Note::TooManyChannels);
        }
        made.truncate(MAX_CHANNELS);
    }
    for (i, c) in made.iter_mut().enumerate() {
        c.number = i as u16 + 1;
    }
    let mut zones: Vec<Zone> = Vec::new();
    for (k, chunk) in made.chunks(80).enumerate() {
        if zones.len() >= MAX_ZONES {
            add(&mut notes, Note::TooManyZones);
            break;
        }
        let suffix = if k > 0 {
            format!(" {}", k + 1)
        } else {
            String::new()
        };
        let base: String = zone_name
            .chars()
            .take(16 - suffix.chars().count())
            .collect();
        zones.push(Zone {
            slot: zones.len(),
            name: format!("{base}{suffix}"),
            channels: chunk.iter().map(|c| c.number).collect(),
        });
    }
    let report = Report {
        channels: made.len(),
        zones: zones.len(),
        notes,
    };
    cp.channels = made;
    cp.zones = zones;
    for v in cp.vfo.iter_mut().flatten() {
        v.contact = 0;
        v.rx_list = 0;
        v.aprs = 0;
    }
    Ok(report)
}

// --- Export ---

const HEAD: [&str; 17] = [
    "Location",
    "Name",
    "Frequency",
    "Duplex",
    "Offset",
    "Tone",
    "rToneFreq",
    "cToneFreq",
    "DtcsCode",
    "DtcsPolarity",
    "RxDtcsCode",
    "CrossMode",
    "Mode",
    "TStep",
    "Skip",
    "Power",
    "Comment",
];

fn watts(p: u8) -> &'static str {
    match p {
        1 => "1.0W",
        3 => "2.0W",
        6 => "5.0W",
        9 | 10 => "10.0W",
        _ => "",
    }
}

fn mhz_text(hz: u64) -> String {
    format!("{:.6}", hz as f64 / 1e6)
}

struct ToneCols {
    tone: &'static str,
    r_tone: String,
    c_tone: String,
    dtcs: String,
    pol: String,
    rx_dtcs: String,
    cross: String,
}

fn tone_columns(c: &Channel) -> ToneCols {
    let dcs = |t: &str| t.starts_with('D');
    let code = |t: &str| t.get(1..4).unwrap_or("023").to_string();
    let (tx, rx) = (c.tx_tone.as_str(), c.rx_tone.as_str());
    let mut o = ToneCols {
        tone: "",
        r_tone: "88.5".into(),
        c_tone: "88.5".into(),
        dtcs: "023".into(),
        pol: "NN".into(),
        rx_dtcs: "023".into(),
        cross: "Tone->Tone".into(),
    };
    if tx.is_empty() && rx.is_empty() {
        return o;
    }
    if !tx.is_empty() && !dcs(tx) && rx.is_empty() {
        o.tone = "Tone";
        o.r_tone = tx.into();
        return o;
    }
    if !tx.is_empty() && !dcs(tx) && tx == rx {
        o.tone = "TSQL";
        o.r_tone = tx.into();
        o.c_tone = tx.into();
        return o;
    }
    let pol = format!(
        "{}{}",
        if tx.ends_with('I') { 'R' } else { 'N' },
        if rx.ends_with('I') { 'R' } else { 'N' }
    );
    if !tx.is_empty() && !rx.is_empty() && dcs(tx) && code(tx) == code(rx) {
        o.tone = "DTCS";
        o.dtcs = code(tx);
        o.rx_dtcs = code(tx);
        o.pol = pol;
        return o;
    }
    // anything else is a cross mode: what is sent, then what is needed to hear
    let kind = |t: &str| {
        if t.is_empty() {
            ""
        } else if dcs(t) {
            "DTCS"
        } else {
            "Tone"
        }
    };
    o.tone = "Cross";
    o.cross = format!("{}->{}", kind(tx), kind(rx));
    if !tx.is_empty() && !dcs(tx) {
        o.r_tone = tx.into();
    }
    if !rx.is_empty() && !dcs(rx) {
        o.c_tone = rx.into();
    }
    if !tx.is_empty() && dcs(tx) {
        o.dtcs = code(tx);
    } else if !rx.is_empty() && dcs(rx) {
        o.dtcs = code(rx);
    }
    if !rx.is_empty() && dcs(rx) {
        o.rx_dtcs = code(rx);
    }
    o.pol = pol;
    o
}

/// CHIRP CSV for the codeplug's FM channels, and how many DMR channels were left out.
pub fn export_chirp(cp: &Codeplug) -> (String, usize) {
    let mut rows: Vec<Vec<String>> = vec![HEAD.iter().map(|s| (*s).to_string()).collect()];
    let mut dmr = 0;
    for c in &cp.channels {
        if c.mode != Mode::Fm {
            dmr += 1;
            continue;
        }
        let diff = c.tx_hz as i64 - c.rx_hz as i64;
        let duplex = if diff == 0 {
            ""
        } else if diff.abs() < 100_000_000 {
            if diff > 0 {
                "+"
            } else {
                "-"
            }
        } else {
            "split"
        };
        let t = tone_columns(c);
        rows.push(vec![
            c.number.to_string(),
            c.name.clone(),
            mhz_text(c.rx_hz),
            if c.rx_only { "off" } else { duplex }.into(),
            if duplex == "split" {
                mhz_text(c.tx_hz)
            } else {
                mhz_text(diff.unsigned_abs())
            },
            t.tone.into(),
            t.r_tone,
            t.c_tone,
            t.dtcs,
            t.pol,
            t.rx_dtcs,
            t.cross,
            if c.bw == 25.0 { "FM" } else { "NFM" }.into(),
            "5.00".into(),
            if c.all_skip { "S" } else { "" }.into(),
            watts(c.power).into(),
            String::new(),
        ]);
    }
    let q = |v: &str| {
        if v.contains(['"', ',', '\n']) {
            format!("\"{}\"", v.replace('"', "\"\""))
        } else {
            v.to_string()
        }
    };
    let text = rows
        .iter()
        .map(|r| r.iter().map(|v| q(v)).collect::<Vec<_>>().join(","))
        .collect::<Vec<_>>()
        .join("\r\n")
        + "\r\n";
    (text, dmr)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Image;

    const FILE: &str = "Location,Name,Frequency,Duplex,Offset,Tone,rToneFreq,cToneFreq,DtcsCode,DtcsPolarity,RxDtcsCode,CrossMode,Mode,TStep,Skip,Power,Comment\r\n\
0,Rpt 2m,146.805000,-,0.600000,Tone,100.0,88.5,023,NN,023,Tone->Tone,FM,5.00,,5.0W,\r\n\
1,Rpt 70,446.600000,-,5.000000,TSQL,141.3,141.3,023,NN,023,Tone->Tone,FM,5.00,,50W,\r\n\
2,Simplex,146.520000,,0.000000,,88.5,88.5,023,NN,023,Tone->Tone,NFM,5.00,S,,\r\n\
3,Dcs,147.000000,+,0.600000,DTCS,88.5,88.5,023,NR,023,Tone->Tone,FM,5.00,,,\r\n\
4,Cross,147.100000,+,0.600000,Cross,100.0,110.9,023,NN,023,Tone->Tone,FM,5.00,,,\r\n\
5,Split,147.200000,split,147.800000,,88.5,88.5,023,NN,023,Tone->Tone,FM,5.00,,,\r\n\
6,Listen,162.400000,off,0.000000,,88.5,88.5,023,NN,023,Tone->Tone,FM,5.00,,,\r\n\
7,Digital,445.000000,,0.000000,,88.5,88.5,023,NN,023,Tone->Tone,DV,5.00,,,\r\n\
8,Reverse,146.0,,0,TSQL-R,88.5,88.5,023,NN,023,Tone->Tone,FM,5.00,,,\r\n";

    #[test]
    fn recognizes_a_chirp_header() {
        assert!(is_chirp(FILE));
        assert!(!is_chirp("Name,Frequency\nA,1\n"));
    }

    #[test]
    fn imports_fm_channels_with_offsets_tones_and_power() {
        let mut cp = Codeplug::decode(&Image::blank());
        let r = import_chirp(&mut cp, FILE, "CHIRP").unwrap();
        assert_eq!((r.channels, r.zones), (8, 1));
        let c = |n: &str| cp.channels.iter().find(|c| c.name == n).unwrap().clone();
        let a = c("Rpt 2m");
        assert_eq!(
            (
                a.rx_hz,
                a.tx_hz,
                a.tx_tone.as_str(),
                a.rx_tone.as_str(),
                a.power,
                a.bw
            ),
            (146_805_000, 146_205_000, "100.0", "", MID, 25.0)
        );
        let b = c("Rpt 70");
        assert_eq!(
            (
                b.rx_hz,
                b.tx_hz,
                b.tx_tone.as_str(),
                b.rx_tone.as_str(),
                b.power
            ),
            (446_600_000, 441_600_000, "141.3", "141.3", HIGH)
        );
        let s = c("Simplex");
        assert_eq!(
            (s.rx_hz, s.tx_hz, s.bw, s.all_skip),
            (146_520_000, 146_520_000, 12.5, true)
        );
        let d = c("Dcs");
        assert_eq!(
            (d.tx_hz, d.tx_tone.as_str(), d.rx_tone.as_str()),
            (147_600_000, "D023N", "D023I")
        );
        let x = c("Cross");
        assert_eq!((x.tx_tone.as_str(), x.rx_tone.as_str()), ("100.0", "110.9"));
        assert_eq!(c("Split").tx_hz, 147_800_000);
        assert!(c("Listen").rx_only);
        // the DV row and the reverse-tone-squelch row: one left out for its mode, one kept without its tones
        assert!(r.notes.iter().any(|(n, k)| *n == Note::Mode && *k == 1));
        assert!(r.notes.iter().any(|(n, _)| *n == Note::BadTone));
        assert!(cp.channels.iter().map(|c| c.number).eq(1..=8));
        assert_eq!(cp.zones[0].channels.len(), 8);
    }

    #[test]
    fn a_file_with_no_fm_channel_changes_nothing() {
        let mut cp = Codeplug::decode(&Image::blank());
        cp.channels.push(Channel::new(1, 146_520_000, 146_520_000));
        let only_dv = "Location,Name,Frequency,Duplex,Offset,Tone,Mode\r\n0,D,445.0,,0,,DV\r\n";
        assert!(import_chirp(&mut cp, only_dv, "CHIRP").is_err());
        assert_eq!(cp.channels.len(), 1);
        assert!(import_chirp(&mut cp, "a,b\n1,2\n", "CHIRP").is_err());
    }

    #[test]
    fn export_then_import_gives_the_same_channels() {
        let mut cp = Codeplug::decode(&Image::blank());
        import_chirp(&mut cp, FILE, "CHIRP").unwrap();
        let (text, dmr) = export_chirp(&cp);
        assert_eq!(dmr, 0);
        let mut back = Codeplug::decode(&Image::blank());
        let r = import_chirp(&mut back, &text, "CHIRP").unwrap();
        assert_eq!(r.channels, cp.channels.len());
        for (a, b) in cp.channels.iter().zip(&back.channels) {
            assert_eq!(
                (&a.name, a.rx_hz, a.tx_hz, &a.rx_tone, &a.tx_tone, a.bw, a.rx_only),
                (&b.name, b.rx_hz, b.tx_hz, &b.rx_tone, &b.tx_tone, b.bw, b.rx_only),
                "{}",
                a.name
            );
        }
    }

    #[test]
    fn dmr_channels_are_left_out_of_an_export() {
        let mut cp = Codeplug::decode(&Image::blank());
        let mut c = Channel::new(1, 445_720_000, 440_720_000);
        c.mode = Mode::Dmr;
        cp.channels.push(c);
        cp.channels.push(Channel::new(2, 146_520_000, 146_520_000));
        let (text, dmr) = export_chirp(&cp);
        assert_eq!(dmr, 1);
        assert_eq!(text.lines().count(), 2);
    }

    #[test]
    fn long_names_are_shortened_and_zones_split_at_80() {
        let mut csv = String::from("Location,Name,Frequency,Duplex,Offset,Tone,Mode\r\n");
        for i in 0..85 {
            csv.push_str(&format!(
                "{i},A very long channel name {i},146.{:03},,0,,FM\r\n",
                100 + i
            ));
        }
        let mut cp = Codeplug::decode(&Image::blank());
        let r = import_chirp(&mut cp, &csv, "CHIRP").unwrap();
        assert_eq!((r.channels, r.zones), (85, 2));
        assert_eq!(
            (cp.zones[0].name.as_str(), cp.zones[1].name.as_str()),
            ("CHIRP", "CHIRP 2")
        );
        assert_eq!(
            (cp.zones[0].channels.len(), cp.zones[1].channels.len()),
            (80, 5)
        );
        assert!(cp.channels.iter().all(|c| c.name.chars().count() <= 16));
        assert!(r.notes.iter().any(|(n, _)| *n == Note::Shortened));
    }
}
