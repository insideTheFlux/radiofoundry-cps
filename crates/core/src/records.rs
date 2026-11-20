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
        sfr_b: r[0x2A] & 0xBF,
        tone_burst: r[0x2A] != 0xFF && r[0x2A] & 0x40 != 0,
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
    r[0x2A] = (c.sfr_b & 0xBF) | if c.tone_burst { 0x40 } else { 0 };
    if r[0x2A] == 0xFF {
        r[0x2A] = 0xCF; // 0xFF reads as not set: the unused bits 5-4 cleared (as the firmware)
    }
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
}
