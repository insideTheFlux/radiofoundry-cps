// SPDX-License-Identifier: MIT
// Copyright (c) 2026 The RadioFoundry Authors
// Copyright (c) 2026 insideTheFlux
//
// Systems of the channel-mode layer (RadioFoundry `docs/spec/channel-modes.md` 7, `p25-codeplug.md` 3): what channels
// of a trunked kind share. A system record, its entries (sites, control channels, a channel plan) and its secret live
// in the codeplug settings area. Ported from `site/static/js/codeplug/channelmodes.js` and `mode_p25.js`. A system of a
// type this editor doesn't know is kept whole, with its entries and secret, byte for byte.

use crate::fields::{get_name, put_name};
use crate::kv::{self, Entry};
use crate::modes::{
    grow, name_check, u16_at, u32_at, u8_at, Fallback, NAC_ANY,
};
use std::collections::HashMap;

pub const SYSTEM_KEY: u16 = 0x0100;
pub const ENTRY_KEY: u16 = 0x0200;
pub const SECRET_KEY: u16 = 0x0300;
pub const MAX_SYSTEMS: usize = 255;
pub const MAX_ENTRIES: usize = 256;
const SYS_HDR: usize = 28;
const ENTRY_HDR: usize = 4;

/// System flag bits (7.1): bit 0 needs its secret, bit 1 the transmit side is allowed (clear: listen only).
pub const SYS_NEEDS_KEY: u8 = 0x01;
pub const SYS_TX: u8 = 0x02;

/// Sharing (7.1): 0 off, 1 identity only, 2 whole system with its key.
pub const SHARE_OFF: u8 = 0;
pub const SHARE_IDENTITY: u8 = 1;
pub const SHARE_WHOLE: u8 = 2;

pub const SYSTEM_P25_TRUNK: u8 = 3;

pub fn type_name(t: u8) -> &'static str {
    match t {
        1 => "T3",
        SYSTEM_P25_TRUNK => "P25T",
        _ => "?",
    }
}

fn s8_at(b: &[u8], o: usize) -> i8 {
    u8_at(b, o) as i8
}

fn u24_at(b: &[u8], o: usize) -> u32 {
    if o + 3 <= b.len() {
        u32::from(b[o]) | (u32::from(b[o + 1]) << 8) | (u32::from(b[o + 2]) << 16)
    } else {
        0
    }
}

// --- The P25 trunked system's block (p25-codeplug.md 3) ---

pub const WACN_ANY: u32 = 0xFF_FFFF;
pub const SYSID_ANY: u16 = 0xFFFF;
const NAC_SET: u16 = 0x8000;
const P25_UNKNOWN: u8 = 0xFF;

/// `None` is "any" for the identity fields.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct P25Trunk {
    pub wacn: Option<u32>,
    pub sysid: Option<u16>,
    /// The control channel NAC; `None` = any.
    pub cc_nac: Option<u16>,
    pub skip_enc: bool,
    pub plan_only: bool,
    pub no_reg: bool,
    pub hold_s: u8,
    pub roam_dbm: i8,
}

/// The identity is the first five octets of the block.
const P25_ID_LEN: u8 = 5;

pub fn decode_p25_trunk(b: &[u8]) -> P25Trunk {
    let w = if b.len() >= 3 { u24_at(b, 0) } else { 0 };
    let s = u16_at(b, 3);
    let n = u16_at(b, 5);
    let cc = if n & NAC_SET != 0 && n & 0x7000 == 0 { n & 0x0FFF } else { NAC_ANY };
    P25Trunk {
        wacn: (w != WACN_ANY).then_some(w & 0xF_FFFF),
        sysid: (s != SYSID_ANY).then_some(s & 0x0FFF),
        cc_nac: (cc != NAC_ANY).then_some(cc),
        skip_enc: u8_at(b, 7) & 1 != 0,
        plan_only: u8_at(b, 7) & 2 != 0,
        no_reg: u8_at(b, 7) & 4 != 0,
        hold_s: u8_at(b, 8),
        roam_dbm: s8_at(b, 9),
    }
}

