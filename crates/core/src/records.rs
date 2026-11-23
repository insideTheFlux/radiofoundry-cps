// SPDX-License-Identifier: MIT
// Copyright (c) 2026 The RadioFoundry Authors
// Copyright (c) 2026 insideTheFlux
//
// The open-firmware codeplug's records (open-firmware-cps.md 7.2-7.3), decoded into plain structs. Ported from
// RadioFoundry's `site/static/js/codeplug/records.js`. References between records (a channel's contact, a zone's
// channels) are 1-based numbers.

use crate::fields::{
    coord_bytes, get_bcd_id, get_coord, get_freq, get_name, get_tone, put16, put_bcd_id, put_freq,
    put_name, put_tone, u16le,
};
use crate::image::{EE, FL};

pub const MAX_CHANNELS: usize = 1024;
pub const MAX_ZONES: usize = 68;
pub const MAX_CONTACTS: usize = 1024;
pub const MAX_RX_LISTS: usize = 76;

pub const CH_SIZE: usize = 56;
pub const CH_PER_BANK: usize = 128;
const BANK0: usize = EE + 0x3780;
const BANK1: usize = FL + 0x7B1B0;
const BANK_STRIDE: usize = 16 + CH_PER_BANK * CH_SIZE;

pub const GENERAL: usize = EE + 0x00E0;
pub const GENERAL_SIZE: usize = 0x28;
pub const BOOT_TEXT: usize = EE + 0x7540;
pub const VFO: [usize; 2] = [EE + 0x7590, EE + 0x75C8];
pub const LUCZ: usize = EE + 0x6000;
pub const ZONE_BITMAP: usize = EE + 0x8010;
pub const ZONE_DATA: usize = EE + 0x8030;
pub const ZONE_RECORD: usize = 0xB0;
pub const ZONE_LAYOUT: usize = EE + 0x806F;
pub const CONTACTS: usize = FL + 0x87620;
pub const CONTACT_SIZE: usize = 24;
pub const LIST_COUNTS: usize = FL + 0x8D620;
pub const LISTS: usize = FL + 0x8D6A0;
pub const LIST_SIZE: usize = 80;
/// DTMF contacts (open-firmware-cps.md 7.3.7): 63 records of 32 bytes. (A 64th would overlap channel bank 0's bitmap.)
pub const DTMF_CONTACTS: usize = EE + 0x2F88;
pub const DTMF_SIZE: usize = 0x20;
pub const MAX_DTMF: usize = 63;
/// APRS configurations (7.3.8): 8 records of 64 bytes.
pub const APRS_BASE: usize = EE + 0x1588;
pub const APRS_SIZE: usize = 0x40;
pub const MAX_APRS: usize = 8;
/// The longest an APRS configuration's name and comment can be.
pub const APRS_NAME_MAX: usize = 8;
pub const APRS_COMMENT_MAX: usize = 23;

/// Address of a channel bank's in-use bitmap (16 bytes, then 128 records).
pub fn bank_bitmap(bank: usize) -> usize {
    if bank == 0 {
        BANK0
    } else {
        BANK1 + (bank - 1) * BANK_STRIDE
    }
}

