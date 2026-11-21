// SPDX-License-Identifier: MIT
// Copyright (c) 2026 The RadioFoundry Authors
// Copyright (c) 2026 insideTheFlux
//
// Channel modes (RadioFoundry `docs/spec/channel-modes.md`, `p25-codeplug.md`, `nxdn-codeplug.md`): a channel that is
// more than FM or DMR is an ordinary channel record plus a keyed record, its tag, in the codeplug settings area
// ([`crate::kv`]). Ported from `site/static/js/codeplug/channelmodes.js`, `mode_p25.js` and `mode_nxdn.js`, for the
// two kinds this editor knows, P25 conventional (2) and NXDN conventional (4). Every other kind, every system and
// every other key of the area is kept byte for byte: a channel of a kind this editor doesn't know shows its kind and
// keeps its tag as found.

use crate::fields::put_name;
use crate::kv::{self, Entry};
use crate::systems::{self, EntryBody, System};
use crate::records::{channel_addr, Channel, Mode, CH_SIZE, MAX_CHANNELS};
use std::collections::HashMap;

pub const TAG_KEY: u16 = 0x0400;
pub const SETTING_KEY: u16 = 0x0A00;
const MAX_TAGS: usize = 1024;

/// Tag flag bits (channel-modes.md 2.1).
pub const TAG_TX: u8 = 0x01;
pub const TAG_BASE_OWN: u8 = 0x02;
pub const TAG_FALLBACK: u8 = 0x04;
pub const TAG_RESERVED: u8 = 0x08;
const TAG_HDR: usize = 5;
const TAG_FB: usize = 4;

pub const KIND_T3: u8 = 1;
pub const KIND_P25: u8 = 2;
pub const KIND_P25_TRUNK: u8 = 3;
pub const KIND_NXDN: u8 = 4;

/// The short name of a kind as the radio shows it; "?" for one this editor doesn't know.
pub fn kind_name(kind: u8) -> &'static str {
    match kind {
        KIND_T3 => "T3",
        KIND_P25 => "P25",
        KIND_P25_TRUNK => "P25T",
        KIND_NXDN => "NXDN",
        _ => "?",
    }
}

/// Kinds a new channel can be made in this editor.
pub const EDITABLE_KINDS: [u8; 4] = [KIND_P25, KIND_NXDN, KIND_P25_TRUNK, KIND_T3];

// --- Bytes ---

pub(crate) fn u8_at(b: &[u8], o: usize) -> u8 {
    b.get(o).copied().unwrap_or(0)
}

/// Little endian, 0 past a short record's end.
pub(crate) fn u16_at(b: &[u8], o: usize) -> u16 {
    if o + 2 <= b.len() {
        u16::from(b[o]) | (u16::from(b[o + 1]) << 8)
    } else {
        0
    }
}

pub(crate) fn u32_at(b: &[u8], o: usize) -> u32 {
    if o + 4 <= b.len() {
        u32::from(u16_at(b, o)) | (u32::from(u16_at(b, o + 2)) << 16)
    } else {
        0
    }
}

/// `prev`'s bytes, at least `len` long (zeros added), or `len` zeros: a record written over the one stored keeps the
/// bytes no field covers.
pub(crate) fn grow(prev: Option<&[u8]>, len: usize) -> Vec<u8> {
    let mut v = vec![0u8; len.max(prev.map_or(0, <[u8]>::len))];
    if let Some(p) = prev {
        v[..p.len()].copy_from_slice(p);
    }
    v
}

/// The name check (2.2): FNV-1a 32 over the name as a channel record stores it (16 octets; from the first 0x00 or
/// 0xFF on, 0x00), the halves XORed.
pub fn name_check(name: &str) -> u16 {
    let mut r = [0u8; 16];
    put_name(&mut r, 0, 16, name);
    let mut h: u32 = 0x811C_9DC5;
    let mut end = false;
    for &c in &r {
        end = end || c == 0x00 || c == 0xFF;
        h = (h ^ u32::from(if end { 0 } else { c })).wrapping_mul(0x0100_0193);
    }
    ((h ^ (h >> 16)) & 0xFFFF) as u16
}

// --- Tags (2.1) ---

/// A tag as stored: `params` is the kind's parameter block, whatever its length.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tag {
    pub kind: u8,
    pub system: u8,
    pub check: u16,
    pub flags: u8,
    pub fb_chan: u16,
    pub fb_check: u16,
    pub params: Vec<u8>,
}

/// A tag, or `None` when the value is none (too short, or kind 0 or 0xFF).
pub fn decode_tag(v: &[u8]) -> Option<Tag> {
    if v.len() < TAG_HDR || v[0] == 0 || v[0] == 0xFF {
        return None;
    }
    let fb = v[4] & TAG_FALLBACK != 0;
    if fb && v.len() < TAG_HDR + TAG_FB {
        return None;
    }
    Some(Tag {
        kind: v[0],
        system: v[1],
        check: u16_at(v, 2),
        flags: v[4],
        fb_chan: if fb { u16_at(v, 5) } else { 0 },
        fb_check: if fb { u16_at(v, 7) } else { 0 },
        params: v[if fb { TAG_HDR + TAG_FB } else { TAG_HDR }..].to_vec(),
    })
}