pub fn encode_p25_trunk(s: &P25Trunk, prev: Option<&[u8]>) -> Vec<u8> {
    let mut v = grow(prev, 10);
    let w = s.wacn.map_or(WACN_ANY, |w| w & 0xF_FFFF);
    v[0] = (w & 0xFF) as u8;
    v[1] = ((w >> 8) & 0xFF) as u8;
    v[2] = ((w >> 16) & 0xFF) as u8;
    v[3..5].copy_from_slice(&s.sysid.map_or(SYSID_ANY, |x| x & 0x0FFF).to_le_bytes());
    v[5..7].copy_from_slice(&s.cc_nac.map_or(0, |n| NAC_SET | (n & 0x0FFF)).to_le_bytes());
    v[7] = (v[7] & !7) | u8::from(s.skip_enc) | (u8::from(s.plan_only) << 1) | (u8::from(s.no_reg) << 2);
    v[8] = s.hold_s;
    v[9] = s.roam_dbm as u8;
    v
}

/// A system's type-specific fields: those of a type this editor edits, else the block as stored (kept).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SystemFields {
    P25Trunk(P25Trunk),
    Unknown,
}

fn decode_fields(type_id: u8, block: &[u8]) -> SystemFields {
    match type_id {
        SYSTEM_P25_TRUNK => SystemFields::P25Trunk(decode_p25_trunk(block)),
        _ => SystemFields::Unknown,
    }
}

fn encode_fields(f: &SystemFields, prev: Option<&[u8]>) -> Option<Vec<u8>> {
    match f {
        SystemFields::P25Trunk(x) => Some(encode_p25_trunk(x, prev)),
        SystemFields::Unknown => None,
    }
}

/// The identity length of a type the editor knows (7.1).
fn id_len(type_id: u8) -> Option<u8> {
    (type_id == SYSTEM_P25_TRUNK).then_some(P25_ID_LEN)
}

/// Whether a type is one this editor edits.
pub fn type_known(type_id: u8) -> bool {
    id_len(type_id).is_some()
}

/// The fields a new system of `type_id` starts with.
pub fn new_fields(type_id: u8) -> SystemFields {
    match type_id {
        SYSTEM_P25_TRUNK => SystemFields::P25Trunk(P25Trunk::default()),
        _ => SystemFields::Unknown,
    }
}

// --- Entries (7.2) ---

/// An entry's class body, for the classes of the types this editor knows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EntryBody {
    /// P25 control channel (class 0): the identifier (0-15) and channel number (0-4095) of its channel, and its
    /// frequencies (the TX one is 0 when it comes from the identifier's transmit offset).
    P25Control { iden: u8, chan_no: u16, rx_hz: u32, tx_hz: u32 },
    /// P25 site (class 1): RFSS and site, `None` = not known; both are for display and roaming.
    P25Site { rfss: Option<u8>, site: Option<u8> },
    /// P25 channel identifier plan row (class 2): the group is the identifier.
    P25Plan { base_hz: u32, spacing_hz: u32, tx_offset_hz: i32, bw_hz: u32, tdma: bool, slots: u8 },
    /// A class this editor doesn't edit: kept as stored.
    Other,
}

/// One entry of a system: its class, group (the site or the identifier; 0xFF the whole system's), whether it is a
/// control channel, the flag bits 1-7 as found, and the body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SysEntry {
    pub cls: u8,
    pub group: u8,
    pub control: bool,
    pub flags: u8,
    pub body: EntryBody,
    /// The record as read; empty for an entry made here.
    pub raw: Vec<u8>,
}

impl SysEntry {
    /// The entry without its raw bytes: what "unchanged" is compared on.
    fn fields(&self) -> SysEntry {
        SysEntry { raw: Vec::new(), ..self.clone() }
    }
}

/// An entry record (7.2): `(system, class, group, flags, body)`; `None` when too short.
fn decode_entry_record(v: &[u8]) -> Option<(u8, u8, u8, u8, &[u8])> {
    (v.len() >= ENTRY_HDR).then(|| (v[0], v[1], v[2], v[3], &v[ENTRY_HDR..]))
}