/// Address of channel `n` (1-based).
pub fn channel_addr(n: usize) -> usize {
    bank_bitmap((n - 1) / CH_PER_BANK) + 16 + ((n - 1) % CH_PER_BANK) * CH_SIZE
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Fm,
    Dmr,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Channel {
    pub number: u16,
    pub name: String,
    pub rx_hz: u64,
    pub tx_hz: u64,
    pub mode: Mode,
    pub power: u8,
    pub tot: u16,
    pub rx_tone: String,
    pub tx_tone: String,
    pub bw: f64,
    pub rx_only: bool,
    pub all_skip: bool,
    pub zone_skip: bool,
    pub vox: bool,
    pub squelch: u8,
    pub color_code: u8,
    pub timeslot: u8,
    pub contact: u16,
    pub rx_list: u8,
    pub aprs: u8,
    pub dmr_id: u32,
    pub no_beep: bool,
    pub no_eco: bool,
    pub dmo: bool,
    pub roam: bool,
    pub lat: f64,
    pub lon: f64,
    pub use_location: bool,
    pub alias_ts1: u8,
    pub alias_ts2: u8,
    pub sfr: bool,
    pub sfr_a: u8,
    pub sfr_b: u8,
    pub tone_burst: bool,
    /// Accept a call with an invalid CRC (SFR byte B bits 5-4 = 2).
    pub accept_bad_crc: bool,
    /// What a live tag in the settings area makes of the channel (P25, NXDN, ...); `None` for a plain channel. A
    /// tagged channel's record is a safe stand-in, DMR and receive only (channel-modes.md 2.3).
    pub tagged: Option<crate::modes::Tagged>,
}

impl Channel {
    /// The contact the channel calls, if it has one.
    pub fn contact_ref<'a>(&self, contacts: &'a [Contact]) -> Option<&'a Contact> {
        (self.contact != 0).then(|| contacts.iter().find(|k| k.number == self.contact)).flatten()
    }

    /// A new FM channel: wide, radio-default power and squelch, the vendor defaults in the bytes no field covers.
    pub fn new(number: u16, rx_hz: u64, tx_hz: u64) -> Self {
        let mut r = blank_channel();
        put_freq(&mut r, 0x10, rx_hz);
        put_freq(&mut r, 0x14, tx_hz);
        let mut c = decode_channel(number, &r).expect("a blank channel with frequencies decodes");
        c.name = format!("Channel {number}");
        c.bw = 25.0;
        c.squelch = 0;
        c.color_code = 1;
        c.timeslot = 1;
        c
    }
}