pub fn encode_tag(t: &Tag) -> Vec<u8> {
    let at = if t.flags & TAG_FALLBACK != 0 { TAG_HDR + TAG_FB } else { TAG_HDR };
    let mut v = vec![0u8; at + t.params.len()];
    v[0] = t.kind;
    v[1] = t.system;
    v[2..4].copy_from_slice(&t.check.to_le_bytes());
    v[4] = t.flags;
    if at > TAG_HDR {
        v[5..7].copy_from_slice(&t.fb_chan.to_le_bytes());
        v[7..9].copy_from_slice(&t.fb_check.to_le_bytes());
    }
    v[at..].copy_from_slice(&t.params);
    v
}

/// A tag is live for a channel record (2.3): the record's name, and a stand-in shape unless the base stands on its
/// own. Only a live tag counts; any other is stale: kept as found, never applied.
pub fn tag_live(t: &Tag, c: &Channel) -> bool {
    t.check == name_check(&c.name)
        && (t.flags & TAG_BASE_OWN != 0 || (c.mode == Mode::Dmr && c.rx_only))
}

// --- The kinds' parameter blocks ---

pub const NAC_DEFAULT: u16 = 0x293;
pub const NAC_ANY: u16 = 0xF7E;
pub const NAC_REPEAT_ANY: u16 = 0xF7F;
const NAC_SET: u16 = 0x8000;
pub const P25_UNIT_MAX: u32 = 0xFF_FFFE;
/// Above this the editor warns: unit IDs this long are unusual.
pub const P25_UNIT_WARN: u32 = 9_999_999;

/// A P25 conventional channel (p25-codeplug.md 2): squelch 0 normal, 1 selective; the RX and TX NAC.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct P25Conv {
    pub squelch: u8,
    pub rx_nac: u16,
    pub tx_nac: u16,
}

impl Default for P25Conv {
    fn default() -> Self {
        Self { squelch: 0, rx_nac: NAC_DEFAULT, tx_nac: NAC_DEFAULT }
    }
}

/// A NAC field: its value when set, else `dflt`; a value the field can't hold reads as `dflt` too.
fn nac_field(f: u16, dflt: u16) -> u16 {
    if f & NAC_SET != 0 && f & 0x7000 == 0 {
        f & 0x0FFF
    } else {
        dflt
    }
}

fn nac_out(v: u16, dflt: u16) -> u16 {
    if v == dflt {
        0
    } else {
        NAC_SET | (v & 0x0FFF)
    }
}

pub fn decode_p25(b: &[u8]) -> P25Conv {
    let sq = u8_at(b, 0) & 3;
    let mut rx = nac_field(u16_at(b, 1), NAC_DEFAULT);
    if rx == NAC_REPEAT_ANY {
        rx = NAC_DEFAULT;
    }
    let tx_dflt = if rx == NAC_ANY { NAC_DEFAULT } else { rx };
    let mut tx = nac_field(u16_at(b, 3), tx_dflt);
    if tx == NAC_ANY || tx == NAC_REPEAT_ANY {
        tx = tx_dflt;
    }
    P25Conv { squelch: u8::from(sq == 1), rx_nac: rx, tx_nac: tx }
}

pub fn encode_p25(p: &P25Conv, prev: Option<&[u8]>) -> Vec<u8> {
    let mut v = grow(prev, 5);
    v[0] = (v[0] & !3) | u8::from(p.squelch == 1);
    v[1..3].copy_from_slice(&nac_out(p.rx_nac, NAC_DEFAULT).to_le_bytes());
    let tx_dflt = if p.rx_nac == NAC_ANY { NAC_DEFAULT } else { p.rx_nac };
    v[3..5].copy_from_slice(&nac_out(p.tx_nac, tx_dflt).to_le_bytes());
    v
}

pub const RAN_MAX: u8 = 63;
const TX_RAN_SET: u8 = 0x80;
const RATE_4800: u8 = 0x04;
pub const NXDN_UNIT_MAX: u32 = 65519;

/// The TX RAN a block that leaves it unset sends: the RX RAN, RAN 1 when that is every RAN (0).
fn tx_ran_default(rx: u8) -> u8 {
    if rx != 0 {
        rx
    } else {
        1
    }
}

/// An NXDN conventional channel (nxdn-codeplug.md 2): the rate, squelch (0 RAN, 1 selective), the RX and TX RAN.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NxdnConv {
    pub rate9600: bool,
    pub squelch: u8,
    pub rx_ran: u8,
    pub tx_ran: u8,
}

impl Default for NxdnConv {
    fn default() -> Self {
        Self { rate9600: true, squelch: 0, rx_ran: 0, tx_ran: 1 }
    }
}

