// SPDX-License-Identifier: MIT
// Copyright (c) 2026 The RadioFoundry Authors
// Copyright (c) 2026 insideTheFlux
//
// qdmr's YAML codeplug (RadioFoundry's docs/spec/qdmr-yaml.md) into the codeplug, and back. An import replaces the
// channel, zone, contact and RX group list tables and the general settings qdmr has, as one change; an export writes
// what the codeplug holds in the form qdmr 0.15 writes. Both report notes: what the other side can't hold. Ported from
// RadioFoundry's `site/static/js/codeplug/qdmr.js`. The mapping comes from qdmr's published manual and from files qdmr
// writes and reads, never from its source. APRS configurations and DTMF contacts are not part of this editor's model
// yet, so an import counts them in the notes and an export leaves them out.

use crate::fields::parse_tone;
use crate::model::Codeplug;
use crate::records::{
    Aprs, CallType, Channel, Contact, DtmfContact, Mode, RxList, Zone, APRS_COMMENT_MAX, APRS_NAME_MAX, MAX_APRS,
    MAX_CHANNELS, MAX_CONTACTS, MAX_DTMF, MAX_RX_LISTS,
    MAX_ZONES,
};
use crate::yaml::{parse, stringify, text_of, Value};

pub const QDMR_VERSION: &str = "0.15.1";

const NAME_MAX: usize = 16;
const RADIO_NAME_MAX: usize = 8;
const BOOT_MAX: usize = 16;
const RX_LIST_CONTACTS: usize = 32;
const ZONE_CHANNELS: usize = 80;
const ALL_CALL_ID: u32 = 0xFF_FFFF;

/// Power: qdmr's level names and the channel's power byte (open-firmware-cps.md 7.3.3).
const POWER: [(&str, u8); 5] = [("Min", 1), ("Low", 3), ("Mid", 6), ("High", 9), ("Max", 10)];
const ALIAS: [&str; 4] = ["None", "APRS", "Text", "Both"];
/// qdmr's APRS icon names and the symbol each stands for in the primary table (`/`), as qdmr encodes them.
const ICONS: &[(&str, char)] = &[
    ("PoliceStation", '!'),
    ("None", '"'),
    ("Digipeater", '#'),
    ("Phone", '$'),
    ("DXCluster", '%'),
    ("HFGateway", '&'),
    ("SmallPlane", '\''),
    ("MobileSatelliteStation", '('),
    ("WheelChair", ')'),
    ("Snowmobile", '*'),
    ("RedCross", '+'),
    ("BoyScout", ','),
    ("Home", '-'),
    ("X", '.'),
    ("RedDot", '/'),
    ("Circle0", '0'),
    ("Circle1", '1'),
    ("Circle2", '2'),
    ("Circle3", '3'),
    ("Circle4", '4'),
    ("Circle5", '5'),
    ("Circle6", '6'),
    ("Circle7", '7'),
    ("Circle8", '8'),
    ("Circle9", '9'),
    ("Fire", ':'),
    ("Campground", ';'),
    ("Motorcycle", '<'),
    ("RailEngine", '='),
    ("Car", '>'),
    ("FileServer", '?'),
    ("HCFuture", '@'),
    ("AidStation", 'A'),
    ("BBS", 'B'),
    ("Canoe", 'C'),
    ("Eyeball", 'E'),
    ("Tractor", 'F'),
    ("GridSquare", 'G'),
    ("Hotel", 'H'),
    ("TCPIP", 'I'),
    ("School", 'K'),
    ("Logon", 'L'),
    ("MacOS", 'M'),
    ("NTSStation", 'N'),
    ("Balloon", 'O'),
    ("PoliceCar", 'P'),
    ("TBD", 'Q'),
    ("RV", 'R'),
    ("Shuttle", 'S'),
    ("SSTV", 'T'),
    ("Bus", 'U'),
    ("ATV", 'V'),
    ("WXService", 'W'),
    ("Helo", 'X'),
    ("Yacht", 'Y'),
    ("Windows", 'Z'),
    ("Jogger", '['),
    ("Triangle", '\\'),
    ("PBBS", ']'),
    ("LargePlane", '^'),
    ("WXStation", '_'),
    ("DishAntenna", '`'),
    ("Ambulance", 'a'),
    ("Bike", 'b'),
    ("ICP", 'c'),
    ("FireStation", 'd'),
    ("Horse", 'e'),
    ("FireTruck", 'f'),
    ("Glider", 'g'),
    ("Hospital", 'h'),
    ("IOTA", 'i'),
    ("Jeep", 'j'),
    ("SmallTruck", 'k'),
    ("Laptop", 'l'),
    ("MicE", 'm'),
    ("Node", 'n'),
    ("EOC", 'o'),
    ("Rover", 'p'),
    ("Grid", 'q'),
    ("Antenna", 'r'),
    ("PowerBoat", 's'),
    ("TruckStop", 't'),
    ("TruckLarge", 'u'),
    ("Van", 'v'),
    ("Water", 'w'),
    ("XAPRS", 'x'),
    ("Yagi", 'y'),
    ("Shelter", 'z'),
];

/// The icon name for a symbol of the primary table.
fn icon_of(sym: char) -> Option<&'static str> {
    ICONS.iter().find(|(_, c)| *c == sym).map(|(n, _)| *n)
}


#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Note {
    // import
    Shortened,
    RadioIdType,
    ContactType,
    ListCut,
    DmrPositioning,
    PathCut,
    IconUnknown,
    AprsPeriod,
    ChannelType,
    BadFrequency,
    TotOff,
    ScanLists,
    BadTone,
    ZoneCut,
    Roaming,
    Encryption,
    TooManyChannels,
    TooManyContacts,
    TooManyLists,
    TooManyDtmf,
    TooManyAprs,
    TooManyZones,
    // export
    LocationRounded,
    AprsNoChannel,
    IconExport,
    AprsFlags,
    Sfr,
    ToneBurst,
    PowerLevel,
}

impl Note {
    pub fn text(self, n: usize) -> String {
        match self {
            Note::Shortened => format!("{n} name(s) were too long and were shortened."),
            Note::RadioIdType => format!("{n} radio ID(s) of an unknown type were left out."),
            Note::ContactType => format!("{n} contact(s) of an unknown type were left out."),
            Note::ListCut => format!("{n} RX group list(s) held more than {RX_LIST_CONTACTS} contacts; the rest were left out."),
            Note::DmrPositioning => format!("{n} DMR position reporting setting(s) were left out."),
            Note::PathCut => format!("{n} APRS configurations have more than two path entries; the first two were kept."),
            Note::IconUnknown => format!("{n} APRS icons have no counterpart; the car icon is used instead."),
            Note::AprsPeriod => format!("{n} APRS beacon periods: the radio sets the period in its own menu."),
            Note::ChannelType => format!("{n} channel(s) of an unknown type were left out."),
            Note::BadFrequency => format!("{n} channel(s) had no usable frequency and were left out."),
            Note::TotOff => format!("{n} channel(s) had the TX timeout off; they use the radio's own."),
            Note::ScanLists => format!("{n} scan list(s) were left out."),
            Note::BadTone => format!("{n} tone(s) the radio can't hold were left out."),
            Note::ZoneCut => format!("{n} zone(s) held more than {ZONE_CHANNELS} channels; the rest were left out."),
            Note::Roaming => "Roaming settings were left out.".into(),
            Note::Encryption => "Encryption keys were left out.".into(),
            Note::TooManyChannels => format!("{n} channel(s) did not fit and were left out."),
            Note::TooManyContacts => format!("{n} contact(s) did not fit and were left out."),
            Note::TooManyLists => format!("{n} RX group list(s) did not fit and were left out."),
            Note::TooManyDtmf => format!("{n} DTMF contacts beyond the 63 the radio holds."),
            Note::TooManyAprs => format!("{n} APRS configurations beyond the 8 the radio holds."),
            Note::TooManyZones => format!("{n} zone(s) did not fit and were left out."),
            Note::LocationRounded => format!("{n} channel location(s) were rounded to a six-character locator."),
            Note::AprsNoChannel => format!("{n} APRS frequencies with no FM channel on that frequency to name as the APRS channel."),
            Note::IconExport => format!("{n} APRS symbols that qdmr has no icon for; they were saved without an icon."),
            Note::AprsFlags => format!("{n} APRS configurations' silent beacon and position ambiguity settings."),
            Note::Sfr => format!("{n} channel(s) have SFR settings, which a qdmr file can't hold."),
            Note::ToneBurst => format!("{n} channel(s) have a 1750 Hz tone burst, which a qdmr file can't hold."),
            Note::PowerLevel => format!("{n} channel(s) have a power level qdmr has no name for; they were written as the radio's default."),
        }
    }
}

