// SPDX-License-Identifier: MIT
// Copyright (c) 2026 The RadioFoundry Authors
// Copyright (c) 2026 insideTheFlux
//
// Codeplug tables as CSV (RFC 4180), the format of RadioFoundry's web editor (`site/static/js/codeplug/csv.js`): one
// file per table, a header row of fixed English names, references by name (a channel's contact, a zone's channels).
// Import matches rows to the records already there by number, then by name; records missing from the file are removed
// and their references cleared. A file may hold only some columns: the others keep their values.

use crate::fields::parse_tone;
use crate::model::Codeplug;
use crate::records::{
    CallType, Channel, Contact, Mode, RxList, Zone, MAX_CHANNELS, MAX_CONTACTS, MAX_RX_LISTS,
};

// --- RFC 4180 ---

/// Rows of fields. The delimiter is the first of `,` `;` or tab found outside quotes in the first line (Excel writes `;`
/// in locales with a decimal comma). A leading byte order mark is dropped; blank lines are skipped.
pub fn parse(text: &str) -> Vec<Vec<String>> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let delim = sniff(text);
    let (mut rows, mut row, mut field) = (Vec::new(), Vec::new(), String::new());
    let (mut quoted, mut any) = (false, false);
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    let end_row =
        |rows: &mut Vec<Vec<String>>, row: &mut Vec<String>, field: &mut String, any: &mut bool| {
            row.push(std::mem::take(field));
            if row.len() > 1 || !row[0].is_empty() || *any {
                rows.push(std::mem::take(row));
            } else {
                row.clear();
            }
            *any = false;
        };
    while i < chars.len() {
        let c = chars[i];
        if quoted {
            if c == '"' {
                if chars.get(i + 1) == Some(&'"') {
                    field.push('"');
                    i += 1;
                } else {
                    quoted = false;
                }
            } else {
                field.push(c);
            }
        } else if c == '"' && field.is_empty() {
            quoted = true;
            any = true;
        } else if c == delim {
            row.push(std::mem::take(&mut field));
        } else if c == '\r' || c == '\n' {
            if c == '\r' && chars.get(i + 1) == Some(&'\n') {
                i += 1;
            }
            end_row(&mut rows, &mut row, &mut field, &mut any);
        } else {
            field.push(c);
        }
        i += 1;
    }
    if !field.is_empty() || !row.is_empty() || any {
        end_row(&mut rows, &mut row, &mut field, &mut any);
    }
    rows
}

fn sniff(text: &str) -> char {
    let mut quoted = false;
    for c in text.chars() {
        match c {
            '"' => quoted = !quoted,
            ',' | ';' | '\t' if !quoted => return c,
            '\n' | '\r' if !quoted => break,
            _ => {}
        }
    }
    ','
}

pub fn stringify(rows: &[Vec<String>]) -> String {
    let cell = |s: &str| -> String {
        let needs = s.contains(['"', ',', '\r', '\n'])
            || s.starts_with(char::is_whitespace)
            || s.ends_with(char::is_whitespace);
        if needs {
            format!("\"{}\"", s.replace('"', "\"\""))
        } else {
            s.to_string()
        }
    };
    let mut out = String::new();
    for r in rows {
        out.push_str(&r.iter().map(|c| cell(c)).collect::<Vec<_>>().join(","));
        out.push_str("\r\n");
    }
    out
}

// --- Values ---

/// A cell that does not hold what its column needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bad(pub String);

fn bad<T>(m: impl Into<String>) -> Result<T, Bad> {
    Err(Bad(m.into()))
}

fn as_bool(s: &str) -> Result<bool, Bad> {
    match s.trim().to_lowercase().as_str() {
        "yes" | "y" | "true" | "1" | "on" | "x" => Ok(true),
        "no" | "n" | "false" | "0" | "off" | "" => Ok(false),
        _ => bad("not yes or no"),
    }
}

fn yes_no(v: bool) -> String {
    if v { "Yes" } else { "No" }.into()
}

fn as_int(s: &str, lo: i64, hi: i64, blank: Option<i64>) -> Result<i64, Bad> {
    let s = s.trim();
    if s.is_empty() {
        if let Some(b) = blank {
            return Ok(b);
        }
    }
    match s.parse::<i64>() {
        Ok(n) if (lo..=hi).contains(&n) => Ok(n),
        _ => bad(format!("not a whole number from {lo} to {hi}")),
    }
}

fn as_num(s: &str, lo: f64, hi: f64) -> Result<f64, Bad> {
    let s = s.trim().replace(',', ".");
    if s.is_empty() {
        return Ok(0.0);
    }
    match s.parse::<f64>() {
        Ok(n) if n.is_finite() && (lo..=hi).contains(&n) => Ok(n),
        _ => bad(format!("not a number from {lo} to {hi}")),
    }
}

fn as_name(s: &str, max: usize, required: bool) -> Result<String, Bad> {
    let s = s.trim();
    if required && s.is_empty() {
        return bad("no name");
    }
    if s.chars().count() > max {
        return bad(format!("longer than {max} characters"));
    }
    Ok(s.to_string())
}

/// "446.00625" or "446006250" (Hz) to Hz on the 10 Hz grid; `None` if it isn't a frequency below 1 GHz.
pub fn parse_freq(text: &str) -> Option<u64> {
    let s = text.trim();
    let (whole, frac) = s.split_once('.').map_or((s, None), |(w, f)| (w, Some(f)));
    if whole.is_empty()
        || !whole.chars().all(|c| c.is_ascii_digit())
        || frac.is_some_and(|f| f.is_empty() || !f.chars().all(|c| c.is_ascii_digit()))
    {
        return None;
    }
    let n: f64 = s.parse().ok()?;
    let hz = if n < 1e4 {
        (n * 1e5).round() as u64 * 10
    } else {
        (n / 10.0).round() as u64 * 10
    };
    (hz > 0 && hz < 1_000_000_000).then_some(hz)
}

/// MHz as the web editor writes it: five decimals with up to two trailing zeros dropped.
pub fn mhz(hz: u64) -> String {
    let s = format!("{:.5}", hz as f64 / 1e6);
    if let Some(t) = s.strip_suffix("00") {
        t.to_string()
    } else if let Some(t) = s.strip_suffix('0') {
        t.to_string()
    } else {
        s
    }
}