pub fn decode_nxdn(b: &[u8]) -> NxdnConv {
    let f = u8_at(b, 0);
    let mut rx = u8_at(b, 1);
    if rx > RAN_MAX {
        rx = 0;
    }
    let t = u8_at(b, 2);
    NxdnConv {
        rate9600: f & RATE_4800 == 0,
        squelch: u8::from(f & 3 == 1),
        rx_ran: rx,
        tx_ran: if t & 0xC0 == TX_RAN_SET { t & RAN_MAX } else { tx_ran_default(rx) },
    }
}

pub fn encode_nxdn(p: &NxdnConv, prev: Option<&[u8]>) -> Vec<u8> {
    let mut v = grow(prev, 3);
    let rx = if p.rx_ran <= RAN_MAX { p.rx_ran } else { 0 };
    v[0] = (v[0] & !7) | u8::from(p.squelch == 1) | if p.rate9600 { 0 } else { RATE_4800 };
    v[1] = rx;
    v[2] = if p.tx_ran == tx_ran_default(rx) { 0 } else { TX_RAN_SET | (p.tx_ran & RAN_MAX) };
    v
}

/// A channel's mode parameters: the module's fields for a kind this editor knows, else the block as stored (kept).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Params {
    P25(P25Conv),
    Nxdn(NxdnConv),
    /// A P25 trunked channel (p25-codeplug.md 4): follow every group of the system.
    P25Trunk(P25TrunkChan),
    /// A Tier III channel (dmr-trunking-codeplug.md 4): no parameter block, so the block as stored is kept.
    T3,
    /// A kind this editor doesn't edit: its tag is kept as found.
    Unknown,
}

/// A P25 trunked channel's parameter block: bit 0, follow every group of the system; clear, the talk group and the RX
/// group list only.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct P25TrunkChan {
    pub follow_all: bool,
}

pub fn decode_params(kind: u8, block: &[u8]) -> Params {
    match kind {
        KIND_P25 => Params::P25(decode_p25(block)),
        KIND_NXDN => Params::Nxdn(decode_nxdn(block)),
        KIND_P25_TRUNK => Params::P25Trunk(P25TrunkChan { follow_all: u8_at(block, 0) & 1 != 0 }),
        KIND_T3 => Params::T3,
        _ => Params::Unknown,
    }
}

/// The block for `p`, written over `prev`; `None` for a kind this editor doesn't know.
fn encode_params(p: &Params, prev: Option<&[u8]>) -> Option<Vec<u8>> {
    match p {
        Params::P25(x) => Some(encode_p25(x, prev)),
        Params::Nxdn(x) => Some(encode_nxdn(x, prev)),
        Params::P25Trunk(x) => {
            let mut v = grow(prev, 1);
            v[0] = (v[0] & !1) | u8::from(x.follow_all);
            Some(v)
        }
        // A Tier III channel has no block of its own: what is stored stays.
        Params::T3 => Some(prev.map_or_else(Vec::new, <[u8]>::to_vec)),
        Params::Unknown => None,
    }
}

/// The parameters a new channel of `kind` starts with.
pub fn new_params(kind: u8) -> Params {
    match kind {
        KIND_P25 => Params::P25(P25Conv::default()),
        KIND_NXDN => Params::Nxdn(NxdnConv::default()),
        KIND_P25_TRUNK => Params::P25Trunk(P25TrunkChan::default()),
        KIND_T3 => Params::T3,
        _ => Params::Unknown,
    }
}

// --- A channel of a kind ---

/// A tag's fallback channel (2.1): a channel whose name still matches the stored check, or a number as stored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fallback {
    /// The system's, if any.
    None,
    Channel(u16),
    /// A channel number whose name no longer matches (kept as found).
    Stale(u16),
}

/// What a live tag makes of a channel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tagged {
    pub kind: u8,
    /// The system (0-based) as stored, 0xFF = none. Kinds 2 and 4 have none.
    pub system: u8,
    /// Transmit allowed (flag bit 0), in place of the record's receive-only bit.
    pub tx: bool,
    /// The record stands on its own (flag bit 1): it is a real FM or DMR channel too.
    pub base_own: bool,
    pub fallback: Fallback,
    /// Kind bit 4 of a trunked channel (P25 trunked, Tier III): no fallback on this channel.
    pub no_fallback: bool,
    pub params: Params,
    /// The tag as read: the bytes no field covers come from here when the channel is written.
    pub raw: Vec<u8>,
}

/// The tag flag a P25 trunked channel names (the kind's own bit 4).
pub const TAG_KIND_BIT4: u8 = 0x10;

impl Channel {
    /// The channel's kind: 0 for a plain FM or DMR channel.
    pub fn kind(&self) -> u8 {
        self.tagged.as_ref().map_or(0, |t| t.kind)
    }
}

/// The system a channel names in its tag, if it is there.
pub fn channel_system<'a>(c: &Channel, systems: &'a [System]) -> Option<&'a System> {
    let t = c.tagged.as_ref()?;
    (t.system != 0xFF).then(|| systems.iter().find(|s| s.number == u16::from(t.system) + 1)).flatten()
}