type Notes = Vec<(Note, usize)>;

fn add(notes: &mut Notes, n: Note, by: usize) {
    match notes.iter_mut().find(|(k, _)| *k == n) {
        Some((_, c)) => *c += by,
        None => notes.push((n, by)),
    }
}

#[derive(Clone, Debug, Default)]
pub struct Report {
    pub channels: usize,
    pub contacts: usize,
    pub rx_lists: usize,
    pub dtmf_contacts: usize,
    pub aprs: usize,
    pub zones: usize,
    pub notes: Vec<(Note, usize)>,
}

// --- Values ---

fn text(v: Option<&Value>) -> String {
    match v.map(Value::untag) {
        None | Some(Value::Null) => String::new(),
        Some(v) => text_of(v),
    }
}

/// JavaScript's `Number(x) || 0` for a scalar.
fn number(v: Option<&Value>) -> f64 {
    let n = match v.map(Value::untag) {
        Some(Value::Int(i)) => *i as f64,
        Some(Value::Float(f)) => *f,
        Some(Value::Bool(b)) => f64::from(u8::from(*b)),
        Some(Value::Str(s)) => {
            let t = s.trim();
            if t.is_empty() {
                0.0
            } else {
                t.parse::<f64>().unwrap_or(0.0)
            }
        }
        _ => 0.0,
    };
    if n.is_nan() {
        0.0
    } else {
        n
    }
}

/// The leading number of a text, as JavaScript's `parseFloat` reads it.
fn parse_float_prefix(s: &str) -> Option<f64> {
    let s = s.trim_start();
    let mut end = 0;
    let b = s.as_bytes();
    if end < b.len() && (b[end] == b'-' || b[end] == b'+') {
        end += 1;
    }
    let digits_start = end;
    while end < b.len() && b[end].is_ascii_digit() {
        end += 1;
    }
    if end < b.len() && b[end] == b'.' {
        end += 1;
        while end < b.len() && b[end].is_ascii_digit() {
            end += 1;
        }
    }
    if end == digits_start || s[digits_start..end] == *"." {
        return None;
    }
    s[..end].parse().ok()
}

/// A frequency in Hz on the 10 Hz grid: "439.5625 MHz", "145 kHz", or a bare number in MHz; `None` if not one.
pub fn parse_frequency(v: Option<&Value>) -> Option<u64> {
    let v = v?.untag();
    match v {
        Value::Int(_) | Value::Float(_) => {
            let n = number(Some(v));
            if n > 0.0 {
                Some((n * 1e5).round() as u64 * 10)
            } else {
                None
            }
        }
        Value::Str(s) => {
            let s = s.trim();
            let split = s
                .find(|c: char| !(c.is_ascii_digit() || c == '.'))
                .unwrap_or(s.len());
            let (num, unit) = s.split_at(split);
            let valid = !num.is_empty()
                && num.matches('.').count() <= 1
                && num.ends_with(|c: char| c.is_ascii_digit());
            if !valid {
                return None;
            }
            let scale = match unit.trim().to_lowercase().as_str() {
                "" | "mhz" => 1e6,
                "ghz" => 1e9,
                "khz" => 1e3,
                "hz" => 1.0,
                _ => return None,
            };
            let hz = ((num.parse::<f64>().ok()? * scale) / 10.0).round() as u64 * 10;
            (hz > 0 && hz < 10_000_000_000).then_some(hz)
        }
        _ => None,
    }
}

pub fn frequency_text(hz: u64) -> String {
    // five decimals (10 Hz), trailing zeros dropped
    let units = (hz + 5) / 10;
    let (whole, frac) = (units / 100_000, units % 100_000);
    let mut s = format!("{whole}.{frac:05}");
    while s.ends_with('0') {
        s.pop();
    }
    if s.ends_with('.') {
        s.pop();
    }
    format!("{s} MHz")
}

/// A duration in milliseconds: "3 min", "45 s", "500 ms", a bare number in seconds; 0 for off or infinite; `None` for
/// `!default` or nothing.
pub fn parse_duration(v: Option<&Value>) -> Option<f64> {
    let v = v?;
    if matches!(v, Value::Null) || v.is_default_tag() {
        return None;
    }
    match v.untag() {
        Value::Int(_) | Value::Float(_) => Some(number(Some(v)) * 1000.0),
        Value::Str(s) => {
            let s = s.trim();
            if matches!(
                s.to_lowercase().as_str(),
                "infinite" | "off" | "none" | "disabled" | "disable"
            ) {
                return Some(0.0);
            }
            let split = s
                .find(|c: char| !(c.is_ascii_digit() || c == '.'))
                .unwrap_or(s.len());
            let (num, unit) = s.split_at(split);
            let valid = !num.is_empty()
                && num.matches('.').count() <= 1
                && num.ends_with(|c: char| c.is_ascii_digit());
            if !valid {
                return None;
            }
            let scale = match unit.trim().to_lowercase().as_str() {
                "ms" => 1.0,
                "" | "s" | "sec" => 1000.0,
                "min" => 60_000.0,
                "h" => 3_600_000.0,
                _ => return None,
            };
            Some(num.parse::<f64>().ok()? * scale)
        }
        _ => None,
    }
}

fn seconds(ms: u64) -> String {
    if ms != 0 && ms.is_multiple_of(60_000) {
        format!("{} min", ms / 60_000)
    } else if ms.is_multiple_of(1000) {
        format!("{} s", ms / 1000)
    } else {
        format!("{ms} ms")
    }
}

fn round4(x: f64) -> f64 {
    (x * 10_000.0).round() / 10_000.0
}

/// Maidenhead locator (4, 6 or 8 characters) to the center of its square; `None` if not a locator.
pub fn from_locator(loc: &str) -> Option<(f64, f64)> {
    let s: Vec<char> = loc.trim().to_uppercase().chars().collect();
    let letter = |c: char, max: char| ('A'..=max).contains(&c);
    let valid = matches!(s.len(), 4 | 6 | 8)
        && letter(s[0], 'R')
        && letter(s[1], 'R')
        && s[2].is_ascii_digit()
        && s[3].is_ascii_digit()
        && (s.len() < 6 || (letter(s[4], 'X') && letter(s[5], 'X')))
        && (s.len() < 8 || (s[6].is_ascii_digit() && s[7].is_ascii_digit()));
    if !valid {
        return None;
    }
    let d = |c: char| f64::from(c as u32 - '0' as u32);
    let a = |c: char| f64::from(c as u32 - 'A' as u32);
    let mut lon = a(s[0]) * 20.0 - 180.0 + d(s[2]) * 2.0;
    let mut lat = a(s[1]) * 10.0 - 90.0 + d(s[3]);
    let (mut w, mut h) = (2.0, 1.0);
    if s.len() >= 6 {
        w = 2.0 / 24.0;
        h = 1.0 / 24.0;
        lon += a(s[4]) * w;
        lat += a(s[5]) * h;
    }
    if s.len() == 8 {
        w /= 10.0;
        h /= 10.0;
        lon += d(s[6]) * w;
        lat += d(s[7]) * h;
    }
    Some((round4(lat + h / 2.0), round4(lon + w / 2.0)))
}