fn as_freq(s: &str, blank: Option<u64>) -> Result<u64, Bad> {
    if s.trim().is_empty() {
        if let Some(b) = blank {
            return Ok(b);
        }
    }
    parse_freq(&s.trim().replace(',', ".")).ok_or_else(|| Bad("not a frequency".into()))
}

fn as_tone(s: &str) -> Result<String, Bad> {
    parse_tone(&s.trim().replace(',', ".")).ok_or_else(|| Bad("not a CTCSS or DCS tone".into()))
}

fn as_id(s: &str) -> Result<u32, Bad> {
    let s = s.trim();
    if s.is_empty() || s == "0" {
        return Ok(0);
    }
    match s.parse::<u32>() {
        Ok(n) if (1..=0xFF_FFFF).contains(&n) => Ok(n),
        _ => bad("not an ID from 1 to 16777215"),
    }
}

const POWER_NAMES: [(u8, &str); 6] = [
    (0, "Default"),
    (1, "Min"),
    (3, "Low"),
    (6, "Mid"),
    (9, "High"),
    (10, "Max"),
];

fn power_text(n: u8) -> String {
    POWER_NAMES
        .iter()
        .find(|(v, _)| *v == n)
        .map_or_else(|| n.to_string(), |(_, s)| (*s).to_string())
}

fn as_power(s: &str) -> Result<u8, Bad> {
    let v = s.trim().to_lowercase();
    if v.is_empty() {
        return Ok(0);
    }
    if let Some((n, _)) = POWER_NAMES.iter().find(|(_, w)| w.to_lowercase() == v) {
        return Ok(*n);
    }
    match v.parse::<u8>() {
        Ok(n) if n <= 10 => Ok(n),
        _ => bad("not a power: Default, Min, Low, Mid, High, Max or 0 to 10"),
    }
}

fn coord_text(x: f64) -> String {
    if x == 0.0 {
        return String::new();
    }
    let s = format!("{x:.4}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

// --- References by name ---

const SEP: char = '|';

/// A record's name in a file: its name, or "#n" when the name can't stand for it (another record has it, or it holds
/// the member list separator, or looks like "#n" itself).
fn ref_text(number: u16, name: &str, same_name: usize) -> String {
    let like_number = name
        .trim()
        .strip_prefix('#')
        .is_some_and(|r| !r.is_empty() && r.chars().all(|c| c.is_ascii_digit()));
    if !name.contains(SEP) && !like_number && same_name == 1 {
        name.to_string()
    } else {
        format!("#{number}")
    }
}

fn find_ref(text: &str, pool: &[(u16, String)]) -> Result<Option<u16>, Bad> {
    let s = text.trim();
    if s.is_empty() {
        return Ok(None);
    }
    if let Some(n) = s.strip_prefix('#').and_then(|r| r.parse::<u16>().ok()) {
        return pool
            .iter()
            .find(|(m, _)| *m == n)
            .map(|(m, _)| Some(*m))
            .ok_or_else(|| Bad(format!("no record {s}")));
    }
    let exact: Vec<_> = pool.iter().filter(|(_, n)| n == s).collect();
    if exact.len() == 1 {
        return Ok(Some(exact[0].0));
    }
    let loose: Vec<_> = if exact.is_empty() {
        pool.iter()
            .filter(|(_, n)| n.to_lowercase() == s.to_lowercase())
            .collect()
    } else {
        exact
    };
    match loose.len() {
        1 => Ok(Some(loose[0].0)),
        0 => bad(format!("no record named {s}")),
        _ => bad(format!("more than one record named {s}")),
    }
}

fn pool_text(n: u16, name: &str, pool: &[(u16, String)]) -> String {
    ref_text(n, name, pool.iter().filter(|(_, x)| x == name).count())
}

fn refs_text(numbers: &[u16], pool: &[(u16, String)]) -> String {
    numbers
        .iter()
        .map(|n| {
            pool.iter()
                .find(|(m, _)| m == n)
                .map_or_else(|| format!("#{n}"), |(_, name)| pool_text(*n, name, pool))
        })
        .collect::<Vec<_>>()
        .join(&SEP.to_string())
}

fn find_refs(
    text: &str,
    pool: &[(u16, String)],
    max: usize,
    problems: &mut Vec<String>,
) -> Vec<u16> {
    let mut out = Vec::new();
    for part in text.split(SEP) {
        if part.trim().is_empty() {
            continue;
        }
        match find_ref(part, pool) {
            Ok(Some(n)) => out.push(n),
            Ok(None) => {}
            Err(e) => problems.push(e.0),
        }
    }
    if out.len() > max {
        problems.push(format!("{} members; the most is {max}", out.len()));
        out.truncate(max);
    }
    out
}

// --- Tables ---

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableKind {
    Contacts,
    RxLists,
    Channels,
    Zones,
}

/// Import order: what a table refers to comes first.
pub const ORDER: [TableKind; 4] = [
    TableKind::Contacts,
    TableKind::RxLists,
    TableKind::Channels,
    TableKind::Zones,
];

impl TableKind {
    pub fn file(self) -> &'static str {
        match self {
            TableKind::Contacts => "contacts.csv",
            TableKind::RxLists => "rx-group-lists.csv",
            TableKind::Channels => "channels.csv",
            TableKind::Zones => "zones.csv",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            TableKind::Contacts => "Contacts",
            TableKind::RxLists => "RX group lists",
            TableKind::Channels => "Channels",
            TableKind::Zones => "Zones",
        }
    }

    fn required(self) -> &'static [&'static str] {
        match self {
            TableKind::Contacts => &["name", "id"],
            TableKind::RxLists => &["name", "contacts"],
            TableKind::Channels => &["name", "rxHz"],
            TableKind::Zones => &["name", "channels"],
        }
    }
}

struct Col {
    header: &'static str,
    aliases: &'static [&'static str],
    key: &'static str,
}

const fn col(header: &'static str, key: &'static str, aliases: &'static [&'static str]) -> Col {
    Col {
        header,
        aliases,
        key,
    }
}

const NUMBER: Col = col("Number", "number", &["No", "#", "Index"]);