fn decode_entry(type_id: u8, cls: u8, group: u8, flags: u8, body: &[u8], raw: &[u8]) -> SysEntry {
    let b = if type_id == SYSTEM_P25_TRUNK {
        match cls {
            0 => {
                let chan = u16_at(body, 0);
                EntryBody::P25Control { iden: (chan >> 12) as u8, chan_no: chan & 0x0FFF, rx_hz: u32_at(body, 2), tx_hz: u32_at(body, 6) }
            }
            1 => EntryBody::P25Site {
                rfss: (!body.is_empty() && u8_at(body, 0) != P25_UNKNOWN).then(|| u8_at(body, 0)),
                site: (body.len() >= 2 && u8_at(body, 1) != P25_UNKNOWN).then(|| u8_at(body, 1)),
            },
            2 => {
                let tdma = u8_at(body, 16) == 1;
                EntryBody::P25Plan {
                    base_hz: u32_at(body, 0),
                    spacing_hz: u32_at(body, 4),
                    tx_offset_hz: u32_at(body, 8) as i32,
                    bw_hz: u32_at(body, 12),
                    tdma,
                    // A TDMA row's slots per carrier; 0 means 2.
                    slots: match u8_at(body, 17) {
                        _ if !tdma => 2,
                        0 => 2,
                        n => n,
                    },
                }
            }
            _ => EntryBody::Other,
        }
    } else {
        EntryBody::Other
    };
    SysEntry { cls, group, control: flags & 1 != 0, flags: flags & !1, body: b, raw: raw.to_vec() }
}

/// The entry's record for system `n` (0-based): the stored one when nothing it holds changed (so the bytes this editor
/// doesn't know are kept), else rebuilt over the stored bytes.
fn encode_entry(type_id: u8, n: u8, e: &SysEntry) -> Vec<u8> {
    if let Some((sys, cls, group, flags, body)) = decode_entry_record(&e.raw) {
        if sys == n && decode_entry(type_id, cls, group, flags, body, &[]).fields() == e.fields() {
            return e.raw.clone();
        }
    }
    if !type_known(type_id) {
        return e.raw.clone();
    }
    let mut body: Vec<u8> = e.raw.get(ENTRY_HDR..).map_or_else(Vec::new, <[u8]>::to_vec);
    match &e.body {
        EntryBody::P25Control { iden, chan_no, rx_hz, tx_hz } => {
            body = grow(Some(&body), 10);
            body[2..6].copy_from_slice(&rx_hz.to_le_bytes());
            body[6..10].copy_from_slice(&tx_hz.to_le_bytes());
            body[0..2].copy_from_slice(&((u16::from(*iden & 15) << 12) | (chan_no & 0x0FFF)).to_le_bytes());
        }
        EntryBody::P25Site { rfss, site } => {
            body = grow(Some(&body), 2);
            body[0] = rfss.unwrap_or(P25_UNKNOWN);
            body[1] = site.unwrap_or(P25_UNKNOWN);
        }
        EntryBody::P25Plan { base_hz, spacing_hz, tx_offset_hz, bw_hz, tdma, slots } => {
            body = grow(Some(&body), 18);
            body[0..4].copy_from_slice(&base_hz.to_le_bytes());
            body[4..8].copy_from_slice(&spacing_hz.to_le_bytes());
            body[8..12].copy_from_slice(&(*tx_offset_hz as u32).to_le_bytes());
            body[12..16].copy_from_slice(&bw_hz.to_le_bytes());
            body[16] = u8::from(*tdma);
            body[17] = if *tdma { *slots } else { 0 };
        }
        EntryBody::Other => {}
    }
    let mut v = vec![n, e.cls, e.group, (e.flags & !1) | u8::from(e.control)];
    v.extend_from_slice(&body);
    v
}

// --- Systems (7.1) ---

/// A system. `number` is 1-255 (key `0x0100 + number - 1`); 0 until the system is numbered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct System {
    pub number: u16,
    pub type_id: u8,
    pub name: String,
    pub sharing: u8,
    /// This radio's ID on the system; 0 = the kind's radio-wide one. Never shared.
    pub own_id: u32,
    /// The transmit side is allowed. Clear: listen only.
    pub tx: bool,
    pub needs_key: bool,
    pub flags_raw: u8,
    pub fallback: Fallback,
    pub fields: SystemFields,
    pub entries: Vec<SysEntry>,
    /// The system's own secret, as stored (never shown): `None` for none.
    pub secret: Option<Vec<u8>>,
    /// The record as read; empty for a system made here.
    pub raw: Vec<u8>,
}

impl System {
    /// A new system of `type_id`: listen only until the user turns the transmit side on.
    pub fn new(type_id: u8, name: &str) -> Self {
        Self {
            number: 0,
            type_id,
            name: name.to_string(),
            sharing: SHARE_IDENTITY,
            own_id: 0,
            tx: false,
            needs_key: false,
            flags_raw: 0,
            fallback: Fallback::None,
            fields: new_fields(type_id),
            entries: Vec::new(),
            secret: None,
            raw: Vec::new(),
        }
    }