/// The 6-character locator of a position.
pub fn to_locator(lat: f64, lon: f64) -> String {
    let x = (lon + 180.0).clamp(0.0, 359.9999);
    let y = (lat + 90.0).clamp(0.0, 179.9999);
    let up = |n: f64| char::from(b'A' + n as u8);
    let low = |n: f64| char::from(b'a' + n as u8);
    format!(
        "{}{}{}{}{}{}",
        up((x / 20.0).floor()),
        up((y / 10.0).floor()),
        ((x % 20.0) / 2.0).floor() as u8,
        (y % 10.0).floor() as u8,
        low(((x % 2.0) * 12.0).floor()),
        low(((y % 1.0) * 24.0).floor()),
    )
}

/// A sub-tone: `{ctcss: 67}` or `{ctcss: 67 Hz}`, `{dcs: 23}` (negative = inverted); `Some("")` for none, `None` if not
/// a tone.
fn parse_sub_tone(v: Option<&Value>) -> Option<String> {
    let v = match v {
        None => return Some(String::new()),
        Some(v) => v.untag(),
    };
    match v {
        Value::Null => return Some(String::new()),
        Value::Str(s) if s.is_empty() => return Some(String::new()),
        Value::Map(_) => {}
        _ => return None,
    }
    if let Some(c) = v.get("ctcss") {
        let f = parse_float_prefix(&text(Some(c)));
        return parse_tone(&match f {
            Some(f) => format!("{f:.1}"),
            None => "x".into(),
        });
    }
    if let Some(d) = v.get("dcs") {
        let raw = text(Some(d));
        let raw = raw.trim();
        let (sign, digits) = match raw.chars().next() {
            Some(c) if "-+niNI".contains(c) => (Some(c), &raw[1..]),
            _ => (None, raw),
        };
        let stripped = digits.trim_start_matches('0');
        let stripped = if stripped.is_empty() && !digits.is_empty() {
            "0"
        } else {
            stripped
        };
        if stripped.is_empty()
            || stripped.len() > 3
            || !stripped.chars().all(|c| ('0'..='7').contains(&c))
        {
            return None;
        }
        let inverted = sign == Some('-') || matches!(sign, Some('i' | 'I'));
        return Some(format!(
            "D{:0>3}{}",
            stripped,
            if inverted { 'I' } else { 'N' }
        ));
    }
    None
}

fn sub_tone(t: &str) -> Value {
    if t.is_empty() {
        return Value::Null;
    }
    if let Some(d) = t.strip_prefix('D') {
        let n: i64 = d.get(..3).and_then(|x| x.parse().ok()).unwrap_or(0);
        let n = if t.ends_with('I') { -n } else { n };
        return Value::flow(Value::Map(vec![("dcs".into(), Value::Int(n))]));
    }
    let f: f64 = t.parse().unwrap_or(0.0);
    Value::flow(Value::Map(vec![(
        "ctcss".into(),
        Value::str(format!("{f} Hz")),
    )]))
}

/// The single element of `{dmr: {...}}` and its type.
fn element(x: &Value) -> Option<(&str, &Value)> {
    match x.untag() {
        Value::Map(m) if m.len() == 1 && m[0].1.untag().is_map() => {
            Some((m[0].0.as_str(), &m[0].1))
        }
        _ => None,
    }
}

fn is_dmr(t: &str) -> bool {
    t == "dmr" || t == "digital"
}

fn shorten(s: String, max: usize, notes: &mut Notes) -> String {
    if s.chars().count() <= max {
        return s;
    }
    add(notes, Note::Shortened, 1);
    s.chars().take(max).collect()
}

fn power_of(name: &str) -> u8 {
    POWER
        .iter()
        .find(|(n, _)| *n == name)
        .map_or(0, |(_, v)| *v)
}

// --- Import ---

/// Replaces the codeplug's tables with a qdmr YAML file's. Nothing is changed if the file can't be read.
/// A source like `N0CALL-7`: the call sign and an SSID of 0-15 (0 when there is none or it is not one).
fn call_ssid(v: &str) -> u8 {
    let up = v.to_uppercase();
    let mut parts = up.split('-');
    parts.next();
    parts
        .next()
        .and_then(|s| s.trim().parse::<f64>().ok())
        .filter(|n| n.fract() == 0.0 && (0.0..=15.0).contains(n))
        .map_or(0, |n| n as u8)
}