const CONTACT_COLS: &[Col] = &[
    NUMBER,
    col("Name", "name", &["Contact Name", "Contact"]),
    col(
        "ID",
        "id",
        &["DMR ID", "Call ID", "Radio ID", "TG", "Talkgroup"],
    ),
    col("Call Type", "type", &["Type"]),
    col("Timeslot", "timeslot", &["TS", "Timeslot Override"]),
];
const LIST_COLS: &[Col] = &[
    NUMBER,
    col("Name", "name", &["Group List", "RX Group List"]),
    col("Contacts", "contacts", &["Members", "Talkgroups"]),
];
const ZONE_COLS: &[Col] = &[
    col("Name", "name", &["Zone Name", "Zone"]),
    col("Channels", "channels", &["Members", "Channel Members"]),
];
const CHANNEL_COLS: &[Col] = &[
    NUMBER,
    col("Name", "name", &["Channel Name", "Channel"]),
    col("Mode", "mode", &["Channel Mode", "Channel Type", "Type"]),
    col(
        "RX Frequency (MHz)",
        "rxHz",
        &["RX Frequency", "RX Freq", "Receive Frequency", "RX"],
    ),
    col(
        "TX Frequency (MHz)",
        "txHz",
        &["TX Frequency", "TX Freq", "Transmit Frequency", "TX"],
    ),
    col("Power", "power", &["TX Power"]),
    col("Timeout (s)", "tot", &["Timeout", "TOT"]),
    col("Bandwidth (kHz)", "bw", &["Bandwidth", "BW"]),
    col("RX Tone", "rxTone", &["RX CTCSS/DCS", "Decode", "RX CTCSS"]),
    col("TX Tone", "txTone", &["TX CTCSS/DCS", "Encode", "TX CTCSS"]),
    col("Squelch", "squelch", &[]),
    col("Color Code", "colorCode", &["CC", "Colour Code"]),
    col("Timeslot", "timeslot", &["TS", "Time Slot", "Slot"]),
    col("TX Contact", "contact", &["Contact"]),
    col("RX Group List", "rxList", &["Group List", "RX List"]),
    col("APRS", "aprs", &["APRS Configuration"]),
    col("DMR ID", "dmrId", &["Channel DMR ID"]),
    col("RX Only", "rxOnly", &[]),
    col("All Skip", "allSkip", &["Scan Skip"]),
    col("Zone Skip", "zoneSkip", &[]),
    col("VOX", "vox", &[]),
    col("Tone Burst", "toneBurst", &["1750 Hz"]),
    col("No Beep", "noBeep", &[]),
    col("No Eco", "noEco", &[]),
    col("DMO", "dmo", &[]),
    col("Roaming", "roam", &[]),
    col("Talker Alias TS1", "aliasTs1", &[]),
    col("Talker Alias TS2", "aliasTs2", &[]),
    col("Use Location", "useLocation", &[]),
    col("Latitude", "lat", &[]),
    col("Longitude", "lon", &[]),
    col("SFR", "sfr", &[]),
    col("SFR A", "sfrA", &[]),
    col("SFR B", "sfrB", &[]),
];

fn columns(t: TableKind) -> &'static [Col] {
    match t {
        TableKind::Contacts => CONTACT_COLS,
        TableKind::RxLists => LIST_COLS,
        TableKind::Channels => CHANNEL_COLS,
        TableKind::Zones => ZONE_COLS,
    }
}

fn pool_contacts(cp: &Codeplug) -> Vec<(u16, String)> {
    cp.contacts
        .iter()
        .map(|c| (c.number, c.name.clone()))
        .collect()
}
fn pool_lists(cp: &Codeplug) -> Vec<(u16, String)> {
    cp.rx_lists
        .iter()
        .map(|l| (u16::from(l.number), l.name.clone()))
        .collect()
}
fn pool_channels(cp: &Codeplug) -> Vec<(u16, String)> {
    cp.channels
        .iter()
        .map(|c| (c.number, c.name.clone()))
        .collect()
}

// --- Export ---

pub fn export_table(cp: &Codeplug, t: TableKind) -> String {
    let cols = columns(t);
    let mut rows = vec![cols
        .iter()
        .map(|c| c.header.to_string())
        .collect::<Vec<_>>()];
    let (contacts, lists, channels) = (pool_contacts(cp), pool_lists(cp), pool_channels(cp));
    match t {
        TableKind::Contacts => {
            for r in &cp.contacts {
                rows.push(vec![
                    r.number.to_string(),
                    r.name.clone(),
                    r.id.to_string(),
                    match r.call_type {
                        CallType::Group => "Group",
                        CallType::Private => "Private",
                        CallType::All => "All",
                    }
                    .into(),
                    if r.timeslot == 0 {
                        String::new()
                    } else {
                        r.timeslot.to_string()
                    },
                ]);
            }
        }
        TableKind::RxLists => {
            for l in &cp.rx_lists {
                rows.push(vec![
                    l.number.to_string(),
                    l.name.clone(),
                    refs_text(&l.contacts, &contacts),
                ]);
            }
        }
        TableKind::Zones => {
            for z in &cp.zones {
                rows.push(vec![z.name.clone(), refs_text(&z.channels, &channels)]);
            }
        }
        TableKind::Channels => {
            let one = |n: u16, pool: &[(u16, String)]| -> String {
                if n == 0 {
                    String::new()
                } else {
                    pool.iter()
                        .find(|(m, _)| *m == n)
                        .map_or_else(|| format!("#{n}"), |(_, name)| pool_text(n, name, pool))
                }
            };
            for c in &cp.channels {
                rows.push(vec![
                    c.number.to_string(),
                    c.name.clone(),
                    if c.mode == Mode::Dmr { "DMR" } else { "FM" }.into(),
                    mhz(c.rx_hz),
                    mhz(c.tx_hz),
                    power_text(c.power),
                    if c.tot == 0 {
                        "0".into()
                    } else {
                        c.tot.to_string()
                    },
                    if c.bw == 25.0 { "25" } else { "12.5" }.into(),
                    c.rx_tone.clone(),
                    c.tx_tone.clone(),
                    c.squelch.to_string(),
                    c.color_code.to_string(),
                    c.timeslot.to_string(),
                    one(c.contact, &contacts),
                    one(u16::from(c.rx_list), &lists),
                    if c.aprs == 0 {
                        String::new()
                    } else {
                        format!("#{}", c.aprs)
                    },
                    if c.dmr_id == 0 {
                        String::new()
                    } else {
                        c.dmr_id.to_string()
                    },
                    yes_no(c.rx_only),
                    yes_no(c.all_skip),
                    yes_no(c.zone_skip),
                    yes_no(c.vox),
                    yes_no(c.tone_burst),
                    yes_no(c.no_beep),
                    yes_no(c.no_eco),
                    yes_no(c.dmo),
                    yes_no(c.roam),
                    c.alias_ts1.to_string(),
                    c.alias_ts2.to_string(),
                    yes_no(c.use_location),
                    coord_text(c.lat),
                    coord_text(c.lon),
                    yes_no(c.sfr),
                    c.sfr_a.to_string(),
                    c.sfr_b.to_string(),
                ]);
            }
        }
    }
    stringify(&rows)
}