/// The record a channel of a kind keeps (2.3): DMR and receive only, unless the tag says the base stands on its own.
/// A kind without frequencies of its own takes its system's first control channel's.
pub fn stand_in(c: &mut Channel, systems: &[System]) {
    let Some(t) = &c.tagged else { return };
    if t.base_own {
        return;
    }
    c.mode = Mode::Dmr;
    c.rx_only = true;
    if !own_freqs(t.kind) {
        let ctl = channel_system(c, systems).and_then(|s| {
            s.entries.iter().find_map(|e| match &e.body {
                EntryBody::P25Control { rx_hz, tx_hz, .. } | EntryBody::T3Channel { rx_hz, tx_hz, .. }
                    if e.cls == 0 && e.control && *rx_hz != 0 =>
                {
                    Some((*rx_hz, *tx_hz))
                }
                _ => None,
            })
        });
        if let Some((rx, tx)) = ctl {
            c.rx_hz = u64::from(rx);
            c.tx_hz = u64::from(if tx != 0 { tx } else { rx });
        }
    }
}

/// Whether a kind keeps its own frequencies in the record: the conventional kinds do; one that follows a system
/// takes the system's control channel's.
pub fn own_freqs(kind: u8) -> bool {
    matches!(kind, KIND_P25 | KIND_NXDN)
}

/// Channel `c` made of `kind` (0: plain, in `mode`). Fields of the kind it had are dropped; transmit allowed carries
/// over as the record's receive-only bit and back.
pub fn set_kind(c: &mut Channel, kind: u8, mode: Mode, systems: &[System]) {
    if kind == 0 {
        if let Some(t) = &c.tagged {
            // A kind without frequencies of its own left its system's in the record: never a place to transmit.
            c.rx_only = !t.tx || (!t.base_own && !own_freqs(t.kind));
        }
        c.tagged = None;
        c.mode = mode;
        return;
    }
    if c.kind() == kind {
        return;
    }
    let tx = c.tagged.as_ref().map_or(!c.rx_only, |t| t.tx);
    set_kind(c, 0, Mode::Dmr, systems);
    // A kind that runs on a system starts on the first system of its type, if there is one.
    let want = match kind {
        KIND_P25_TRUNK => Some(systems::SYSTEM_P25_TRUNK),
        KIND_T3 => Some(systems::SYSTEM_T3),
        _ => None,
    };
    let system = want
        .and_then(|t| systems.iter().find(|s| s.type_id == t && (1..=255).contains(&s.number)))
        .map_or(0xFF, |s| (s.number - 1) as u8);
    c.tagged = Some(Tagged {
        kind,
        system,
        // Transmit allowed starts as the channel's own where the kind transmits by default (a Tier III site grants
        // every transmission); a P25 trunked channel starts receive only.
        tx: matches!(kind, KIND_P25 | KIND_NXDN | KIND_T3) && tx,
        base_own: false,
        fallback: Fallback::None,
        no_fallback: false,
        params: new_params(kind),
        raw: Vec::new(),
    });
    stand_in(c, systems);
}

// --- Decoding ---

/// The live tags in the settings area onto the channels (after the channels are decoded).
pub fn decode_tags(channels: &mut [Channel], entries: &[Entry]) {
    let names: HashMap<u16, String> = channels.iter().map(|c| (c.number, c.name.clone())).collect();
    let mut fallbacks: Vec<(usize, Tag)> = Vec::new();
    for (i, c) in channels.iter_mut().enumerate() {
        if c.number == 0 || usize::from(c.number) > MAX_TAGS {
            continue;
        }
        let Some(raw) = kv::get_value(entries, TAG_KEY + c.number - 1) else { continue };
        let Some(t) = decode_tag(raw) else { continue };
        if !tag_live(&t, c) {
            continue;
        }
        c.tagged = Some(Tagged {
            kind: t.kind,
            system: t.system,
            tx: t.flags & TAG_TX != 0,
            base_own: t.flags & TAG_BASE_OWN != 0,
            fallback: Fallback::None,
            no_fallback: matches!(t.kind, KIND_P25_TRUNK | KIND_T3) && t.flags & TAG_KIND_BIT4 != 0,
            params: decode_params(t.kind, &t.params),
            raw: raw.to_vec(),
        });
        if t.flags & TAG_FALLBACK != 0 {
            fallbacks.push((i, t));
        }
    }
    // Fallbacks name channels: resolved once every channel has its tag.
    for (i, t) in fallbacks {
        if t.fb_chan == 0 {
            continue;
        }
        let r = match names.get(&t.fb_chan) {
            Some(n) if name_check(n) == t.fb_check => Fallback::Channel(t.fb_chan),
            _ => Fallback::Stale(t.fb_chan),
        };
        if let Some(x) = &mut channels[i].tagged {
            x.fallback = r;
        }
    }
}

/// The kinds' radio-wide records (5.5): the unit IDs. 0 = not set: the radio's DMR ID is the unit ID.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ModeSettings {
    /// P25 (kind 2; the trunked kind reads it too): 1 to 0xFFFFFE.
    pub p25_unit_id: u32,
    /// NXDN (kind 4): 1 to 65519.
    pub nxdn_unit_id: u16,
}