pub fn import_qdmr(cp: &mut Codeplug, yaml: &str) -> Result<Report, String> {
    let doc = parse(yaml).map_err(|e| format!("That file is not valid YAML: {e}"))?;
    if !doc.is_map()
        || !(doc.get("channels").is_some_and(Value::truthy)
            || doc.get("contacts").is_some_and(Value::truthy)
            || doc.get("radioIDs").is_some_and(Value::truthy))
    {
        return Err("That file is not a qdmr codeplug (it has no channels, contacts or radio IDs).".into());
    }
    let mut notes: Notes = Vec::new();
    let list = |k: &str| doc.get(k).map_or(&[][..], Value::as_list);
    let empty = Value::Map(Vec::new());
    let obj = |k: &str| match doc.get(k) {
        Some(v) if v.is_map() => v,
        _ => &empty,
    };
    let settings = obj("settings");

    // Radio IDs: the default one is the radio's; another one a channel names becomes that channel's DMR ID
    struct RadioId {
        key: String,
        name: String,
        number: u32,
    }
    let mut ids: Vec<RadioId> = Vec::new();
    for x in list("radioIDs") {
        match element(x) {
            Some((t, v)) if is_dmr(t) => {
                let key = text(v.get("id"));
                let r = RadioId {
                    key: key.clone(),
                    name: text(v.get("name")),
                    number: number(v.get("number")) as u32,
                };
                // a later entry with the same id replaces the earlier, as a map would
                match ids.iter_mut().find(|i| i.key == key) {
                    Some(e) => *e = r,
                    None => ids.push(r),
                }
            }
            _ => add(&mut notes, Note::RadioIdType, 1),
        }
    }
    let default_key = text(settings.get("defaultID"));
    let def = ids
        .iter()
        .position(|i| i.key == default_key)
        .or(if ids.is_empty() { None } else { Some(0) });

    // Contacts
    let mut contacts: Vec<Contact> = Vec::new();
    let mut dtmf_contacts: Vec<DtmfContact> = Vec::new();
    let mut contact_by_id: Vec<(String, u16)> = Vec::new();
    for x in list("contacts") {
        let Some((t, v)) = element(x) else { continue };
        if is_dmr(t) {
            let call_type = match text(v.get("type")).as_str() {
                "GroupCall" => CallType::Group,
                "PrivateCall" => CallType::Private,
                "AllCall" => CallType::All,
                _ => {
                    add(&mut notes, Note::ContactType, 1);
                    continue;
                }
            };
            let ov = text(v.get("openGD77").and_then(|o| o.get("timeSlotOverride")));
            let n = contacts.len() + 1;
            let number = if n <= MAX_CONTACTS { n as u16 } else { 0 };
            let name = shorten(text(v.get("name")), NAME_MAX, &mut notes);
            let c = Contact {
                number,
                name,
                id: if call_type == CallType::All {
                    ALL_CALL_ID
                } else {
                    number_u32(v.get("number"))
                },
                call_type,
                timeslot: match ov.as_str() {
                    "TS1" => 1,
                    "TS2" => 2,
                    _ => 0,
                },
            };
            contact_by_id.push((text(v.get("id")), number));
            contacts.push(c);
        } else if t == "dtmf" {
            let name = shorten(text(v.get("name")), NAME_MAX, &mut notes);
            dtmf_contacts.push(DtmfContact { number: 0, name, code: text(v.get("number")).to_uppercase() });
        } else {
            add(&mut notes, Note::ContactType, 1);
        }
    }
    let contact_ref = |r: &str| {
        contact_by_id
            .iter()
            .rev()
            .find(|(id, _)| id == r)
            .map(|(_, n)| *n)
    };

    let mut rx_lists: Vec<RxList> = Vec::new();
    let mut list_by_id: Vec<(String, u8)> = Vec::new();
    for g in list("groupLists") {
        if !g.is_map() {
            continue;
        }
        let members: Vec<u16> = g
            .get("contacts")
            .map_or(&[][..], Value::as_list)
            .iter()
            .filter_map(|r| contact_ref(&text(Some(r))))
            .filter(|n| *n > 0)
            .collect();
        if members.len() > RX_LIST_CONTACTS {
            add(&mut notes, Note::ListCut, 1);
        }
        let n = rx_lists.len() + 1;
        let number = if n <= MAX_RX_LISTS { n as u8 } else { 0 };
        let name = shorten(text(g.get("name")), NAME_MAX, &mut notes);
        list_by_id.push((text(g.get("id")), number));
        rx_lists.push(RxList {
            number,
            name,
            contacts: members.into_iter().take(RX_LIST_CONTACTS).collect(),
        });
    }

    // Positioning: APRS systems become APRS configurations; DMR position reporting has no counterpart
    let fixed = settings
        .get("gnss")
        .filter(|g| g.is_map() && matches!(g.get("fixedPositionEnabled"), Some(Value::Bool(true))))
        .and_then(|g| from_locator(&text(g.get("fixedPosition"))));
    let mut aprs: Vec<Aprs> = Vec::new();
    let mut aprs_by_id: Vec<(String, u8)> = Vec::new();
    let mut reverts: Vec<(usize, String)> = Vec::new();
    for x in list("positioning") {
        let Some((t, v)) = element(x) else { continue };
        if t != "aprs" {
            add(&mut notes, Note::DmrPositioning, 1);
            continue;
        }
        let path: Vec<String> = v
            .get("path")
            .map_or(&[][..], Value::as_list)
            .iter()
            .map(|p| text(Some(p)).to_uppercase())
            .collect();
        if path.len() > 2 {
            add(&mut notes, Note::PathCut, 1);
        }
        let icon_name = text(v.get("icon"));
        let icon = ICONS.iter().find(|(n, _)| *n == icon_name).map(|(_, c)| *c);
        if v.get("icon").is_some_and(|i| !matches!(i, Value::Null)) && icon.is_none() {
            add(&mut notes, Note::IconUnknown, 1);
        }
        let empty_o = Value::Map(Vec::new());
        let o = v
            .get("opengd77")
            .filter(|o| o.is_map())
            .or_else(|| v.get("openGD77").filter(|o| o.is_map()))
            .unwrap_or(&empty_o);
        let n = aprs.len() + 1;
        let apn = if n <= MAX_APRS { n as u8 } else { 0 };
        let mut a = Aprs::new(apn, "");
        a.name = shorten(text(v.get("name")), APRS_NAME_MAX, &mut notes);
        a.ssid = call_ssid(&text(v.get("source")));
        a.lat = fixed.map_or(0.0, |f| f.0);
        a.lon = fixed.map_or(0.0, |f| f.1);
        a.path1 = path.first().cloned().unwrap_or_default();
        a.path2 = path.get(1).cloned().unwrap_or_default();
        a.sym_code = icon.filter(|c| *c != '"').unwrap_or('>');
        a.comment = shorten(text(v.get("message")), APRS_COMMENT_MAX, &mut notes);
        a.baud300 = number(o.get("baud")) == 300.0;
        a.use_fixed = fixed.is_some();
        a.qsy = matches!(o.get("transmitQSY"), Some(Value::Bool(true)));
        if parse_duration(v.get("period")).is_some_and(|d| d != 0.0) {
            add(&mut notes, Note::AprsPeriod, 1);
        }
        aprs_by_id.push((text(v.get("id")), apn));
        reverts.push((aprs.len(), text(v.get("revert"))));
        aprs.push(a);
    }

    // Channels
    let audio = settings.get("audio").filter(|a| a.is_map());
    let vox_on = |v: Option<&Value>| -> bool {
        match v {
            None => audio_vox(audio),
            Some(v) if v.is_default_tag() || matches!(v, Value::Null) => audio_vox(audio),
            Some(v) => match v.untag() {
                Value::Int(_) | Value::Float(_) => number(Some(v)) > 0.0,
                Value::Bool(b) => *b,
                _ => false,
            },
        }
    };
    let mut channels: Vec<Channel> = Vec::new();
    let mut channel_by_id: Vec<(String, u16)> = Vec::new();
    let mut channel_tx: Vec<(String, u64)> = Vec::new();
    for x in list("channels") {
        let Some((t, v)) = element(x) else { continue };
        let dmr = is_dmr(t);
        if !dmr && t != "fm" && t != "analog" {
            add(&mut notes, Note::ChannelType, 1);
            continue;
        }
        let rx_hz = parse_frequency(v.get("rxFrequency"));
        let tx_hz = parse_frequency(
            v.get("txFrequency")
                .filter(|f| !matches!(f, Value::Null))
                .or(v.get("rxFrequency")),
        );
        let (Some(rx_hz), Some(tx_hz)) = (rx_hz, tx_hz) else {
            add(&mut notes, Note::BadFrequency, 1);
            continue;
        };
        let empty_o = Value::Map(Vec::new());
        let o = v.get("openGD77").filter(|o| o.is_map()).unwrap_or(&empty_o);
        let loc = from_locator(&text(o.get("location")));
        let tot = parse_duration(v.get("timeout"));
        // no per-channel "never": an explicit infinite or 0 takes the radio's TX timeout, like !default
        if tot == Some(0.0) {
            add(&mut notes, Note::TotOff, 1);
        }
        let n = channels.len() + 1;
        let mut c = Channel::new(if n <= MAX_CHANNELS { n as u16 } else { 0 }, rx_hz, tx_hz);
        c.name = shorten(text(v.get("name")), NAME_MAX, &mut notes);
        c.mode = if dmr { Mode::Dmr } else { Mode::Fm };
        c.power = power_of(&text(v.get("power")));
        c.tot = match tot {
            Some(ms) if ms > 0.0 => ((ms / 15_000.0).ceil() as u64 * 15).min(255 * 15) as u16,
            _ => 0,
        };
        c.rx_tone = String::new();
        c.tx_tone = String::new();
        c.bw = 12.5;
        c.rx_only = matches!(v.get("rxOnly"), Some(Value::Bool(true)));
        c.all_skip = matches!(o.get("scanAllSkip"), Some(Value::Bool(true)));
        c.zone_skip = matches!(o.get("scanZoneSkip"), Some(Value::Bool(true)));
        c.vox = vox_on(v.get("vox"));
        c.squelch = 0;
        c.color_code = 0;
        c.timeslot = 1;
        c.no_beep = matches!(o.get("beep"), Some(Value::Bool(false)));
        c.no_eco = matches!(o.get("powerSave"), Some(Value::Bool(false)));
        c.alias_ts1 = ALIAS
            .iter()
            .position(|a| *a == text(o.get("talkerAliasTS1")))
            .unwrap_or(0) as u8;
        c.alias_ts2 = ALIAS
            .iter()
            .position(|a| *a == text(o.get("talkerAliasTS2")))
            .unwrap_or(0) as u8;
        c.use_location = loc.is_some();
        c.lat = loc.map_or(0.0, |l| l.0);
        c.lon = loc.map_or(0.0, |l| l.1);
        if v.get("scanList").is_some_and(|s| !matches!(s, Value::Null)) {
            add(&mut notes, Note::ScanLists, 1);
        }
        if dmr {
            c.color_code = number(v.get("colorCode")).clamp(0.0, 15.0) as u8;
            c.timeslot = if text(v.get("timeSlot")) == "TS2" { 2 } else { 1 };
            c.contact = contact_ref(&text(v.get("contact"))).unwrap_or(0);
            c.rx_list = list_by_id
                .iter()
                .rev()
                .find(|(id, _)| *id == text(v.get("groupList")))
                .map_or(0, |(_, n)| *n);
            let id = v.get("radioId").or(v.get("radioID"));
            if let Some(id) = id {
                if !matches!(id, Value::Null) && !id.is_default_tag() {
                    let key = text(Some(id));
                    if let Some(i) = ids.iter().position(|r| r.key == key) {
                        if Some(i) != def {
                            c.dmr_id = ids[i].number;
                        }
                    }
                }
            }
            c.roam = v.get("roaming").is_some_and(|r| !matches!(r, Value::Null));
            c.dmo = matches!(
                v.get("extended").and_then(|e| e.get("talkaround")),
                Some(Value::Bool(true))
            );
        } else {
            c.bw = if text(v.get("bandwidth")) == "Wide" { 25.0 } else { 12.5 };
            for (key, dst) in [("rxTone", &mut c.rx_tone), ("txTone", &mut c.tx_tone)] {
                match parse_sub_tone(v.get(key)) {
                    Some(t) => *dst = t,
                    None => add(&mut notes, Note::BadTone, 1),
                }
            }
            if let Some(sq @ (Value::Int(_) | Value::Float(_))) = v.get("squelch").map(Value::untag)
            {
                let n = number(Some(sq));
                if n > 0.0 {
                    c.squelch = (n * 2.0).min(21.0) as u8;
                }
            }
        }
        let aprs_id = text(v.get("aprs"));
        if let Some((_, n)) = aprs_by_id.iter().rev().find(|(id, _)| *id == aprs_id) {
            c.aprs = *n;
        }
        channel_by_id.push((text(v.get("id")), c.number));
        channel_tx.push((text(v.get("id")), c.tx_hz));
        channels.push(c);
    }
    // An APRS configuration's frequency is its revert channel's.
    for (i, id) in &reverts {
        if let Some((_, hz)) = channel_tx.iter().rev().find(|(cid, _)| cid == id) {
            aprs[*i].tx_hz = *hz;
        }
    }

    // Zones: qdmr's A and B lists; a zone with both becomes two, "<name> A" and "<name> B", as qdmr writes them here
    let mut zones: Vec<Zone> = Vec::new();
    for z in list("zones") {
        if !z.is_map() {
            continue;
        }
        let pick = |k: &str| -> Vec<u16> {
            z.get(k)
                .map_or(&[][..], Value::as_list)
                .iter()
                .filter_map(|r| {
                    let r = text(Some(r));
                    channel_by_id
                        .iter()
                        .rev()
                        .find(|(id, _)| *id == r)
                        .map(|(_, n)| *n)
                })
                .filter(|n| *n > 0)
                .collect()
        };
        let (a, b) = (pick("A"), pick("B"));
        let name = text(z.get("name"));
        if !b.is_empty() {
            for (suffix, chans) in [("A", a), ("B", b)] {
                let nm = shorten(format!("{name} {suffix}"), NAME_MAX, &mut notes);
                zones.push(Zone {
                    slot: 0,
                    name: nm,
                    channels: chans,
                });
            }
        } else {
            let nm = shorten(name, NAME_MAX, &mut notes);
            zones.push(Zone {
                slot: 0,
                name: nm,
                channels: a,
            });
        }
    }
    for z in &mut zones {
        if z.channels.len() > ZONE_CHANNELS {
            add(&mut notes, Note::ZoneCut, 1);
            z.channels.truncate(ZONE_CHANNELS);
        }
    }

    for (key, note) in [("scanLists", Note::ScanLists), ("roaming", Note::Roaming)] {
        let any = match doc.get(key).map(Value::untag) {
            Some(Value::List(l)) => !l.is_empty(),
            Some(Value::Map(m)) => m.iter().any(|(_, x)| match x.untag() {
                Value::List(l) => !l.is_empty(),
                other => other.truthy(),
            }),
            _ => false,
        };
        if any {
            add(&mut notes, note, 1);
        }
    }
    if doc
        .get("commercial")
        .and_then(|c| c.get("encryptionKeys"))
        .is_some_and(|k| !k.as_list().is_empty())
    {
        add(&mut notes, Note::Encryption, 1);
    }

    // Into the codeplug: numbers in file order, so the radio shows the channels as qdmr lists them
    if channels.len() > MAX_CHANNELS {
        add(&mut notes, Note::TooManyChannels, channels.len() - MAX_CHANNELS);
        channels.truncate(MAX_CHANNELS);
    }
    if contacts.len() > MAX_CONTACTS {
        add(&mut notes, Note::TooManyContacts, contacts.len() - MAX_CONTACTS);
        contacts.truncate(MAX_CONTACTS);
    }
    if rx_lists.len() > MAX_RX_LISTS {
        add(&mut notes, Note::TooManyLists, rx_lists.len() - MAX_RX_LISTS);
        rx_lists.truncate(MAX_RX_LISTS);
    }
    if dtmf_contacts.len() > MAX_DTMF {
        add(&mut notes, Note::TooManyDtmf, dtmf_contacts.len() - MAX_DTMF);
        dtmf_contacts.truncate(MAX_DTMF);
    }
    if aprs.len() > MAX_APRS {
        add(&mut notes, Note::TooManyAprs, aprs.len() - MAX_APRS);
        aprs.truncate(MAX_APRS);
    }
    for (i, d) in dtmf_contacts.iter_mut().enumerate() {
        d.number = (i + 1) as u8;
    }
    if zones.len() > MAX_ZONES {
        add(&mut notes, Note::TooManyZones, zones.len() - MAX_ZONES);
        zones.truncate(MAX_ZONES);
    }
    for (i, z) in zones.iter_mut().enumerate() {
        z.slot = i;
    }
    let report = Report {
        channels: channels.len(),
        contacts: contacts.len(),
        rx_lists: rx_lists.len(),
        dtmf_contacts: dtmf_contacts.len(),
        aprs: aprs.len(),
        zones: zones.len(),
        notes: Vec::new(),
    };
    cp.channels = channels;
    cp.contacts = contacts;
    cp.rx_lists = rx_lists;
    cp.dtmf_contacts = dtmf_contacts;
    cp.aprs = aprs;
    cp.zones = zones;
    // the VFOs keep their frequencies; references into the old tables can't stay
    for v in cp.vfo.iter_mut().flatten() {
        v.contact = 0;
        v.rx_list = 0;
        v.aprs = 0;
    }
    let g = &mut cp.general;
    if let Some(i) = def {
        g.dmr_id = ids[i].number;
        g.radio_name = shorten(ids[i].name.clone(), RADIO_NAME_MAX, &mut notes);
    }
    if let Some(boot) = settings.get("boot").filter(|b| b.is_map()) {
        if let Some(m) = boot.get("message1").filter(|m| !matches!(m, Value::Null)) {
            g.boot_line1 = shorten(text(Some(m)), BOOT_MAX, &mut notes);
        }
        if let Some(m) = boot.get("message2").filter(|m| !matches!(m, Value::Null)) {
            g.boot_line2 = shorten(text(Some(m)), BOOT_MAX, &mut notes);
        }
    }
    if let Some(d) = settings.get("dmr").filter(|d| d.is_map()) {
        let hang = |v: Option<&Value>| parse_duration(v).map(|ms| (ms / 500.0).round().min(255.0) as u8);
        g.group_hang = hang(d.get("groupCallHangTime")).unwrap_or(g.group_hang);
        g.private_hang = hang(d.get("privateCallHangTime")).unwrap_or(g.private_hang);
    }
    Ok(Report { notes, ..report })
}