    /// Two systems with the same identity are the same system (7.1); `None` for a type this editor doesn't know.
    pub fn identity(&self) -> Option<String> {
        let len = usize::from(id_len(self.type_id)?);
        let block = encode_fields(&self.fields, None)?;
        Some(format!("{}:{:?}", self.type_id, &block[..len.min(block.len())]))
    }
}

/// A system record's header and block (7.1); `None` when the value is no system.
struct Header<'a> {
    type_id: u8,
    flags: u8,
    sharing: u8,
    own_id: u32,
    fb_chan: u16,
    fb_check: u16,
    name: String,
    block: &'a [u8],
}

fn decode_header(v: &[u8]) -> Option<Header<'_>> {
    if v.len() < SYS_HDR || v[0] == 0 || v[0] == 0xFF {
        return None;
    }
    Some(Header {
        type_id: v[0],
        flags: v[1],
        sharing: v[2],
        own_id: u32_at(v, 4),
        fb_chan: u16_at(v, 8),
        fb_check: u16_at(v, 10),
        name: get_name(v, 12, 16),
        block: &v[SYS_HDR..],
    })
}

/// The systems and their entries and secrets in the area's entries. `channel_names` resolves fallbacks (a channel
/// whose name still matches the stored check). Entries of a system that isn't there are returned as orphans, kept.
pub fn decode_systems(entries: &[Entry], channel_names: &HashMap<u16, String>) -> (Vec<System>, Vec<Vec<u8>>) {
    let mut systems: Vec<System> = Vec::new();
    for n in 0..MAX_SYSTEMS {
        let Some(raw) = kv::get_value(entries, SYSTEM_KEY + n as u16) else { continue };
        let Some(h) = decode_header(raw) else { continue };
        let fb = if h.fb_chan == 0 {
            Fallback::None
        } else {
            match channel_names.get(&h.fb_chan) {
                Some(name) if name_check(name) == h.fb_check => Fallback::Channel(h.fb_chan),
                _ => Fallback::Stale(h.fb_chan),
            }
        };
        systems.push(System {
            number: n as u16 + 1,
            type_id: h.type_id,
            name: h.name,
            sharing: h.sharing,
            own_id: h.own_id,
            tx: h.flags & SYS_TX != 0,
            needs_key: h.flags & SYS_NEEDS_KEY != 0,
            flags_raw: h.flags,
            fallback: fb,
            fields: decode_fields(h.type_id, h.block),
            entries: Vec::new(),
            secret: kv::get_value(entries, SECRET_KEY + n as u16).map(<[u8]>::to_vec),
            raw: raw.to_vec(),
        });
    }
    let mut fam: Vec<&Entry> =
        entries.iter().filter(|e| (ENTRY_KEY..ENTRY_KEY + MAX_ENTRIES as u16).contains(&e.key)).collect();
    fam.sort_by_key(|e| e.key);
    let mut orphans = Vec::new();
    for e in fam {
        match decode_entry_record(&e.value) {
            Some((sys, cls, group, flags, body)) => {
                match systems.iter_mut().find(|s| s.number == u16::from(sys) + 1) {
                    Some(s) => s.entries.push(decode_entry(s.type_id, cls, group, flags, body, &e.value)),
                    None => orphans.push(e.value.clone()),
                }
            }
            None => orphans.push(e.value.clone()),
        }
    }
    (systems, orphans)
}

/// A fallback as stored: the channel number and the name check. A number as read keeps the check stored with it.
fn fallback_out(f: &Fallback, names: &HashMap<u16, &str>, was: Option<(u16, u16)>) -> (u16, u16) {
    match f {
        Fallback::None => (0, 0),
        Fallback::Channel(n) => names.get(n).map_or((0, 0), |name| (*n, name_check(name))),
        Fallback::Stale(n) => (*n, was.filter(|w| w.0 == *n).map_or(0, |w| w.1)),
    }
}