pub fn decode_settings(entries: &[Entry]) -> ModeSettings {
    let p25 = kv::get_value(entries, SETTING_KEY + u16::from(KIND_P25));
    let nxdn = kv::get_value(entries, SETTING_KEY + u16::from(KIND_NXDN));
    let id = p25.filter(|b| b.len() >= 4).map_or(0, |b| u32_at(b, 0));
    let nid = nxdn.filter(|b| b.len() >= 2).map_or(0, |b| u16_at(b, 0));
    ModeSettings {
        p25_unit_id: if (1..=P25_UNIT_MAX).contains(&id) { id } else { 0 },
        nxdn_unit_id: if (1..=NXDN_UNIT_MAX).contains(&u32::from(nid)) { nid } else { 0 },
    }
}

// --- Writing ---

/// A fallback as stored: the channel number and the name check. A number as read keeps the check stored with it.
fn fallback_out(f: &Fallback, names: &HashMap<u16, &str>, src: Option<&Tag>) -> (u16, u16) {
    match f {
        Fallback::None => (0, 0),
        Fallback::Channel(n) => match names.get(n) {
            Some(name) => (*n, name_check(name)),
            None => (0, 0),
        },
        Fallback::Stale(n) => (*n, src.filter(|s| s.fb_chan == *n).map_or(0, |s| s.fb_check)),
    }
}

/// The names of the channels an image stores, by number.
pub fn channel_names(b: &[u8]) -> HashMap<u16, String> {
    let mut out = HashMap::new();
    for n in 1..=MAX_CHANNELS {
        if !channel_used(b, n) {
            continue;
        }
        let a = channel_addr(n);
        if let Some(c) = crate::records::decode_channel(n as u16, &b[a..a + CH_SIZE]) {
            out.insert(n as u16, c.name);
        }
    }
    out
}

fn channel_used(b: &[u8], n: usize) -> bool {
    let (bank, k) = ((n - 1) / crate::records::CH_PER_BANK, (n - 1) % crate::records::CH_PER_BANK);
    b[crate::records::bank_bitmap(bank) + (k >> 3)] & (1 << (k & 7)) != 0
}

pub(crate) fn set(out: &mut Vec<Entry>, key: u16, v: &[u8]) {
    if kv::get_value(out, key) != Some(v) {
        *out = kv::set_value(out, key, v);
    }
}

pub(crate) fn del(out: &mut Vec<Entry>, key: u16) {
    if kv::get_value(out, key).is_some() {
        *out = kv::delete_value(out, key);
    }
}

/// What the writer takes from the codeplug: the channels, the kinds' radio-wide records, the systems (with their
/// entries and secrets) and the entries of systems the model doesn't have, kept.
pub struct ModesIn<'a> {
    pub channels: &'a [Channel],
    pub settings: &'a ModeSettings,
    pub systems: &'a [System],
    pub orphans: &'a [Vec<u8>],
}