/// Every table, as `(file name, text)`; the text starts with a byte order mark so a spreadsheet reads it as UTF-8.
pub fn export_all(cp: &Codeplug) -> Vec<(&'static str, String)> {
    ORDER
        .iter()
        .map(|&t| (t.file(), format!("\u{feff}{}", export_table(cp, t))))
        .collect()
}

// --- Import ---

fn norm(s: &str) -> String {
    let mut out = String::new();
    let mut depth = 0;
    for c in s.to_lowercase().chars() {
        match c {
            '(' => depth += 1,
            ')' if depth > 0 => depth -= 1,
            _ if depth == 0 && (c.is_ascii_alphanumeric() || c == '#') => out.push(c),
            _ => {}
        }
    }
    out
}

/// The columns a header row names: `(column index in the table, index in the file)`, and the headers not recognized.
fn map_header(t: TableKind, head: &[String]) -> (Vec<(&'static Col, usize)>, Vec<String>) {
    let (mut found, mut unknown): (Vec<(&Col, usize)>, Vec<String>) = (Vec::new(), Vec::new());
    for (index, h) in head.iter().enumerate() {
        let n = norm(h);
        let hit = columns(t).iter().find(|c| {
            !found.iter().any(|(f, _)| std::ptr::eq(*f, *c))
                && std::iter::once(c.header)
                    .chain(c.aliases.iter().copied())
                    .any(|a| norm(a) == n)
        });
        match hit {
            Some(c) => found.push((c, index)),
            None if !n.is_empty() => unknown.push(h.trim().to_string()),
            None => {}
        }
    }
    (found, unknown)
}

/// Which table a header row is for: the one whose columns it names most, among those whose required columns it has.
pub fn detect(text: &str) -> Option<TableKind> {
    let rows = parse(text);
    let head = rows.first()?;
    let (mut best, mut score) = (None, 0);
    for t in ORDER {
        let (found, _) = map_header(t, head);
        if !t
            .required()
            .iter()
            .all(|k| found.iter().any(|(c, _)| c.key == *k))
        {
            continue;
        }
        if found.len() > score {
            best = Some(t);
            score = found.len();
        }
    }
    best
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CsvProblem {
    pub row: usize,
    pub column: String,
    pub message: String,
    pub skipped: bool,
}

#[derive(Clone, Debug)]
pub struct ImportResult {
    pub table: TableKind,
    pub added: usize,
    pub updated: usize,
    pub removed: usize,
    pub problems: Vec<CsvProblem>,
    pub unknown: Vec<String>,
}

fn set_channel(
    c: &mut Channel,
    key: &str,
    s: &str,
    cp: &Codeplug,
    notes: &mut Vec<String>,
) -> Result<(), Bad> {
    match key {
        "name" => c.name = as_name(s, 16, true)?,
        "mode" => {
            c.mode = match s.trim().to_lowercase().as_str() {
                "" | "fm" => Mode::Fm,
                "dmr" => Mode::Dmr,
                _ => return bad("not FM or DMR"),
            }
        }
        "rxHz" => c.rx_hz = as_freq(s, None)?,
        "txHz" => c.tx_hz = as_freq(s, None)?,
        "power" => c.power = as_power(s)?,
        "tot" => c.tot = as_int(s, 0, 3825, Some(0))? as u16,
        "bw" => {
            let v = s.trim().replace(',', ".").to_lowercase();
            let v = v.trim_end_matches("khz").trim();
            c.bw = match v {
                "" | "12.5" => 12.5,
                "25" => 25.0,
                _ => return bad("not 12.5 or 25"),
            }
        }
        "rxTone" => c.rx_tone = as_tone(s)?,
        "txTone" => c.tx_tone = as_tone(s)?,
        "squelch" => c.squelch = as_int(s, 0, 21, Some(0))? as u8,
        "colorCode" => c.color_code = as_int(s, 0, 15, Some(1))? as u8,
        "timeslot" => c.timeslot = as_int(s, 1, 2, Some(1))? as u8,
        "contact" => c.contact = find_ref(s, &pool_contacts(cp))?.unwrap_or(0),
        "rxList" => c.rx_list = find_ref(s, &pool_lists(cp))?.map_or(0, |n| n as u8),
        "aprs" => {
            let t = s.trim();
            c.aprs = if t.is_empty() {
                0
            } else if let Some(n) = t.strip_prefix('#').and_then(|r| r.parse::<u8>().ok()) {
                n
            } else {
                notes.push(format!("APRS configuration \"{t}\" is not imported"));
                0
            };
        }
        "dmrId" => c.dmr_id = as_id(s)?,
        "rxOnly" => c.rx_only = as_bool(s)?,
        "allSkip" => c.all_skip = as_bool(s)?,
        "zoneSkip" => c.zone_skip = as_bool(s)?,
        "vox" => c.vox = as_bool(s)?,
        "toneBurst" => c.tone_burst = as_bool(s)?,
        "noBeep" => c.no_beep = as_bool(s)?,
        "noEco" => c.no_eco = as_bool(s)?,
        "dmo" => c.dmo = as_bool(s)?,
        "roam" => c.roam = as_bool(s)?,
        "aliasTs1" => c.alias_ts1 = as_int(s, 0, 3, Some(0))? as u8,
        "aliasTs2" => c.alias_ts2 = as_int(s, 0, 3, Some(0))? as u8,
        "useLocation" => c.use_location = as_bool(s)?,
        "lat" => c.lat = as_num(s, -90.0, 90.0)?,
        "lon" => c.lon = as_num(s, -180.0, 180.0)?,
        "sfr" => c.sfr = as_bool(s)?,
        "sfrA" => c.sfr_a = as_int(s, 0, 255, Some(0))? as u8,
        "sfrB" => c.sfr_b = as_int(s, 0, 255, Some(0))? as u8,
        _ => {}
    }
    Ok(())
}

fn set_contact(c: &mut Contact, key: &str, s: &str) -> Result<(), Bad> {
    match key {
        "name" => c.name = as_name(s, 16, true)?,
        "id" => c.id = as_int(s, 1, 0xFF_FFFF, None)? as u32,
        "type" => {
            c.call_type = match s.trim().to_lowercase().as_str() {
                "" | "group" => CallType::Group,
                "private" => CallType::Private,
                "all" => CallType::All,
                _ => return bad("not Group, Private or All"),
            }
        }
        "timeslot" => c.timeslot = as_int(s, 0, 2, Some(0))? as u8,
        _ => {}
    }
    Ok(())
}

/// Imports one table's CSV into `cp` (changed in place).
pub fn import_table(
    cp: &mut Codeplug,
    text: &str,
    table: Option<TableKind>,
) -> Result<ImportResult, String> {
    let t = table
        .or_else(|| detect(text))
        .ok_or("That file is not one of the codeplug tables (no table's columns match).")?;
    let rows = parse(text);
    let (found, unknown) = map_header(t, rows.first().map_or(&[][..], |r| &r[..]));
    let has_number = found.iter().any(|(c, _)| c.key == "number");
    let mut problems: Vec<CsvProblem> = Vec::new();
    let (mut added, mut updated) = (0, 0);

    macro_rules! cell {
        ($row:expr, $key:expr) => {
            found
                .iter()
                .find(|(c, _)| c.key == $key)
                .map(|(_, i)| $row.get(*i).map_or("", |s| s.as_str()))
        };
    }

    match t {
        TableKind::Contacts => {
            let old = cp.contacts.clone();
            let mut taken = vec![false; old.len()];
            let mut out: Vec<Contact> = Vec::new();
            for (i, row) in rows.iter().enumerate().skip(1) {
                if row.iter().all(|v| v.trim().is_empty()) {
                    continue;
                }
                let n_text = if has_number {
                    cell!(row, "number").unwrap_or("").trim().to_string()
                } else {
                    String::new()
                };
                let name_text = cell!(row, "name").unwrap_or("").trim().to_string();
                let idx = (!n_text.is_empty())
                    .then(|| {
                        old.iter()
                            .enumerate()
                            .position(|(k, x)| !taken[k] && x.number.to_string() == n_text)
                    })
                    .flatten()
                    .or_else(|| {
                        (!name_text.is_empty())
                            .then(|| {
                                old.iter()
                                    .enumerate()
                                    .position(|(k, x)| !taken[k] && x.name == name_text)
                            })
                            .flatten()
                    });
                let fresh = idx.is_none();
                let mut work = match idx {
                    Some(k) => {
                        taken[k] = true;
                        old[k].clone()
                    }
                    None => Contact {
                        number: 0,
                        name: String::new(),
                        id: 1,
                        call_type: CallType::Group,
                        timeslot: 0,
                    },
                };
                let mut skip = false;
                for (c, ix) in &found {
                    if c.key == "number" {
                        match as_int(
                            row.get(*ix).map_or("", |s| s.as_str()),
                            1,
                            MAX_CONTACTS as i64,
                            None,
                        ) {
                            Ok(n) => work.number = n as u16,
                            Err(e) if fresh && !n_text.is_empty() => problems.push(CsvProblem {
                                row: i + 1,
                                column: c.header.into(),
                                message: e.0,
                                skipped: false,
                            }),
                            Err(_) => {}
                        }
                        continue;
                    }
                    if let Err(e) =
                        set_contact(&mut work, c.key, row.get(*ix).map_or("", |s| s.as_str()))
                    {
                        let lost = fresh && t.required().contains(&c.key);
                        problems.push(CsvProblem {
                            row: i + 1,
                            column: c.header.into(),
                            message: e.0,
                            skipped: lost,
                        });
                        skip |= lost;
                    }
                }
                if skip {
                    continue;
                }
                if work.name.is_empty() {
                    problems.push(CsvProblem {
                        row: i + 1,
                        column: "Name".into(),
                        message: "no name".into(),
                        skipped: true,
                    });
                    continue;
                }
                if fresh {
                    added += 1;
                } else {
                    updated += 1;
                }
                out.push(work);
            }
            let gone: Vec<u16> = old
                .iter()
                .enumerate()
                .filter(|(k, _)| !taken[*k])
                .map(|(_, c)| c.number)
                .collect();
            assign_numbers(
                &mut out.iter_mut().map(|c| &mut c.number).collect::<Vec<_>>(),
                MAX_CONTACTS as u16,
            );
            cp.contacts = out;
            for n in &gone {
                for c in &mut cp.channels {
                    if c.contact == *n {
                        c.contact = 0;
                    }
                }
                for l in &mut cp.rx_lists {
                    l.contacts.retain(|m| m != n);
                }
            }
            if cp.contacts.len() > MAX_CONTACTS {
                problems.push(CsvProblem {
                    row: 0,
                    column: String::new(),
                    message: format!("{} contacts; the most is {MAX_CONTACTS}", cp.contacts.len()),
                    skipped: false,
                });
            }
            return Ok(ImportResult {
                table: t,
                added,
                updated,
                removed: gone.len(),
                problems,
                unknown,
            });
        }
        TableKind::RxLists => {
            let pool = pool_contacts(cp);
            let old = cp.rx_lists.clone();
            let mut taken = vec![false; old.len()];
            let mut out: Vec<RxList> = Vec::new();
            for (i, row) in rows.iter().enumerate().skip(1) {
                if row.iter().all(|v| v.trim().is_empty()) {
                    continue;
                }
                let n_text = if has_number {
                    cell!(row, "number").unwrap_or("").trim().to_string()
                } else {
                    String::new()
                };
                let name_text = cell!(row, "name").unwrap_or("").trim().to_string();
                let idx = (!n_text.is_empty())
                    .then(|| {
                        old.iter()
                            .enumerate()
                            .position(|(k, x)| !taken[k] && x.number.to_string() == n_text)
                    })
                    .flatten()
                    .or_else(|| {
                        (!name_text.is_empty())
                            .then(|| {
                                old.iter()
                                    .enumerate()
                                    .position(|(k, x)| !taken[k] && x.name == name_text)
                            })
                            .flatten()
                    });
                let fresh = idx.is_none();
                let mut work = match idx {
                    Some(k) => {
                        taken[k] = true;
                        old[k].clone()
                    }
                    None => RxList {
                        number: 0,
                        name: String::new(),
                        contacts: Vec::new(),
                    },
                };
                let mut skip = false;
                for (c, ix) in &found {
                    let s = row.get(*ix).map_or("", |s| s.as_str());
                    match c.key {
                        "number" => {
                            if let Ok(n) = as_int(s, 1, MAX_RX_LISTS as i64, None) {
                                work.number = n as u8;
                            }
                        }
                        "name" => match as_name(s, 16, true) {
                            Ok(v) => work.name = v,
                            Err(e) => {
                                let lost = fresh;
                                problems.push(CsvProblem {
                                    row: i + 1,
                                    column: c.header.into(),
                                    message: e.0,
                                    skipped: lost,
                                });
                                skip |= lost;
                            }
                        },
                        "contacts" => {
                            let mut msgs = Vec::new();
                            work.contacts = find_refs(s, &pool, 32, &mut msgs);
                            for m in msgs {
                                problems.push(CsvProblem {
                                    row: i + 1,
                                    column: c.header.into(),
                                    message: m,
                                    skipped: false,
                                });
                            }
                        }
                        _ => {}
                    }
                }
                if skip || work.name.is_empty() {
                    continue;
                }
                if fresh {
                    added += 1;
                } else {
                    updated += 1;
                }
                out.push(work);
            }
            let gone: Vec<u8> = old
                .iter()
                .enumerate()
                .filter(|(k, _)| !taken[*k])
                .map(|(_, l)| l.number)
                .collect();
            let mut nums: Vec<u16> = out.iter().map(|l| u16::from(l.number)).collect();
            assign_numbers(
                &mut nums.iter_mut().collect::<Vec<_>>(),
                MAX_RX_LISTS as u16,
            );
            for (l, n) in out.iter_mut().zip(nums) {
                l.number = n as u8;
            }
            cp.rx_lists = out;
            for n in &gone {
                for c in &mut cp.channels {
                    if c.rx_list == *n {
                        c.rx_list = 0;
                    }
                }
            }
            return Ok(ImportResult {
                table: t,
                added,
                updated,
                removed: gone.len(),
                problems,
                unknown,
            });
        }
        TableKind::Channels => {
            let old = cp.channels.clone();
            let mut taken = vec![false; old.len()];
            let mut out: Vec<Channel> = Vec::new();
            let snapshot = cp.clone();
            for (i, row) in rows.iter().enumerate().skip(1) {
                if row.iter().all(|v| v.trim().is_empty()) {
                    continue;
                }
                let n_text = if has_number {
                    cell!(row, "number").unwrap_or("").trim().to_string()
                } else {
                    String::new()
                };
                let name_text = cell!(row, "name").unwrap_or("").trim().to_string();
                let idx = (!n_text.is_empty())
                    .then(|| {
                        old.iter()
                            .enumerate()
                            .position(|(k, x)| !taken[k] && x.number.to_string() == n_text)
                    })
                    .flatten()
                    .or_else(|| {
                        (!name_text.is_empty())
                            .then(|| {
                                old.iter()
                                    .enumerate()
                                    .position(|(k, x)| !taken[k] && x.name == name_text)
                            })
                            .flatten()
                    });
                let fresh = idx.is_none();
                let mut work = match idx {
                    Some(k) => {
                        taken[k] = true;
                        old[k].clone()
                    }
                    None => Channel::new(0, 146_520_000, 146_520_000),
                };
                let mut skip = false;
                for (c, ix) in &found {
                    let s = row.get(*ix).map_or("", |s| s.as_str());
                    if c.key == "number" {
                        if let Ok(n) = as_int(s, 1, MAX_CHANNELS as i64, None) {
                            work.number = n as u16;
                        }
                        continue;
                    }
                    let mut notes = Vec::new();
                    let r = set_channel(&mut work, c.key, s, &snapshot, &mut notes);
                    for m in notes {
                        problems.push(CsvProblem {
                            row: i + 1,
                            column: c.header.into(),
                            message: m,
                            skipped: false,
                        });
                    }
                    if let Err(e) = r {
                        let lost = fresh && t.required().contains(&c.key);
                        problems.push(CsvProblem {
                            row: i + 1,
                            column: c.header.into(),
                            message: e.0,
                            skipped: lost,
                        });
                        skip |= lost;
                    }
                }
                if skip {
                    continue;
                }
                if work.name.is_empty() {
                    problems.push(CsvProblem {
                        row: i + 1,
                        column: "Name".into(),
                        message: "no name".into(),
                        skipped: true,
                    });
                    continue;
                }
                if fresh {
                    // a new channel's TX follows RX unless the file gave one
                    if !found.iter().any(|(c, _)| c.key == "txHz") {
                        work.tx_hz = work.rx_hz;
                    }
                    added += 1;
                } else {
                    updated += 1;
                }
                out.push(work);
            }
            let gone: Vec<u16> = old
                .iter()
                .enumerate()
                .filter(|(k, _)| !taken[*k])
                .map(|(_, c)| c.number)
                .collect();
            let mut nums: Vec<u16> = out.iter().map(|c| c.number).collect();
            assign_numbers(
                &mut nums.iter_mut().collect::<Vec<_>>(),
                MAX_CHANNELS as u16,
            );
            for (c, n) in out.iter_mut().zip(nums) {
                c.number = n;
            }
            cp.channels = out;
            // a renumbered channel keeps its place in the zones: zones refer by number, so map old to new
            let remap: std::collections::HashMap<u16, u16> = old
                .iter()
                .enumerate()
                .filter(|(k, _)| taken[*k])
                .filter_map(|(_, o)| {
                    cp.channels
                        .iter()
                        .find(|c| c.name == o.name)
                        .map(|c| (o.number, c.number))
                })
                .collect();
            for z in &mut cp.zones {
                z.channels = z
                    .channels
                    .iter()
                    .filter(|n| !gone.contains(n))
                    .map(|n| remap.get(n).copied().unwrap_or(*n))
                    .collect();
            }
            if cp.channels.len() > MAX_CHANNELS {
                problems.push(CsvProblem {
                    row: 0,
                    column: String::new(),
                    message: format!("{} channels; the most is {MAX_CHANNELS}", cp.channels.len()),
                    skipped: false,
                });
            }
            return Ok(ImportResult {
                table: t,
                added,
                updated,
                removed: gone.len(),
                problems,
                unknown,
            });
        }
        TableKind::Zones => {
            let pool = pool_channels(cp);
            let old = cp.zones.clone();
            let mut taken = vec![false; old.len()];
            let mut out: Vec<Zone> = Vec::new();
            for (i, row) in rows.iter().enumerate().skip(1) {
                if row.iter().all(|v| v.trim().is_empty()) {
                    continue;
                }
                let name_text = cell!(row, "name").unwrap_or("").trim().to_string();
                let idx = (!name_text.is_empty())
                    .then(|| {
                        old.iter()
                            .enumerate()
                            .position(|(k, x)| !taken[k] && x.name == name_text)
                    })
                    .flatten();
                let fresh = idx.is_none();
                let mut work = match idx {
                    Some(k) => {
                        taken[k] = true;
                        old[k].clone()
                    }
                    None => Zone {
                        slot: 0,
                        name: String::new(),
                        channels: Vec::new(),
                    },
                };
                let mut skip = false;
                for (c, ix) in &found {
                    let s = row.get(*ix).map_or("", |s| s.as_str());
                    match c.key {
                        "name" => match as_name(s, 16, true) {
                            Ok(v) => work.name = v,
                            Err(e) => {
                                problems.push(CsvProblem {
                                    row: i + 1,
                                    column: c.header.into(),
                                    message: e.0,
                                    skipped: fresh,
                                });
                                skip |= fresh;
                            }
                        },
                        "channels" => {
                            let mut msgs = Vec::new();
                            work.channels = find_refs(s, &pool, 80, &mut msgs);
                            for m in msgs {
                                problems.push(CsvProblem {
                                    row: i + 1,
                                    column: c.header.into(),
                                    message: m,
                                    skipped: false,
                                });
                            }
                        }
                        _ => {}
                    }
                }
                if skip || work.name.is_empty() {
                    continue;
                }
                if fresh {
                    added += 1;
                } else {
                    updated += 1;
                }
                work.slot = out.len();
                out.push(work);
            }
            let removed = taken.iter().filter(|t| !**t).count();
            cp.zones = out;
            Ok(ImportResult {
                table: t,
                added,
                updated,
                removed,
                problems,
                unknown,
            })
        }
    }
}

/// Rows the file numbered twice keep the first; the rest, and rows without a number, get free numbers.
fn assign_numbers(nums: &mut [&mut u16], max: u16) {
    let mut used = std::collections::HashSet::new();
    let mut need: Vec<usize> = Vec::new();
    for (i, n) in nums.iter().enumerate() {
        if **n >= 1 && **n <= max && used.insert(**n) {
            continue;
        }
        need.push(i);
    }
    let mut next = 1u16;
    for i in need {
        while used.contains(&next) && next <= max {
            next += 1;
        }
        *nums[i] = next;
        used.insert(next);
    }
}

/// Several tables (a folder of CSV files): `(name, text)` in dependency order. Files that match no table are listed.
pub fn import_all(
    cp: &mut Codeplug,
    files: &[(String, String)],
) -> (Vec<(String, ImportResult)>, Vec<String>) {
    let tagged: Vec<(&(String, String), Option<TableKind>)> =
        files.iter().map(|f| (f, detect(&f.1))).collect();
    let mut results = Vec::new();
    for t in ORDER {
        for (f, k) in &tagged {
            if *k == Some(t) {
                if let Ok(r) = import_table(cp, &f.1, Some(t)) {
                    results.push((f.0.clone(), r));
                }
            }
        }
    }
    let skipped = tagged
        .iter()
        .filter(|(_, k)| k.is_none())
        .map(|(f, _)| f.0.clone())
        .collect();
    (results, skipped)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Image;

    fn sample() -> Codeplug {
        let mut cp = Codeplug::decode(&Image::blank());
        cp.contacts.push(Contact {
            number: 1,
            name: "Local".into(),
            id: 2,
            call_type: CallType::Group,
            timeslot: 0,
        });
        cp.contacts.push(Contact {
            number: 2,
            name: "Friend".into(),
            id: 3_100_999,
            call_type: CallType::Private,
            timeslot: 2,
        });
        cp.rx_lists.push(RxList {
            number: 1,
            name: "FAV".into(),
            contacts: vec![1, 2],
        });
        let mut a = Channel::new(1, 146_805_000, 146_205_000);
        a.name = "Rpt, \"2m\"".into();
        a.rx_tone = "100.0".into();
        a.tx_tone = "D265N".into();
        let mut b = Channel::new(2, 445_720_000, 440_720_000);
        b.name = "DMR One".into();
        b.mode = Mode::Dmr;
        b.contact = 2;
        b.rx_list = 1;
        b.color_code = 3;
        b.timeslot = 2;
        cp.channels = vec![a, b];
        cp.zones.push(Zone {
            slot: 0,
            name: "Main".into(),
            channels: vec![1, 2],
        });
        cp
    }

    #[test]
    fn parse_handles_quotes_delimiters_and_blank_lines() {
        let rows = parse("\u{feff}a;b;c\r\n1;\"x;y\";\"he said \"\"hi\"\"\"\r\n\r\n");
        assert_eq!(
            rows,
            vec![vec!["a", "b", "c"], vec!["1", "x;y", "he said \"hi\""]]
        );
        assert_eq!(parse("a,b\n1,2"), vec![vec!["a", "b"], vec!["1", "2"]]);
        assert_eq!(parse("a\tb\n1\t2"), vec![vec!["a", "b"], vec!["1", "2"]]);
        let text = stringify(&[vec!["a,b".into(), " x".into(), "q\"".into()]]);
        assert_eq!(text, "\"a,b\",\" x\",\"q\"\"\"\r\n");
        assert_eq!(parse(&text), vec![vec!["a,b", " x", "q\""]]);
    }

    #[test]
    fn frequencies_are_written_and_read_as_the_web_editor_does() {
        assert_eq!(mhz(146_520_000), "146.520");
        assert_eq!(mhz(446_006_250), "446.00625");
        assert_eq!(mhz(146_805_000), "146.805");
        assert_eq!(parse_freq("446.00625"), Some(446_006_250));
        assert_eq!(parse_freq("446006250"), Some(446_006_250));
        assert_eq!(parse_freq("146,5"), None); // the caller turns a decimal comma into a point first
        assert_eq!(parse_freq("abc"), None);
        assert_eq!(parse_freq("0"), None);
    }

    #[test]
    fn every_table_exports_and_imports_back() {
        let cp = sample();
        let mut other = Codeplug::decode(&Image::blank());
        let files: Vec<(String, String)> = export_all(&cp)
            .into_iter()
            .map(|(n, t)| (n.to_string(), t))
            .collect();
        let (results, skipped) = import_all(&mut other, &files);
        assert!(skipped.is_empty());
        assert!(
            results.iter().all(|(_, r)| r.problems.is_empty()),
            "{results:?}"
        );
        assert_eq!(other.contacts, cp.contacts);
        assert_eq!(other.rx_lists, cp.rx_lists);
        assert_eq!(other.channels.len(), 2);
        for (a, b) in other.channels.iter().zip(&cp.channels) {
            assert_eq!(
                (&a.name, a.rx_hz, a.tx_hz, a.mode, &a.rx_tone, &a.tx_tone),
                (&b.name, b.rx_hz, b.tx_hz, b.mode, &b.rx_tone, &b.tx_tone)
            );
            assert_eq!(
                (a.contact, a.rx_list, a.color_code, a.timeslot),
                (b.contact, b.rx_list, b.color_code, b.timeslot)
            );
        }
        assert_eq!(
            other
                .zones
                .iter()
                .map(|z| (z.name.clone(), z.channels.clone()))
                .collect::<Vec<_>>(),
            vec![("Main".to_string(), vec![1, 2])]
        );
    }

    #[test]
    fn detects_the_table_from_the_header() {
        assert_eq!(
            detect("Name,RX Frequency\nA,146.52\n"),
            Some(TableKind::Channels)
        );
        assert_eq!(
            detect("Name,ID,Call Type\nA,1,Group\n"),
            Some(TableKind::Contacts)
        );
        assert_eq!(detect("Name,Channels\nZ,A|B\n"), Some(TableKind::Zones));
        assert_eq!(detect("foo,bar\n1,2\n"), None);
    }

    #[test]
    fn a_partial_file_changes_only_its_columns_and_matches_by_name() {
        let mut cp = sample();
        let r = import_table(
            &mut cp,
            "Name,RX Tone\nDMR One,88.5\nNew one,\n",
            Some(TableKind::Channels),
        )
        .unwrap();
        // "DMR One" kept its number and its other settings; the channel not in the file is gone; a new one was made
        assert_eq!((r.added, r.updated, r.removed), (1, 1, 1));
        assert_eq!(cp.channels.len(), 2);
        let one = cp.channels.iter().find(|c| c.name == "DMR One").unwrap();
        assert_eq!(
            (one.number, one.rx_tone.as_str(), one.rx_hz, one.color_code),
            (2, "88.5", 445_720_000, 3)
        );
        // the zone lost the removed channel
        assert_eq!(cp.zones[0].channels, vec![2]);
    }

    #[test]
    fn a_new_row_without_a_usable_required_value_is_skipped() {
        let mut cp = sample();
        let r = import_table(
            &mut cp,
            "Name,RX Frequency\nBad,abc\nGood,146.52\n",
            Some(TableKind::Channels),
        )
        .unwrap();
        assert!(r
            .problems
            .iter()
            .any(|p| p.skipped && p.row == 2 && p.column == "RX Frequency (MHz)"));
        assert!(
            cp.channels.iter().any(|c| c.name == "Good")
                && !cp.channels.iter().any(|c| c.name == "Bad")
        );
    }

    #[test]
    fn references_resolve_by_name_or_number_and_bad_ones_are_reported() {
        let mut cp = sample();
        let csv = "Name,RX Frequency (MHz),Mode,TX Contact\nNew DMR,446.0,DMR,Friend\nAnother,446.1,DMR,#1\nBad,446.2,DMR,Nobody\n";
        let r = import_table(&mut cp, csv, Some(TableKind::Channels)).unwrap();
        assert_eq!(r.added, 3);
        let by = |n: &str| cp.channels.iter().find(|c| c.name == n).unwrap().clone();
        assert_eq!(
            (
                by("New DMR").contact,
                by("Another").contact,
                by("Bad").contact
            ),
            (2, 1, 0)
        );
        assert!(r
            .problems
            .iter()
            .any(|p| p.column == "TX Contact" && p.message.contains("Nobody")));
        // a new channel's TX follows RX when the file has no TX column
        assert_eq!(by("New DMR").tx_hz, 446_000_000);
    }

    #[test]
    fn deleting_a_contact_clears_its_references() {
        let mut cp = sample();
        import_table(&mut cp, "Name,ID\nLocal,2\n", Some(TableKind::Contacts)).unwrap();
        assert_eq!(cp.contacts.len(), 1);
        assert_eq!(cp.channels[1].contact, 0);
        assert_eq!(cp.rx_lists[0].contacts, vec![1]);
    }

    #[test]
    fn duplicate_numbers_keep_the_first_and_renumber_the_rest() {
        let mut cp = Codeplug::decode(&Image::blank());
        import_table(
            &mut cp,
            "Number,Name,RX Frequency\n5,A,146.52\n5,B,146.53\n,C,146.54\n",
            Some(TableKind::Channels),
        )
        .unwrap();
        let nums: Vec<u16> = cp.channels.iter().map(|c| c.number).collect();
        assert_eq!(nums[0], 5);
        let mut sorted = nums.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), 3);
    }

    #[test]
    fn a_file_that_is_no_table_is_refused() {
        let mut cp = sample();
        assert!(import_table(&mut cp, "foo,bar\n1,2\n", None).is_err());
    }
}