fn audio_vox(audio: Option<&Value>) -> bool {
    match audio.and_then(|a| a.get("vox")) {
        None | Some(Value::Null) => false,
        Some(v) => text(Some(v)) != "off" && number(Some(v)) != 0.0,
    }
}

fn number_u32(v: Option<&Value>) -> u32 {
    number(v) as u32
}

// --- Export ---

fn map(items: Vec<(&str, Value)>) -> Value {
    Value::Map(items.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}

fn default_tag() -> Value {
    Value::tag("!default", Value::str(""))
}

/// qdmr YAML for the codeplug, and the notes on what it couldn't carry.
pub fn export_qdmr(cp: &Codeplug) -> (String, Vec<(Note, usize)>) {
    let mut notes: Notes = Vec::new();
    let g = &cp.general;

    // Radio IDs: the radio's own, then one per channel DMR ID
    let mut radio_ids = vec![map(vec![(
        "dmr",
        Value::flow(map(vec![
            ("id", Value::str("id1")),
            (
                "name",
                Value::str(if g.radio_name.is_empty() {
                    if g.dmr_id == 0 {
                        "ID".to_string()
                    } else {
                        g.dmr_id.to_string()
                    }
                } else {
                    g.radio_name.clone()
                }),
            ),
            ("number", Value::Int(i64::from(g.dmr_id))),
        ])),
    )])];
    let mut id_ref: Vec<(u32, String)> = Vec::new();
    for c in &cp.channels {
        if c.dmr_id == 0 || c.dmr_id == g.dmr_id || id_ref.iter().any(|(n, _)| *n == c.dmr_id) {
            continue;
        }
        let id = format!("id{}", radio_ids.len() + 1);
        id_ref.push((c.dmr_id, id.clone()));
        radio_ids.push(map(vec![(
            "dmr",
            Value::flow(map(vec![
                ("id", Value::str(id)),
                ("name", Value::str(c.dmr_id.to_string())),
                ("number", Value::Int(i64::from(c.dmr_id))),
            ])),
        )]));
    }

    // Positions in the tables, from 1, by record number
    let cref = |n: u16| -> Option<Value> {
        let i = cp.contacts.iter().position(|c| c.number == n && n != 0)?;
        Some(Value::str(format!("cont{}", i + 1)))
    };
    let mut contacts: Vec<Value> = cp
        .contacts
        .iter()
        .enumerate()
        .map(|(i, c)| {
            map(vec![(
                "dmr",
                Value::flow(map(vec![
                    ("id", Value::str(format!("cont{}", i + 1))),
                    ("name", Value::str(c.name.clone())),
                    ("ring", Value::Bool(false)),
                    (
                        "type",
                        Value::str(match c.call_type {
                            CallType::Group => "GroupCall",
                            CallType::Private => "PrivateCall",
                            CallType::All => "AllCall",
                        }),
                    ),
                    (
                        "number",
                        Value::Int(i64::from(if c.call_type == CallType::All {
                            ALL_CALL_ID
                        } else {
                            c.id
                        })),
                    ),
                    (
                        "openGD77",
                        Value::flow(map(vec![(
                            "timeSlotOverride",
                            Value::str(if c.timeslot == 0 {
                                "None".to_string()
                            } else {
                                format!("TS{}", c.timeslot)
                            }),
                        )])),
                    ),
                ])),
            )])
        })
        .collect();

    // DTMF contacts follow the digital contacts and continue their id count.
    for (i, d) in cp.dtmf_contacts.iter().enumerate() {
        contacts.push(map(vec![(
            "dtmf",
            Value::flow(map(vec![
                ("id", Value::str(format!("cont{}", cp.contacts.len() + i + 1))),
                ("name", Value::str(d.name.clone())),
                ("number", Value::str(d.code.clone())),
            ])),
        )]));
    }

    let group_lists: Vec<Value> = cp
        .rx_lists
        .iter()
        .enumerate()
        .map(|(i, l)| {
            Value::flow(map(vec![
                ("id", Value::str(format!("grp{}", i + 1))),
                ("name", Value::str(l.name.clone())),
                (
                    "contacts",
                    Value::flow(Value::List(
                        l.contacts.iter().filter_map(|n| cref(*n)).collect(),
                    )),
                ),
            ]))
        })
        .collect();

    let chan_ref = |n: u16| -> Option<String> {
        let i = cp.channels.iter().position(|c| c.number == n && n != 0)?;
        Some(format!("ch{}", i + 1))
    };

    let aprs_ref = |n: u8| -> Option<Value> {
        let i = cp.aprs.iter().position(|a| a.number == n && n != 0)?;
        Some(Value::str(format!("aprs{}", i + 1)))
    };

    let channels: Vec<Value> = cp
        .channels
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let og = vec![
                ("scanZoneSkip", Value::Bool(c.zone_skip)),
                ("scanAllSkip", Value::Bool(c.all_skip)),
                ("beep", Value::Bool(!c.no_beep)),
                ("powerSave", Value::Bool(!c.no_eco)),
                (
                    "location",
                    Value::str(if c.use_location {
                        to_locator(c.lat, c.lon)
                    } else {
                        String::new()
                    }),
                ),
                (
                    "talkerAliasTS1",
                    Value::str(ALIAS.get(usize::from(c.alias_ts1)).copied().unwrap_or("None")),
                ),
                (
                    "talkerAliasTS2",
                    Value::str(ALIAS.get(usize::from(c.alias_ts2)).copied().unwrap_or("None")),
                ),
            ];
            if c.use_location {
                add(&mut notes, Note::LocationRounded, 1);
            }
            if c.sfr || c.sfr_a != 0 || c.sfr_b != 0 {
                add(&mut notes, Note::Sfr, 1);
            }
            if c.tone_burst && c.mode != Mode::Dmr {
                add(&mut notes, Note::ToneBurst, 1);
            }
            let power_name = POWER.iter().find(|(_, v)| *v == c.power).map(|(n, _)| *n);
            if c.power != 0 && power_name.is_none() {
                add(&mut notes, Note::PowerLevel, 1);
            }
            let mut common = vec![
                ("id", Value::str(format!("ch{}", i + 1))),
                ("name", Value::str(c.name.clone())),
                ("rxFrequency", Value::str(frequency_text(c.rx_hz))),
                ("txFrequency", Value::str(frequency_text(c.tx_hz))),
                (
                    "timeout",
                    if c.tot != 0 {
                        Value::str(seconds(u64::from(c.tot) * 1000))
                    } else {
                        default_tag()
                    },
                ),
                ("rxOnly", Value::Bool(c.rx_only)),
                (
                    "vox",
                    if c.vox { Value::Int(1) } else { Value::str("off") },
                ),
                ("openGD77", map(og)),
            ];
            let power = power_name.map_or_else(default_tag, Value::str);
            if c.mode == Mode::Dmr {
                common.push(("admit", Value::str("Always")));
                common.push(("colorCode", Value::Int(i64::from(c.color_code))));
                common.push(("timeSlot", Value::str(format!("TS{}", c.timeslot))));
                common.push((
                    "radioId",
                    id_ref
                        .iter()
                        .find(|(n, _)| c.dmr_id != 0 && *n == c.dmr_id)
                        .map_or_else(default_tag, |(_, id)| Value::str(id.clone())),
                ));
                if c.rx_list != 0 {
                    if let Some(l) = cp.rx_lists.iter().position(|l| l.number == c.rx_list) {
                        common.push(("groupList", Value::str(format!("grp{}", l + 1))));
                    }
                }
                if let Some(v) = cref(c.contact) {
                    common.push(("contact", v));
                }
                if let Some(v) = aprs_ref(c.aprs) {
                    common.push(("aprs", v));
                }
                if c.roam {
                    common.push(("roaming", default_tag()));
                }
                common.push(("extended", map(vec![("talkaround", Value::Bool(c.dmo))])));
                common.push(("power", power));
                map(vec![("dmr", map(common))])
            } else {
                common.push((
                    "squelch",
                    if c.squelch != 0 {
                        Value::Int(i64::from(
                            (f64::from(c.squelch) / 2.0).round().clamp(1.0, 10.0) as u8,
                        ))
                    } else {
                        default_tag()
                    },
                ));
                common.push(("admit", Value::str("Always")));
                common.push(("rxTone", sub_tone(&c.rx_tone)));
                common.push(("txTone", sub_tone(&c.tx_tone)));
                common.push((
                    "bandwidth",
                    Value::str(if c.bw == 25.0 { "Wide" } else { "Narrow" }),
                ));
                if let Some(v) = aprs_ref(c.aprs) {
                    common.push(("aprs", v));
                }
                common.push(("power", power));
                map(vec![("fm", map(common))])
            }
        })
        .collect();

    let zones: Vec<Value> = cp
        .zones
        .iter()
        .enumerate()
        .map(|(i, z)| {
            map(vec![
                ("id", Value::str(format!("zone{}", i + 1))),
                ("name", Value::str(z.name.clone())),
                (
                    "A",
                    Value::flow(Value::List(
                        z.channels
                            .iter()
                            .filter_map(|n| chan_ref(*n).map(Value::str))
                            .collect(),
                    )),
                ),
                ("B", Value::flow(Value::List(Vec::new()))),
            ])
        })
        .collect();

    // APRS: the call comes from the radio name, the revert channel is an FM channel on the configuration's frequency
    let call: String = {
        let c: String = g
            .radio_name
            .to_uppercase()
            .chars()
            .filter(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
            .take(6)
            .collect();
        if c.is_empty() { "N0CALL".to_string() } else { c }
    };
    let positioning: Vec<Value> = cp
        .aprs
        .iter()
        .enumerate()
        .map(|(i, a)| {
            let revert = (a.tx_hz != 0)
                .then(|| cp.channels.iter().position(|c| c.mode == Mode::Fm && c.tx_hz == a.tx_hz))
                .flatten();
            if a.tx_hz != 0 && revert.is_none() {
                add(&mut notes, Note::AprsNoChannel, 1);
            }
            let icon = (a.sym_table == '/').then(|| icon_of(a.sym_code)).flatten();
            if icon.is_none() {
                add(&mut notes, Note::IconExport, 1);
            }
            if a.silent || a.ambiguity != 0 {
                add(&mut notes, Note::AprsFlags, 1);
            }
            let mut f = vec![
                ("id", Value::str(format!("aprs{}", i + 1))),
                ("name", Value::str(a.name.clone())),
            ];
            if let Some(k) = revert {
                f.push(("revert", Value::str(format!("ch{}", k + 1))));
            }
            f.push(("icon", Value::str(icon.unwrap_or("None"))));
            f.push(("message", Value::str(a.comment.clone())));
            f.push((
                "opengd77",
                map(vec![
                    ("transmitQSY", Value::Bool(a.qsy)),
                    ("baud", Value::Int(if a.baud300 { 300 } else { 1200 })),
                ]),
            ));
            f.push(("destination", Value::str("APN000-0")));
            f.push(("source", Value::str(format!("{call}-{}", a.ssid))));
            f.push((
                "path",
                Value::flow(Value::List(
                    [&a.path1, &a.path2].into_iter().filter(|p| !p.is_empty()).map(|p| Value::str(p.clone())).collect(),
                )),
            ));
            map(vec![("aprs", map(f))])
        })
        .collect();
    let fixed_aprs = cp.aprs.iter().find(|a| a.use_fixed);

    let mut settings = vec![
        ("defaultID", Value::str("id1")),
        (
            "boot",
            map(vec![
                ("message1", Value::str(g.boot_line1.clone())),
                ("message2", Value::str(g.boot_line2.clone())),
            ]),
        ),
    ];
    if let Some(a) = fixed_aprs {
        settings.push((
            "gnss",
            map(vec![
                ("fixedPositionEnabled", Value::Bool(true)),
                ("fixedPosition", Value::str(to_locator(a.lat, a.lon))),
            ]),
        ));
    }
    let mut dmr_set = Vec::new();
    if g.private_hang != 0 {
        dmr_set.push((
            "privateCallHangTime",
            Value::str(seconds(u64::from(g.private_hang) * 500)),
        ));
    }
    if g.group_hang != 0 {
        dmr_set.push((
            "groupCallHangTime",
            Value::str(seconds(u64::from(g.group_hang) * 500)),
        ));
    }
    if !dmr_set.is_empty() {
        settings.push(("dmr", map(dmr_set)));
    }

    let doc = map(vec![
        ("version", Value::str(QDMR_VERSION)),
        ("settings", map(settings)),
        ("radioIDs", Value::List(radio_ids)),
        ("contacts", Value::List(contacts)),
        ("groupLists", Value::List(group_lists)),
        ("channels", Value::List(channels)),
        ("zones", Value::List(zones)),
        ("positioning", Value::List(positioning)),
    ]);
    (format!("---\n{}...\n", stringify(&doc)), notes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Image;

    fn blank() -> Codeplug {
        let mut cp = Codeplug::decode(&Image::blank());
        cp.general.dmr_id = 3_100_123;
        cp.general.radio_name = "N0CALL".into();
        cp
    }

    const SAMPLE: &str = "---
version: 0.15.1
settings:
  defaultID: id1
  boot:
    message1: Hello
    message2: World
  dmr:
    groupCallHangTime: 3 s
radioIDs:
  - dmr: {id: id1, name: N0CALL, number: 3100123}
  - dmr: {id: id2, name: Club, number: 3100999}
contacts:
  - dmr: {id: cont1, name: Local, type: GroupCall, number: 9, openGD77: {timeSlotOverride: TS2}}
  - dmr: {id: cont2, name: Joe, type: PrivateCall, number: 3101234}
  - dtmf: {id: cont3, name: Gate, number: '123'}
groupLists:
  - {id: grp1, name: Net, contacts: [cont1, cont2]}
channels:
  - fm:
      id: ch1
      name: Simplex
      rxFrequency: 146.52 MHz
      txFrequency: 146.52 MHz
      power: High
      timeout: 1 min
      rxTone: {ctcss: 67 Hz}
      txTone: {dcs: -754}
      bandwidth: Wide
      squelch: 3
  - dmr:
      id: ch2
      name: Local TS2
      rxFrequency: 439.5625 MHz
      txFrequency: 434.5625 MHz
      colorCode: 1
      timeSlot: TS2
      contact: cont1
      groupList: grp1
      radioId: id2
      power: !default \"\"
zones:
  - {id: zone1, name: All, A: [ch1, ch2], B: []}
  - {id: zone2, name: Two, A: [ch1], B: [ch2]}
positioning:
  - aprs: {id: aprs1, name: APRS}
...
";

    #[test]
    fn a_qdmr_file_is_read_into_the_tables() {
        let mut cp = blank();
        let r = import_qdmr(&mut cp, SAMPLE).unwrap();
        assert_eq!((r.channels, r.contacts, r.rx_lists, r.zones), (2, 2, 1, 3));
        assert_eq!(cp.general.dmr_id, 3_100_123);
        assert_eq!(cp.general.radio_name, "N0CALL");
        assert_eq!(cp.general.boot_line1, "Hello");
        assert_eq!(cp.general.group_hang, 6);
        let (a, b) = (&cp.channels[0], &cp.channels[1]);
        assert_eq!((a.number, a.name.as_str(), a.rx_hz, a.tx_hz), (1, "Simplex", 146_520_000, 146_520_000));
        assert_eq!((a.power, a.tot, a.bw, a.squelch), (9, 60, 25.0, 6));
        assert_eq!((a.rx_tone.as_str(), a.tx_tone.as_str()), ("67.0", "D754I"));
        assert_eq!(a.mode, Mode::Fm);
        assert_eq!(b.mode, Mode::Dmr);
        assert_eq!((b.rx_hz, b.tx_hz), (439_562_500, 434_562_500));
        assert_eq!((b.color_code, b.timeslot, b.contact, b.rx_list, b.dmr_id), (1, 2, 1, 1, 3_100_999));
        assert_eq!(cp.contacts[0].timeslot, 2);
        assert_eq!(cp.contacts[1].call_type, CallType::Private);
        assert_eq!(cp.rx_lists[0].contacts, vec![1, 2]);
        assert_eq!(cp.zones[0].channels, vec![1, 2]);
        assert_eq!((cp.zones[1].name.as_str(), cp.zones[2].name.as_str()), ("Two A", "Two B"));
        // The DTMF contact and the APRS system come in now.
        assert_eq!((r.dtmf_contacts, r.aprs), (1, 1));
        assert_eq!((cp.dtmf_contacts[0].name.as_str(), cp.dtmf_contacts[0].code.as_str()), ("Gate", "123"));
        assert_eq!((cp.aprs[0].name.as_str(), cp.aprs[0].sym_code), ("APRS", '>'));
    }

    #[test]
    fn what_is_not_a_qdmr_file_is_refused_and_nothing_changes() {
        let mut cp = blank();
        cp.general.radio_name = "KEEP".into();
        assert!(import_qdmr(&mut cp, "a: [1\n").is_err());
        assert!(import_qdmr(&mut cp, "hello: world\n").is_err());
        assert!(import_qdmr(&mut cp, "- a\n- b\n").is_err());
        assert_eq!(cp.general.radio_name, "KEEP");
    }

    #[test]
    fn export_then_import_gives_the_same_tables() {
        let mut cp = blank();
        import_qdmr(&mut cp, SAMPLE).unwrap();
        let (text, _) = export_qdmr(&cp);
        let mut again = blank();
        let r = import_qdmr(&mut again, &text).unwrap();
        assert_eq!((r.channels, r.contacts, r.rx_lists, r.zones), (2, 2, 1, 3));
        assert_eq!(cp.channels, again.channels);
        assert_eq!(cp.contacts, again.contacts);
        assert_eq!(cp.rx_lists, again.rx_lists);
        assert_eq!(cp.zones, again.zones);
        assert_eq!(cp.general, again.general);
    }

    #[test]
    fn values_have_the_forms_qdmr_writes() {
        assert_eq!(frequency_text(439_562_500), "439.5625 MHz");
        assert_eq!(frequency_text(146_520_000), "146.52 MHz");
        assert_eq!(frequency_text(1_000_000), "1 MHz");
        assert_eq!(parse_frequency(Some(&Value::str("145500 kHz"))), Some(145_500_000));
        assert_eq!(parse_frequency(Some(&Value::Float(439.5625))), Some(439_562_500));
        assert_eq!(parse_frequency(Some(&Value::str("abc"))), None);
        assert_eq!(parse_duration(Some(&Value::str("3 min"))), Some(180_000.0));
        assert_eq!(parse_duration(Some(&Value::str("infinite"))), Some(0.0));
        assert_eq!(parse_duration(Some(&default_tag())), None);
        assert_eq!(seconds(180_000), "3 min");
        assert_eq!(seconds(45_000), "45 s");
        assert_eq!(seconds(1500), "1500 ms");
        assert_eq!(to_locator(37.77, -122.42), "CM87ss");
        let (lat, lon) = from_locator("CM87ss").unwrap();
        assert!((lat - 37.77).abs() < 0.05 && (lon + 122.42).abs() < 0.1, "{lat} {lon}");
        assert_eq!(from_locator("ZZ99"), None);
    }

    #[test]
    fn sub_tones_read_and_write() {
        let v = |s: &str| crate::yaml::parse(s).unwrap();
        assert_eq!(parse_sub_tone(Some(&v("{ctcss: 67}"))), Some("67.0".into()));
        assert_eq!(parse_sub_tone(Some(&v("{ctcss: 88.5 Hz}"))), Some("88.5".into()));
        assert_eq!(parse_sub_tone(Some(&v("{dcs: 23}"))), Some("D023N".into()));
        assert_eq!(parse_sub_tone(Some(&v("{dcs: -754}"))), Some("D754I".into()));
        assert_eq!(parse_sub_tone(Some(&v("{dcs: 89}"))), None);
        assert_eq!(parse_sub_tone(Some(&Value::Null)), Some(String::new()));
        assert_eq!(parse_sub_tone(None), Some(String::new()));
        assert_eq!(parse_sub_tone(Some(&Value::str("x"))), None);
        assert_eq!(crate::yaml::stringify(&sub_tone("D023N")), "{dcs: 23}\n");
        assert_eq!(crate::yaml::stringify(&sub_tone("D754I")), "{dcs: -754}\n");
        assert_eq!(crate::yaml::stringify(&sub_tone("100.0")), "{ctcss: 100 Hz}\n");
        assert_eq!(sub_tone(""), Value::Null);
    }

    #[test]
    fn export_reports_what_qdmr_cannot_hold() {
        let mut cp = blank();
        let mut c = Channel::new(1, 146_520_000, 146_520_000);
        c.sfr = true;
        c.power = 4;
        c.use_location = true;
        cp.channels.push(c);
        let (text, notes) = export_qdmr(&cp);
        let kinds: Vec<Note> = notes.iter().map(|(n, _)| *n).collect();
        assert!(kinds.contains(&Note::Sfr) && kinds.contains(&Note::PowerLevel) && kinds.contains(&Note::LocationRounded));
        assert!(text.starts_with("---\nversion: 0.15.1\n") && text.ends_with("...\n"));
        assert!(text.contains("power: !default \"\"\n"));
    }
}