/// The area's entries with the systems, the channels' tags and the kinds' radio-wide records in place of those stored. Another tag
/// goes when no channel is at its number, or when its check is the name there or the name the image had there (2.4);
/// else it is stale and kept. `base_names` gives the names the image the codeplug came from stores, by number.
pub fn mode_entries(
    m: &ModesIn,
    entries: &[Entry],
    base_names: impl FnOnce() -> HashMap<u16, String>,
) -> Vec<Entry> {
    let (channels, settings) = (m.channels, m.settings);
    let mut out = entries.to_vec();
    let names: HashMap<u16, &str> =
        channels.iter().filter(|c| c.number != 0).map(|c| (c.number, c.name.as_str())).collect();
    // Systems, their entries and secrets first, then the tags that name them.
    out = systems::system_entries(m.systems, m.orphans, out, &names);
    let in_tags = |k: u16| (TAG_KEY..TAG_KEY + MAX_TAGS as u16).contains(&k);
    let tagged: Vec<(u16, &Channel)> = channels
        .iter()
        .filter(|c| c.tagged.is_some() && c.number != 0)
        .map(|c| (TAG_KEY + c.number - 1, c))
        .collect();
    let mut base_names = Some(base_names);
    let mut was: Option<HashMap<u16, String>> = None;
    for e in entries.iter().filter(|e| in_tags(e.key)) {
        if tagged.iter().any(|(k, _)| *k == e.key) {
            continue;
        }
        let n = e.key - TAG_KEY + 1;
        let t = decode_tag(&e.value);
        if was.is_none() {
            was = base_names.take().map(|f| f());
        }
        let gone = match names.get(&n) {
            None => true,
            Some(name) => t.is_some_and(|t| {
                t.check == name_check(name)
                    || was.as_ref().and_then(|w| w.get(&n)).is_some_and(|w| t.check == name_check(w))
            }),
        };
        if gone {
            del(&mut out, e.key);
        }
    }
    for (key, c) in tagged {
        let Some(tg) = &c.tagged else { continue };
        let here = kv::get_value(&out, key).and_then(decode_tag);
        let read = decode_tag(&tg.raw);
        // The tag the channel came with: the one at its number, else the one it had where it was read.
        let src = here.filter(|h| h.kind == tg.kind).or_else(|| read.filter(|r| r.kind == tg.kind));
        let mut params = src.as_ref().map_or_else(Vec::new, |s| s.params.clone());
        let unchanged = src.as_ref().is_some_and(|s| decode_params(tg.kind, &s.params) == tg.params);
        if !unchanged {
            if let Some(p) = encode_params(&tg.params, src.as_ref().map(|s| s.params.as_slice())) {
                params = p;
            }
        }
        let mut flags = src.as_ref().map_or(0, |s| s.flags & (0xF0 | TAG_RESERVED));
        if tg.tx {
            flags |= TAG_TX;
        }
        if tg.base_own {
            flags |= TAG_BASE_OWN;
        }
        let (fb_chan, fb_check) = fallback_out(&tg.fallback, &names, src.as_ref());
        if matches!(tg.kind, KIND_P25_TRUNK | KIND_T3) {
            flags = if tg.no_fallback { flags | TAG_KIND_BIT4 } else { flags & !TAG_KIND_BIT4 };
        }
        if fb_chan != 0 {
            flags |= TAG_FALLBACK;
        }
        let v = encode_tag(&Tag {
            kind: tg.kind,
            system: tg.system,
            check: name_check(&c.name),
            flags,
            fb_chan,
            fb_check,
            params,
        });
        set(&mut out, key, &v);
    }

    // The kinds' radio-wide records: written when a field changed; none when the unit ID is 0.
    let have = decode_settings(&out);
    if have.p25_unit_id != settings.p25_unit_id {
        let key = SETTING_KEY + u16::from(KIND_P25);
        if settings.p25_unit_id == 0 {
            del(&mut out, key);
        } else {
            let mut v = grow(kv::get_value(&out, key), 4);
            v[..4].copy_from_slice(&settings.p25_unit_id.to_le_bytes());
            set(&mut out, key, &v);
        }
    }
    if have.nxdn_unit_id != settings.nxdn_unit_id {
        let key = SETTING_KEY + u16::from(KIND_NXDN);
        if settings.nxdn_unit_id == 0 {
            del(&mut out, key);
        } else {
            let mut v = grow(kv::get_value(&out, key), 2);
            v[..2].copy_from_slice(&settings.nxdn_unit_id.to_le_bytes());
            set(&mut out, key, &v);
        }
    }
    out
}