/// The system's record (7.1), over the one stored at its key.
fn encode_system(s: &System, prev: Option<&[u8]>, names: &HashMap<u16, &str>) -> Vec<u8> {
    let was = prev.and_then(decode_header);
    let Some(len) = id_len(s.type_id) else {
        // A type this editor doesn't know: as found.
        return prev.map_or_else(|| s.raw.clone(), <[u8]>::to_vec);
    };
    let old = was.as_ref().filter(|w| w.type_id == s.type_id).map(|w| w.block);
    let block = match old {
        Some(o) if decode_fields(s.type_id, o) == s.fields => o.to_vec(),
        _ => encode_fields(&s.fields, old).unwrap_or_default(),
    };
    let mut v = vec![0u8; SYS_HDR + block.len()];
    v[0] = s.type_id;
    v[1] = (was.as_ref().map_or(0, |w| w.flags) & !(SYS_TX | SYS_NEEDS_KEY))
        | if s.tx { SYS_TX } else { 0 }
        | if s.needs_key { SYS_NEEDS_KEY } else { 0 };
    v[2] = s.sharing;
    v[3] = len;
    v[4..8].copy_from_slice(&s.own_id.to_le_bytes());
    let (fb_chan, fb_check) = fallback_out(&s.fallback, names, was.as_ref().map(|w| (w.fb_chan, w.fb_check)));
    v[8..10].copy_from_slice(&fb_chan.to_le_bytes());
    v[10..12].copy_from_slice(&fb_check.to_le_bytes());
    match (&was, prev) {
        (Some(w), Some(p)) if w.name == s.name => v[12..SYS_HDR].copy_from_slice(&p[12..SYS_HDR]),
        _ => put_name(&mut v, 12, 16, &s.name),
    }
    v[SYS_HDR..].copy_from_slice(&block);
    v
}

/// The area's entries with the systems, their entries and their secrets in place of those stored. A system the
/// model no longer has goes, with its secret; entries are the family rewritten from `0x0200` up when any changed.
pub fn system_entries(
    systems: &[System],
    orphans: &[Vec<u8>],
    mut out: Vec<Entry>,
    names: &HashMap<u16, &str>,
) -> Vec<Entry> {
    let valid: Vec<&System> =
        systems.iter().filter(|s| (1..=MAX_SYSTEMS as u16).contains(&s.number)).collect();
    for n in 0..MAX_SYSTEMS {
        let key = SYSTEM_KEY + n as u16;
        let prev = kv::get_value(&out, key).map(<[u8]>::to_vec);
        let Some(s) = valid.iter().find(|s| s.number == n as u16 + 1) else {
            // A system the model no longer has was deleted: its secret with it.
            if prev.as_deref().and_then(decode_header).is_some() {
                crate::modes::del(&mut out, key);
                crate::modes::del(&mut out, SECRET_KEY + n as u16);
            }
            continue;
        };
        let v = encode_system(s, prev.as_deref(), names);
        crate::modes::set(&mut out, key, &v);
        let skey = SECRET_KEY + n as u16;
        match &s.secret {
            None => crate::modes::del(&mut out, skey),
            Some(raw) => crate::modes::set(&mut out, skey, raw),
        }
    }
    // Entries: the family rewritten from 0x0200 up, in system order, when any record changed.
    let mut want: Vec<Vec<u8>> = Vec::new();
    let mut sorted = valid.clone();
    sorted.sort_by_key(|s| s.number);
    for s in sorted {
        for e in &s.entries {
            want.push(encode_entry(s.type_id, (s.number - 1) as u8, e));
        }
    }
    let nums: Vec<u8> = valid.iter().map(|s| (s.number - 1) as u8).collect();
    want.extend(orphans.iter().filter(|v| !nums.contains(&v.first().copied().unwrap_or(0xFF))).cloned());
    want.truncate(MAX_ENTRIES);
    let mut have: Vec<&Entry> =
        out.iter().filter(|e| (ENTRY_KEY..ENTRY_KEY + MAX_ENTRIES as u16).contains(&e.key)).collect();
    have.sort_by_key(|e| e.key);
    let same = have.len() == want.len()
        && have.iter().enumerate().all(|(i, e)| e.key == ENTRY_KEY + i as u16 && e.value == want[i]);
    if !same {
        out.retain(|e| !(ENTRY_KEY..ENTRY_KEY + MAX_ENTRIES as u16).contains(&e.key));
        for (i, v) in want.iter().enumerate() {
            out = kv::set_value(&out, ENTRY_KEY + i as u16, v);
        }
    }
    out
}

/// Lowest free system numbers given to the systems that have none (`number` 0).
pub fn number_systems(systems: &mut [System]) {
    let mut taken: Vec<u16> =
        systems.iter().filter(|s| (1..=MAX_SYSTEMS as u16).contains(&s.number)).map(|s| s.number).collect();
    let mut n = 1u16;
    for s in systems.iter_mut().filter(|s| !(1..=MAX_SYSTEMS as u16).contains(&s.number)) {
        while taken.contains(&n) {
            n += 1;
        }
        s.number = if usize::from(n) <= MAX_SYSTEMS { n } else { 0 };
        taken.push(n);
    }
}
