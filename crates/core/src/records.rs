// SPDX-License-Identifier: MIT
// Copyright (c) 2026 The RadioFoundry Authors
// Copyright (c) 2026 insideTheFlux
//
// The open-firmware codeplug's records (open-firmware-cps.md 7.2-7.3), decoded into plain structs. Ported from
// RadioFoundry's `site/static/js/codeplug/records.js`. References between records (a channel's contact, a zone's
// channels) are 1-based numbers.

use crate::fields::{get_bcd_id, get_coord, get_freq, get_name, get_tone, u16le};
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
pub const ZONE_BITMAP: usize = EE + 0x8010;
pub const ZONE_DATA: usize = EE + 0x8030;
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