/// The channels' tags and the radio-wide records written into an image's settings area, only when they change it.
/// An area of a newer version is never written. False when what is to be written does not fit the area.
pub fn encode_modes(
    m: &ModesIn,
    b: &mut [u8],
    base_names: impl FnOnce() -> HashMap<u16, String>,
) -> bool {
    let Some(entries) = kv::entries_for_write(b) else { return true };
    let out = mode_entries(m, &entries, base_names);
    kv::put_entries(b, &entries, &out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn me(
        channels: &[Channel],
        settings: &ModeSettings,
        entries: &[Entry],
        base: impl FnOnce() -> HashMap<u16, String>,
    ) -> Vec<Entry> {
        mode_entries(&ModesIn { channels, settings, systems: &[], orphans: &[] }, entries, base)
    }

    #[test]
    fn the_name_check_matches_the_spec() {
        assert_eq!(name_check(""), 0x706C);
        assert_eq!(name_check("A"), 0x18A9);
        assert_eq!(name_check("T3 Ops"), 0x1687);
        assert_eq!(name_check("Sixteen chars!!!"), 0xCDD2);
    }

    #[test]
    fn a_tag_round_trips_with_and_without_a_fallback() {
        let t = Tag { kind: 2, system: 0xFF, check: 0x1234, flags: TAG_TX, fb_chan: 0, fb_check: 0, params: vec![1, 2, 3] };
        assert_eq!(decode_tag(&encode_tag(&t)), Some(t));
        let t = Tag { kind: 3, system: 4, check: 9, flags: TAG_TX | TAG_FALLBACK, fb_chan: 7, fb_check: 0xBEEF, params: vec![9] };
        let v = encode_tag(&t);
        assert_eq!(v.len(), 5 + 4 + 1);
        assert_eq!(decode_tag(&v), Some(t));
        // Kind 0 and 0xFF are no tag; a fallback flag with no room for it is none.
        assert_eq!(decode_tag(&[0, 0xFF, 0, 0, 1]), None);
        assert_eq!(decode_tag(&[0xFF, 0xFF, 0, 0, 1]), None);
        assert_eq!(decode_tag(&[2, 0xFF, 0, 0, TAG_FALLBACK, 1, 0]), None);
    }

    #[test]
    fn the_p25_block_reads_by_the_spec() {
        // A zeroed block, or none: normal squelch on NAC 0x293 both ways.
        assert_eq!(decode_p25(&[]), P25Conv::default());
        assert_eq!(decode_p25(&[0; 5]), P25Conv::default());
        // RX NAC 0x123 set, TX NAC left to the RX one, selective squelch.
        let b = [1, 0x23, 0x81, 0, 0];
        assert_eq!(decode_p25(&b), P25Conv { squelch: 1, rx_nac: 0x123, tx_nac: 0x123 });
        // 0xF7F is refused as an RX NAC (read as 0x293); 0xF7E and 0xF7F as a TX NAC (read as the default).
        assert_eq!(decode_p25(&[0, 0x7F, 0x8F, 0, 0]).rx_nac, NAC_DEFAULT);
        assert_eq!(decode_p25(&[0, 0, 0, 0x7E, 0x8F]).tx_nac, NAC_DEFAULT);
        // A value with bits 12-14 set can't be held: the default.
        assert_eq!(decode_p25(&[0, 0x23, 0xB1, 0, 0]).rx_nac, NAC_DEFAULT);
    }

    #[test]
    fn the_p25_block_writes_over_what_is_stored() {
        let p = P25Conv { squelch: 1, rx_nac: 0x123, tx_nac: 0x456 };
        let v = encode_p25(&p, None);
        assert_eq!(decode_p25(&v), p);
        // The default NAC is stored as unset (0).
        let d = encode_p25(&P25Conv::default(), None);
        assert_eq!(d, vec![0, 0, 0, 0, 0]);
        // Bytes after the ones this version writes are kept; bits 2-7 of the first byte too.
        let prev = [0xFC, 0, 0, 0, 0, 7, 8];
        let v = encode_p25(&p, Some(&prev));
        assert_eq!(v.len(), 7);
        assert_eq!(&v[5..], &[7, 8]);
        assert_eq!(v[0], 0xFD);
    }

    #[test]
    fn the_nxdn_block_reads_and_writes_by_the_spec() {
        // A zeroed block: 9600 bit/s, RAN squelch, every RAN received and RAN 1 sent.
        assert_eq!(decode_nxdn(&[]), NxdnConv::default());
        assert_eq!(decode_nxdn(&[0, 0, 0]), NxdnConv::default());
        let p = NxdnConv { rate9600: false, squelch: 1, rx_ran: 5, tx_ran: 9 };
        let v = encode_nxdn(&p, None);
        assert_eq!(v, vec![0x05, 5, 0x89]);
        assert_eq!(decode_nxdn(&v), p);
        // A TX RAN equal to the default is stored unset; a RAN above 63 reads as 0.
        assert_eq!(encode_nxdn(&NxdnConv { rx_ran: 7, tx_ran: 7, ..NxdnConv::default() }, None)[2], 0);
        assert_eq!(decode_nxdn(&[0, 70, 0]).rx_ran, 0);
    }

    fn ch(n: u16, name: &str) -> Channel {
        let mut c = Channel::new(n, 146_520_000, 146_520_000);
        c.name = name.into();
        c
    }

    fn tagged_p25(n: u16, name: &str) -> Channel {
        let mut c = ch(n, name);
        set_kind(&mut c, KIND_P25, Mode::Dmr, &[]);
        c
    }

    #[test]
    fn making_a_channel_of_a_kind_leaves_a_safe_stand_in() {
        let mut c = ch(1, "One");
        assert!(!c.rx_only);
        set_kind(&mut c, KIND_P25, Mode::Dmr, &[]);
        assert_eq!(c.kind(), KIND_P25);
        // Transmit allowed carries over; the record is DMR and receive only.
        assert!(c.tagged.as_ref().unwrap().tx);
        assert!(c.rx_only && c.mode == Mode::Dmr);
        // Back to plain: transmit allowed returns as the receive-only bit.
        set_kind(&mut c, 0, Mode::Fm, &[]);
        assert_eq!((c.kind(), c.mode, c.rx_only), (0, Mode::Fm, false));
    }

    #[test]
    fn a_tag_written_for_a_channel_is_read_back_live() {
        let mut p = tagged_p25(3, "P25 Test");
        if let Some(t) = &mut p.tagged {
            t.params = Params::P25(P25Conv { squelch: 1, rx_nac: 0x123, tx_nac: 0x123 });
        }
        let entries = me(&[p.clone()], &ModeSettings::default(), &[], HashMap::new);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].key, TAG_KEY + 2);
        let mut back = vec![p.clone()];
        back[0].tagged = None;
        decode_tags(&mut back, &entries);
        let t = back[0].tagged.as_ref().unwrap();
        assert_eq!((t.kind, t.tx), (KIND_P25, true));
        assert_eq!(t.params, p.tagged.as_ref().unwrap().params);
    }

    #[test]
    fn renaming_a_tagged_channel_makes_the_tag_follow_it() {
        let p = tagged_p25(3, "Old Name");
        let e1 = me(std::slice::from_ref(&p), &ModeSettings::default(), &[], HashMap::new);
        let mut renamed = p.clone();
        renamed.name = "New Name".into();
        let names = HashMap::from([(3u16, "Old Name".to_string())]);
        let e2 = me(&[renamed.clone()], &ModeSettings::default(), &e1, || names);
        let t = decode_tag(&e2[0].value).unwrap();
        assert_eq!(t.check, name_check("New Name"));
        assert_eq!(e2.len(), 1);
    }

    #[test]
    fn a_plain_channel_at_a_tagged_number_removes_the_stale_tag() {
        let p = tagged_p25(3, "Was P25");
        let e1 = me(std::slice::from_ref(&p), &ModeSettings::default(), &[], HashMap::new);
        // The channel is made plain: its tag's check is the old name's, so the tag goes.
        let mut plain = p.clone();
        set_kind(&mut plain, 0, Mode::Fm, &[]);
        let names = HashMap::from([(3u16, "Was P25".to_string())]);
        let e2 = me(&[plain], &ModeSettings::default(), &e1, || names.clone());
        assert!(e2.is_empty());
        // The channel is deleted: no channel at the number, the tag goes.
        let e3 = me(&[], &ModeSettings::default(), &e1, || names);
        assert!(e3.is_empty());
    }

    #[test]
    fn a_tag_that_names_neither_the_old_nor_the_new_name_is_kept_as_stale() {
        let p = tagged_p25(3, "Was P25");
        let e1 = me(std::slice::from_ref(&p), &ModeSettings::default(), &[], HashMap::new);
        // Another tool put a different plain channel at 3 and the image had that name: the tag is stale, kept.
        let other = ch(3, "Something Else");
        let names = HashMap::from([(3u16, "Something Else".to_string())]);
        let e2 = me(&[other], &ModeSettings::default(), &e1, || names);
        assert_eq!(e2, e1);
    }

    #[test]
    fn a_tag_of_a_kind_this_editor_does_not_know_is_kept_whole() {
        // A channel of kind 9 (none this editor knows) with a system and bytes it can't read.
        let t = Tag { kind: 9, system: 2, check: name_check("Trunk"), flags: TAG_TX | 0x10, fb_chan: 0, fb_check: 0, params: vec![1, 9, 9] };
        let raw = encode_tag(&t);
        let entries = vec![Entry { key: TAG_KEY + 4, value: raw.clone() }];
        let mut c = ch(5, "Trunk");
        c.mode = Mode::Dmr;
        c.rx_only = true;
        let mut chans = vec![c];
        decode_tags(&mut chans, &entries);
        assert_eq!(chans[0].kind(), 9);
        assert_eq!(chans[0].tagged.as_ref().unwrap().params, Params::Unknown);
        // Writing it back changes nothing.
        let out = me(&chans, &ModeSettings::default(), &entries, HashMap::new);
        assert_eq!(out, entries);
    }

    #[test]
    fn a_p25_trunked_tag_reads_and_writes_back_unchanged() {
        // Kind 3 on system 2 (0-based), transmit off, kind bit 4 (no fallback), follow every group, a trailing byte.
        let t = Tag { kind: 3, system: 1, check: name_check("Trunk"), flags: 0x10, fb_chan: 0, fb_check: 0, params: vec![1, 7] };
        let entries = vec![Entry { key: TAG_KEY + 4, value: encode_tag(&t) }];
        let mut c = ch(5, "Trunk");
        c.mode = Mode::Dmr;
        c.rx_only = true;
        let mut chans = vec![c];
        decode_tags(&mut chans, &entries);
        let tg = chans[0].tagged.as_ref().unwrap();
        assert_eq!((tg.kind, tg.system, tg.tx, tg.no_fallback), (KIND_P25_TRUNK, 1, false, true));
        assert_eq!(tg.params, Params::P25Trunk(P25TrunkChan { follow_all: true }));
        assert_eq!(me(&chans, &ModeSettings::default(), &entries, HashMap::new), entries);
        // Turning follow-all off keeps the other bits and the trailing byte.
        if let Some(t) = &mut chans[0].tagged {
            t.params = Params::P25Trunk(P25TrunkChan { follow_all: false });
        }
        let out = me(&chans, &ModeSettings::default(), &entries, HashMap::new);
        assert_eq!(decode_tag(&out[0].value).unwrap().params, vec![0, 7]);
    }

    #[test]
    fn the_unit_ids_round_trip_and_zero_removes_the_key() {
        let s = ModeSettings { p25_unit_id: 3_100_001, nxdn_unit_id: 4242 };
        let e = me(&[], &s, &[], HashMap::new);
        assert_eq!(decode_settings(&e), s);
        assert_eq!(kv::get_value(&e, SETTING_KEY + 2), Some(&3_100_001u32.to_le_bytes()[..]));
        assert_eq!(kv::get_value(&e, SETTING_KEY + 4), Some(&4242u16.to_le_bytes()[..]));
        let none = me(&[], &ModeSettings::default(), &e, HashMap::new);
        assert!(none.is_empty());
        // Out of range reads as unset.
        let bad = vec![
            Entry { key: SETTING_KEY + 2, value: 0xFFFF_FFFFu32.to_le_bytes().to_vec() },
            Entry { key: SETTING_KEY + 4, value: 65520u16.to_le_bytes().to_vec() },
        ];
        assert_eq!(decode_settings(&bad), ModeSettings::default());
    }
}