/// A channel record (56 bytes); `None` if its frequencies are malformed or zero.
pub fn decode_channel(number: u16, r: &[u8]) -> Option<Channel> {
    let rx_hz = get_freq(r, 0x10)?;
    let tx_hz = get_freq(r, 0x14)?;
    let (l1, f4) = (r[0x26], r[0x33]);
    Some(Channel {
        number,
        name: get_name(r, 0, 16),
        rx_hz,
        tx_hz,
        mode: if r[0x18] == 1 { Mode::Dmr } else { Mode::Fm },
        power: r[0x19],
        tot: u16::from(r[0x1B]) * 15,
        rx_tone: get_tone(r, 0x20),
        tx_tone: get_tone(r, 0x22),
        bw: if f4 & 0x02 != 0 { 25.0 } else { 12.5 },
        rx_only: f4 & 0x04 != 0,
        all_skip: f4 & 0x10 != 0,
        zone_skip: f4 & 0x20 != 0,
        vox: f4 & 0x40 != 0,
        squelch: if r[0x37] <= 21 { r[0x37] } else { 10 },
        color_code: r[0x2C] & 0x0F,
        timeslot: if r[0x31] & 0x40 != 0 { 2 } else { 1 },
        contact: u16le(r, 0x2E),
        rx_list: r[0x2B],
        aprs: r[0x2D],
        dmr_id: if l1 & 0x80 != 0 {
            (u32::from(r[0x27]) << 16) | (u32::from(r[0x28]) << 8) | u32::from(r[0x29])
        } else {
            0
        },
        no_beep: l1 & 0x40 != 0,
        no_eco: l1 & 0x20 != 0,
        dmo: l1 & 0x04 != 0,
        roam: l1 & 0x01 != 0,
        lat: get_coord(r[0x1D], r[0x1C], r[0x1A]),
        lon: get_coord(r[0x24], r[0x1F], r[0x1E]),
        use_location: l1 & 0x08 != 0,
        alias_ts1: r[0x30] & 0x03,
        alias_ts2: (r[0x30] >> 2) & 0x03,
        sfr: l1 & 0x02 != 0,
        sfr_a: r[0x25],
        sfr_b: r[0x2A] & !0x70,
        tone_burst: r[0x2A] != 0xFF && r[0x2A] & 0x40 != 0,
        accept_bad_crc: r[0x2A] & 0x30 == 0x20, // 0xFF and 0xBF read as off
        tagged: None,
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Zone {
    pub slot: usize,
    pub name: String,
    pub channels: Vec<u16>,
}

/// Channels per zone record: 80 normally, 16 in the old layout.
pub fn zone_layout(b: &[u8]) -> usize {
    if b[ZONE_LAYOUT] <= 0x04 {
        80
    } else {
        16
    }
}

pub fn decode_zone(slot: usize, r: &[u8], per: usize) -> Zone {
    let mut channels = Vec::new();
    for i in 0..per {
        let n = u16le(r, 16 + 2 * i);
        if n == 0 {
            break;
        }
        channels.push(n);
    }
    Zone {
        slot,
        name: get_name(r, 0, 16),
        channels,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallType {
    Group,
    Private,
    All,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Contact {
    pub number: u16,
    pub name: String,
    pub id: u32,
    pub call_type: CallType,
    /// 0 = either, 1 or 2.
    pub timeslot: u8,
}

pub fn decode_contact(number: u16, r: &[u8]) -> Option<Contact> {
    if r[0] == 0xFF || r[0x14] > 2 {
        return None;
    }
    let call_type = [CallType::Group, CallType::Private, CallType::All][usize::from(r[0x14])];
    let id = if call_type == CallType::All {
        0xFF_FFFF
    } else {
        get_bcd_id(r, 0x10)
    };
    if id == 0 {
        return None;
    }
    let t = r[0x17];
    let timeslot = if t != 0xFF && t & 1 == 0 {
        if t & 2 != 0 {
            2
        } else {
            1
        }
    } else {
        0
    };
    Some(Contact {
        number,
        name: get_name(r, 0, 16),
        id,
        call_type,
        timeslot,
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RxList {
    pub number: u8,
    pub name: String,
    pub contacts: Vec<u16>,
}

pub fn decode_list(number: u8, r: &[u8], count: u8) -> RxList {
    let mut contacts = Vec::new();
    for i in 0..usize::from(count.saturating_sub(1)).min(32) {
        let c = u16le(r, 16 + 2 * i);
        if c == 0 || c > 1024 {
            break;
        }
        contacts.push(c);
    }
    RxList {
        number,
        name: get_name(r, 0, 16),
        contacts,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct General {
    pub radio_name: String,
    pub dmr_id: u32,
    pub group_hang: u8,
    pub private_hang: u8,
    pub boot_line1: String,
    pub boot_line2: String,
    /// The settings area's keys (codeplug-settings.md): `None` is an absent key, which leaves the radio's own value.
    pub keys: crate::kv::Keys,
}

pub fn decode_general(b: &[u8]) -> General {
    let g = &b[GENERAL..GENERAL + GENERAL_SIZE];
    let hang = |v: u8| if v == 0xFF { 0 } else { v };
    General {
        radio_name: get_name(g, 0, 8),
        dmr_id: get_bcd_id(g, 8),
        group_hang: hang(g[0x18]),
        private_hang: hang(g[0x19]),
        boot_line1: get_name(b, BOOT_TEXT, 16),
        boot_line2: get_name(b, BOOT_TEXT + 16, 16),
        keys: crate::kv::decode_keys(b),
    }
}

// --- Writing ---
//
// An encoder writes only the fields it owns onto the record as stored, so vendor bytes and flags no field covers
// survive an edit.

/// A blank channel record: the vendor defaults the per-channel DMR ID bytes replaced.
pub fn blank_channel() -> [u8; CH_SIZE] {
    let mut r = [0u8; CH_SIZE];
    r[..16].fill(0xFF);
    put16(&mut r, 0x20, 0xFFFF);
    put16(&mut r, 0x22, 0xFFFF);
    r[0x28] = 0x16;
    r
}

/// Two channels hold the same settings (the number is not compared; locations to 0.00005 degree).
pub fn same_channel(a: &Channel, b: &Channel) -> bool {
    let mut x = a.clone();
    x.number = b.number;
    x.lat = b.lat;
    x.lon = b.lon;
    x == *b && (a.lat - b.lat).abs() < 0.00005 && (a.lon - b.lon).abs() < 0.00005
}

/// Writes a channel's fields onto its stored record `r` (a blank one if `r` doesn't decode).
pub fn encode_channel(c: &Channel, r: &mut [u8]) {
    if get_freq(r, 0x10).is_none() {
        r[..CH_SIZE].copy_from_slice(&blank_channel());
    }
    put_name(r, 0, 16, &c.name);
    put_freq(r, 0x10, c.rx_hz);
    put_freq(r, 0x14, c.tx_hz);
    r[0x18] = u8::from(c.mode == Mode::Dmr);
    r[0x19] = c.power;
    r[0x1B] = c.tot.div_ceil(15).min(255) as u8;
    put_tone(r, 0x20, &c.rx_tone);
    put_tone(r, 0x22, &c.tx_tone);
    r[0x33] = (r[0x33] & !0x76)
        | if c.bw == 25.0 { 0x02 } else { 0 }
        | if c.rx_only { 0x04 } else { 0 }
        | if c.all_skip { 0x10 } else { 0 }
        | if c.zone_skip { 0x20 } else { 0 }
        | if c.vox { 0x40 } else { 0 };
    r[0x37] = c.squelch.min(21);
    r[0x2C] = (r[0x2C] & 0xF0) | (c.color_code & 0x0F);
    r[0x31] = (r[0x31] & !0x40) | if c.timeslot == 2 { 0x40 } else { 0 };
    put16(r, 0x2E, c.contact);
    r[0x2B] = c.rx_list;
    r[0x2D] = c.aprs;
    r[0x26] = (r[0x26] & !0xEF)
        | if c.dmr_id != 0 { 0x80 } else { 0 }
        | if c.no_beep { 0x40 } else { 0 }
        | if c.no_eco { 0x20 } else { 0 }
        | if c.use_location { 0x08 } else { 0 }
        | if c.dmo { 0x04 } else { 0 }
        | if c.sfr { 0x02 } else { 0 }
        | if c.roam { 0x01 } else { 0 };
    if c.dmr_id != 0 {
        r[0x27] = (c.dmr_id >> 16) as u8;
        r[0x28] = (c.dmr_id >> 8) as u8;
        r[0x29] = c.dmr_id as u8;
    } else if r[0x27] != 0x00 || r[0x28] != 0x16 || r[0x29] != 0x00 {
        r[0x27] = 0x00;
        r[0x28] = 0x16;
        r[0x29] = 0x00;
    }
    r[0x30] = (r[0x30] & !0x0F) | (c.alias_ts1 & 3) | ((c.alias_ts2 & 3) << 2);
    // Location bytes only when they change, so a record whose location bytes hold something else keeps them
    let old_lat = get_coord(r[0x1D], r[0x1C], r[0x1A]);
    let old_lon = get_coord(r[0x24], r[0x1F], r[0x1E]);
    if (old_lat - c.lat).abs() > 0.00005 || (old_lon - c.lon).abs() > 0.00005 {
        let [ah, am, al] = coord_bytes(c.lat);
        let [oh, om, ol] = coord_bytes(c.lon);
        (r[0x1D], r[0x1C], r[0x1A]) = (ah, am, al);
        (r[0x24], r[0x1F], r[0x1E]) = (oh, om, ol);
    }
    r[0x25] = c.sfr_a;
    r[0x2A] = (c.sfr_b & 0x8F) | (if c.tone_burst { 0x40 } else { 0 }) | (if c.accept_bad_crc { 0x20 } else { 0 }); // bits 5-4 always written: never 0xFF
}

/// The 80-channel layout: name, channel numbers, zeros after the last.
pub fn encode_zone(z: &Zone, r: &mut [u8]) {
    r[..ZONE_RECORD].fill(0);
    put_name(r, 0, 16, &z.name);
    for (i, &n) in z.channels.iter().take(80).enumerate() {
        put16(r, 16 + 2 * i, n);
    }
}

pub fn encode_contact(c: &Contact, r: &mut [u8]) {
    let was = decode_contact(c.number, r);
    put_name(r, 0, 16, &c.name);
    put_bcd_id(
        r,
        0x10,
        if c.call_type == CallType::All {
            0xFF_FFFF
        } else {
            c.id
        },
    );
    r[0x14] = match c.call_type {
        CallType::Group => 0,
        CallType::Private => 1,
        CallType::All => 2,
    };
    if was.is_none() {
        r[0x15] = 0;
        r[0x16] = 0;
    }
    if was.as_ref().is_none_or(|w| w.timeslot != c.timeslot) {
        r[0x17] = if c.timeslot != 0 {
            (c.timeslot - 1) << 1
        } else {
            0x01
        };
    }
}

pub fn encode_list(l: &RxList, r: &mut [u8]) {
    r[..LIST_SIZE].fill(0);
    put_name(r, 0, 16, &l.name);
    for (i, &n) in l.contacts.iter().take(32).enumerate() {
        put16(r, 16 + 2 * i, n);
    }
}

/// The general settings, each written only when it differs from what the image holds.
pub fn encode_general(s: &General, b: &mut [u8]) {
    let was = decode_general(b);
    if was.radio_name != s.radio_name {
        put_name(b, GENERAL, 8, &s.radio_name);
    }
    if was.dmr_id != s.dmr_id {
        if s.dmr_id != 0 {
            put_bcd_id(b, GENERAL + 8, s.dmr_id);
        } else {
            b[GENERAL + 8..GENERAL + 12].fill(0xFF);
        }
    }
    if was.group_hang != s.group_hang {
        b[GENERAL + 0x18] = s.group_hang;
    }
    if was.private_hang != s.private_hang {
        b[GENERAL + 0x19] = s.private_hang;
    }
    if was.boot_line1 != s.boot_line1 {
        put_name(b, BOOT_TEXT, 16, &s.boot_line1);
    }
    if was.boot_line2 != s.boot_line2 {
        put_name(b, BOOT_TEXT + 16, 16, &s.boot_line2);
    }
    // The settings area is touched only when a key changed, and never over an area of a newer version. (`put_keys`
    // says so with false; the editor warns about such an area where it knows the image.)
    crate::kv::put_keys(b, &s.keys);
}

// --- DTMF contacts (7.3.7) ---

const DTMF_DIGITS: &str = "0123456789ABCD*#";

/// A DTMF contact: a name and a code of up to 16 digits (`0`-`9`, `A`-`D`, `*`, `#`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DtmfContact {
    pub number: u8,
    pub name: String,
    pub code: String,
}

/// Whether `code` is a DTMF code: 1 to 16 of `0`-`9`, `A`-`D`, `*`, `#` (either case).
pub fn valid_dtmf(code: &str) -> bool {
    let n = code.chars().count();
    (1..=16).contains(&n) && code.chars().all(|c| DTMF_DIGITS.contains(c.to_ascii_uppercase()))
}

/// A record's contact; `None` when it is empty (a first byte of 0xFF or 0x00).
pub fn decode_dtmf(number: u8, r: &[u8]) -> Option<DtmfContact> {
    if r[0] == 0xFF || r[0] == 0x00 {
        return None;
    }
    let code: String = r[16..32]
        .iter()
        .take_while(|&&b| b < 16)
        .map(|&b| DTMF_DIGITS.as_bytes()[usize::from(b)] as char)
        .collect();
    Some(DtmfContact { number, name: get_name(r, 0, 16), code })
}

pub fn encode_dtmf(d: &DtmfContact, r: &mut [u8]) {
    put_name(r, 0, 16, &d.name);
    r[16..32].fill(0xFF);
    let mut n = 0;
    for ch in d.code.to_uppercase().chars() {
        if let Some(v) = DTMF_DIGITS.find(ch) {
            if n < 16 {
                r[16 + n] = v as u8;
                n += 1;
            }
        }
    }
}

// --- APRS configurations (7.3.8) ---

const APRS_MAGIC: u16 = 0x4153;

/// An APRS configuration: the sender, a fixed position, the digipeater path, the symbol, a comment, the frequency it
/// transmits on and the beacon's flags.
#[derive(Clone, Debug, PartialEq)]
pub struct Aprs {
    pub number: u8,
    pub name: String,
    pub ssid: u8,
    pub lat: f64,
    pub lon: f64,
    pub path1: String,
    pub path2: String,
    pub sym_table: char,
    pub sym_code: char,
    pub comment: String,
    /// 0 = none.
    pub tx_hz: u64,
    pub baud300: bool,
    pub use_fixed: bool,
    pub qsy: bool,
    pub silent: bool,
    /// Position ambiguity, 0-4.
    pub ambiguity: u8,
}

impl Aprs {
    /// A new configuration, as the web editor starts one: symbol `/>` (a car), 1200 baud.
    pub fn new(number: u8, name: &str) -> Self {
        Self {
            number,
            name: name.to_string(),
            ssid: 0,
            lat: 0.0,
            lon: 0.0,
            path1: String::new(),
            path2: String::new(),
            sym_table: '/',
            sym_code: '>',
            comment: String::new(),
            tx_hz: 0,
            baud300: false,
            use_fixed: false,
            qsy: false,
            silent: false,
            ambiguity: 0,
        }
    }
}

fn get_path(r: &[u8], off: usize) -> String {
    let call = get_name(r, off, 6);
    let call = call.trim_end_matches(' ');
    if call.is_empty() {
        return String::new();
    }
    let ssid = r[off + 6];
    if (1..=15).contains(&ssid) {
        format!("{call}-{ssid}")
    } else {
        call.to_string()
    }
}

fn put_path(r: &mut [u8], off: usize, text: &str) {
    let up = text.to_uppercase();
    let mut parts = up.split('-');
    let call = parts.next().unwrap_or("");
    let ssid = parts.next().map_or(0, |s| s.trim().parse::<u32>().unwrap_or(0));
    r[off..off + 6].fill(0x20);
    for (i, ch) in call.chars().take(6).enumerate() {
        r[off + i] = (u32::from(ch) & 0x7F) as u8;
    }
    r[off + 6] = if (1..=15).contains(&ssid) { ssid as u8 } else { 0 };
}

/// A record's configuration; `None` when it is empty or not an APRS record (the magic is the current or the older one).
pub fn decode_aprs(number: u8, r: &[u8]) -> Option<Aprs> {
    let magic = u16le(r, 0x3E);
    if r[0] == 0xFF || r[0] == 0x00 || (magic != APRS_MAGIC && magic != APRS_MAGIC - 1) {
        return None;
    }
    let flags = r[0x3D];
    // The older layout (0x4152) has the ambiguity field one bit lower.
    let amb = if magic == APRS_MAGIC { flags >> 5 } else { (flags >> 4) & 7 };
    Some(Aprs {
        number,
        name: get_name(r, 0, 8),
        ssid: if r[0x08] <= 15 { r[0x08] } else { 0 },
        lat: get_coord(r[0x09], r[0x0A], r[0x0B]),
        lon: get_coord(r[0x0C], r[0x0D], r[0x0E]),
        path1: get_path(r, 0x0F),
        path2: get_path(r, 0x16),
        sym_table: char::from(r[0x1D]),
        sym_code: char::from(r[0x1E]),
        comment: get_name(r, 0x1F, 23),
        tx_hz: get_freq(r, 0x37).unwrap_or(0),
        baud300: flags & 0x01 != 0,
        use_fixed: flags & 0x02 != 0,
        qsy: flags & 0x04 != 0,
        silent: flags & 0x08 != 0,
        ambiguity: amb.min(4),
    })
}

/// Writes a configuration over the record: only the fields that changed are rewritten (the position, the paths and
/// the comment), the bytes no field covers stay, and an older record is upgraded to the current layout, as the
/// firmware does.
pub fn encode_aprs(a: &Aprs, r: &mut [u8]) {
    let was = decode_aprs(a.number, r);
    if was.is_none() {
        r.fill(0);
    }
    put_name(r, 0, 8, &a.name);
    r[0x08] = a.ssid & 0x0F;
    if was.as_ref().is_none_or(|w| w.lat != a.lat || w.lon != a.lon) {
        r[0x09..0x0C].copy_from_slice(&coord_bytes(a.lat));
        r[0x0C..0x0F].copy_from_slice(&coord_bytes(a.lon));
    }
    if was.as_ref().is_none_or(|w| w.path1 != a.path1) {
        put_path(r, 0x0F, &a.path1);
    }
    if was.as_ref().is_none_or(|w| w.path2 != a.path2) {
        put_path(r, 0x16, &a.path2);
    }
    r[0x1D] = (u32::from(a.sym_table) & 0xFF) as u8;
    r[0x1E] = (u32::from(a.sym_code) & 0xFF) as u8;
    if was.as_ref().is_none_or(|w| w.comment != a.comment) {
        put_name(r, 0x1F, 24, &a.comment);
    }
    if a.tx_hz != 0 {
        put_freq(r, 0x37, a.tx_hz);
    } else {
        r[0x37..0x3B].fill(0);
    }
    r[0x3D] = u8::from(a.baud300)
        | (u8::from(a.use_fixed) << 1)
        | (u8::from(a.qsy) << 2)
        | (u8::from(a.silent) << 3)
        | ((a.ambiguity & 7) << 5);
    put16(r, 0x3E, APRS_MAGIC);
}

/// Two configurations hold the same settings (locations to 0.00005 degree, as the web editor compares them).
pub fn same_aprs(a: &Aprs, b: &Aprs) -> bool {
    let mut x = a.clone();
    x.lat = b.lat;
    x.lon = b.lon;
    x == *b && (a.lat - b.lat).abs() < 0.00005 && (a.lon - b.lon).abs() < 0.00005
}

#[cfg(test)]
mod aprs_dtmf_tests {
    use super::*;

    #[test]
    fn a_dtmf_code_is_one_digit_per_byte_ended_by_ff() {
        let d = DtmfContact { number: 1, name: "Gate".into(), code: "09AD*#".into() };
        let mut r = [0u8; DTMF_SIZE];
        encode_dtmf(&d, &mut r);
        assert_eq!(&r[16..24], &[0, 9, 10, 13, 14, 15, 0xFF, 0xFF]);
        assert_eq!(decode_dtmf(1, &r), Some(d));
        // Lower case is stored as upper case; characters that are not digits are dropped; 16 digits at most.
        let d = DtmfContact { number: 1, name: "x".into(), code: "ab-1Z2".into() };
        encode_dtmf(&d, &mut r);
        assert_eq!(decode_dtmf(1, &r).unwrap().code, "AB12");
        let long = DtmfContact { number: 1, name: "x".into(), code: "1".repeat(20) };
        encode_dtmf(&long, &mut r);
        assert_eq!(decode_dtmf(1, &r).unwrap().code.len(), 16);
    }

    #[test]
    fn an_empty_dtmf_record_is_none_and_the_code_check_is_one_to_sixteen() {
        assert_eq!(decode_dtmf(1, &[0xFF; DTMF_SIZE]), None);
        assert_eq!(decode_dtmf(1, &[0x00; DTMF_SIZE]), None);
        for ok in ["1", "a", "0123456789ABCD*#", "*#"] {
            assert!(valid_dtmf(ok), "{ok}");
        }
        for bad in ["", "E", "12 34", "12345678901234567", "1-2"] {
            assert!(!valid_dtmf(bad), "{bad}");
        }
    }

    fn sample() -> Aprs {
        let mut a = Aprs::new(3, "Mobile");
        a.ssid = 9;
        a.lat = 37.7749;
        a.lon = -122.4194;
        a.path1 = "WIDE1-1".into();
        a.path2 = "WIDE2".into();
        a.comment = "A comment".into();
        a.tx_hz = 144_390_000;
        a.qsy = true;
        a.ambiguity = 4;
        a
    }

    #[test]
    fn an_aprs_configuration_round_trips() {
        let a = sample();
        let mut r = [0xFFu8; APRS_SIZE];
        encode_aprs(&a, &mut r);
        let back = decode_aprs(3, &r).unwrap();
        assert_eq!(back.name, "Mobile");
        assert_eq!((back.path1.as_str(), back.path2.as_str()), ("WIDE1-1", "WIDE2"));
        assert_eq!((back.sym_table, back.sym_code), ('/', '>'));
        assert!(same_aprs(&back, &a), "{back:?}");
        // The record always carries the current magic.
        assert_eq!(u16::from_le_bytes([r[0x3E], r[0x3F]]), 0x4153);
    }

    #[test]
    fn a_path_keeps_its_callsign_and_only_an_ssid_from_1_to_15() {
        let mut r = [0u8; APRS_SIZE];
        put_path(&mut r, 0x0F, "wide2-2");
        assert_eq!(get_path(&r, 0x0F), "WIDE2-2");
        put_path(&mut r, 0x0F, "relay-0");
        assert_eq!(get_path(&r, 0x0F), "RELAY");
        put_path(&mut r, 0x0F, "relay-16");
        assert_eq!(get_path(&r, 0x0F), "RELAY");
        put_path(&mut r, 0x0F, "");
        assert_eq!(get_path(&r, 0x0F), "");
        // A callsign longer than 6 is cut.
        put_path(&mut r, 0x0F, "TOOLONGCALL-3");
        assert_eq!(get_path(&r, 0x0F), "TOOLON-3");
    }

    #[test]
    fn the_ambiguity_field_is_read_where_the_layout_puts_it_and_clamped_to_4() {
        let mut r = [0u8; APRS_SIZE];
        encode_aprs(&sample(), &mut r);
        r[0x3D] = 7 << 5; // the current layout
        assert_eq!(decode_aprs(3, &r).unwrap().ambiguity, 4);
        r[0x3E..0x40].copy_from_slice(&0x4152u16.to_le_bytes());
        r[0x3D] = 3 << 4; // the older layout, one bit lower
        assert_eq!(decode_aprs(3, &r).unwrap().ambiguity, 3);
        // Not an APRS record: another magic, or an empty first byte.
        r[0x3E] = 0;
        assert_eq!(decode_aprs(3, &r), None);
    }

    #[test]
    fn an_older_record_is_upgraded_only_when_it_is_rewritten() {
        let a = sample();
        let mut r = [0u8; APRS_SIZE];
        encode_aprs(&a, &mut r);
        r[0x3E..0x40].copy_from_slice(&0x4152u16.to_le_bytes());
        r[0x3D] = (3 << 4) | 0x04;
        let was = decode_aprs(3, &r).unwrap();
        // Nothing changed: the model leaves the record alone (same_aprs), so it stays in the older layout.
        assert!(same_aprs(&was, &decode_aprs(3, &r).unwrap()));
        let mut changed = was.clone();
        changed.comment = "New".into();
        encode_aprs(&changed, &mut r);
        assert_eq!(u16::from_le_bytes([r[0x3E], r[0x3F]]), 0x4153);
        assert_eq!(decode_aprs(3, &r).unwrap().ambiguity, 3, "the ambiguity is kept");
    }
}
