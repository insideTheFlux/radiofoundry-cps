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
/// Tier III's own records of system n (kept as stored, removed with the system).
pub const T3_POLL_KEY: u16 = 0x0800;
pub const T3_SUPL_KEY: u16 = 0x0900;
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

pub const SYSTEM_T3: u8 = 1;
pub const SYSTEM_P25_TRUNK: u8 = 3;
pub const SYSTEM_NXDN: u8 = 4;
pub const SYSTEM_AES: u8 = 5;

/// Traffic keys (channel-modes.md 5.4): key 0x1900 + n, 1024 of them. Only an NXDN conventional system has any this
/// editor reads; the records of other types are kept as stored.
pub const TRAFFIC_KEY: u16 = 0x1900;
pub const MAX_TRAFFIC: usize = 1024;
/// Marks the entries too many for their family (never stored).
pub const OVERFLOW_KEY: u16 = 0xFFFF;
/// A traffic key's algorithm: the NXDN Scrambler. Key IDs 0 to 63, seeds 1 to 32767 (nxdn-codeplug.md 3).
pub const SCRAMBLER: u8 = 1;
pub const KEY_ID_MAX: u16 = 63;
pub const SEED_MAX: u16 = 32767;

/// A scrambler key of an NXDN system. `slot` is the key (0x1900 up) it was read from, `None` for a new one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrafficKey {
    pub slot: Option<u16>,
    pub id: u16,
    pub seed: u16,
    pub flags: u8,
}

/// A traffic-key record's key, or `None` when it is no scrambler key this editor reads (kept as stored then).
pub fn decode_traffic(v: &[u8]) -> Option<TrafficKey> {
    if v.len() != 7 || v[1] != SCRAMBLER {
        return None;
    }
    let (id, seed) = (u16_at(v, 2), u16_at(v, 5));
    (id <= KEY_ID_MAX && (1..=SEED_MAX).contains(&seed)).then_some(TrafficKey { slot: None, id, seed, flags: v[4] })
}

/// A key's record for system `system` (0-based).
pub fn encode_traffic(system: u8, k: &TrafficKey) -> Vec<u8> {
    let mut v = vec![system, SCRAMBLER, 0, 0, k.flags, 0, 0];
    v[2..4].copy_from_slice(&k.id.to_le_bytes());
    v[5..7].copy_from_slice(&k.seed.to_le_bytes());
    v
}

/// Whether a system type keeps traffic keys this editor reads.
pub fn has_traffic(type_id: u8) -> bool {
    matches!(type_id, SYSTEM_NXDN | SYSTEM_AES)
}

/// A traffic key's algorithm for DMR AES-256 voice, and the key's length (channel-modes.md 4: the PI header's ALG).
pub const AES_ALG: u8 = 0x25;
pub const AES_KEY_BYTES: usize = 32;
const AES_RECORD: usize = 5 + AES_KEY_BYTES;

/// An AES-256 traffic key of a DMR AES system. `slot` is the key (0x1900 up) it was read from, `None` for a new one. `id`
/// is the eight-bit PI key ID the call carries.
#[derive(Clone, PartialEq, Eq)]
pub struct AesKey {
    pub slot: Option<u16>,
    pub id: u8,
    pub key: [u8; AES_KEY_BYTES],
    pub flags: u8,
}

// A key is a secret: it never reaches a log or a failed assertion's output.
impl std::fmt::Debug for AesKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "AesKey {{ slot: {:?}, id: {}, key: <hidden>, flags: {} }}", self.slot, self.id, self.flags)
    }
}

/// A 32-byte AES key as 64 hex digits.
pub fn key_hex_32(k: &[u8; AES_KEY_BYTES]) -> String {
    k.iter().map(|b| format!("{b:02X}")).collect()
}

/// An AES key from 64 hex digits (spaces and a 0x prefix are allowed), or `None`.
pub fn parse_key_hex_32(text: &str) -> Option<[u8; AES_KEY_BYTES]> {
    let t: String = text.trim().trim_start_matches("0x").chars().filter(|c| !c.is_whitespace()).collect();
    if t.len() != 2 * AES_KEY_BYTES || !t.is_ascii() {
        return None;
    }
    let mut out = [0u8; AES_KEY_BYTES];
    for (i, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(&t[2 * i..2 * i + 2], 16).ok()?;
    }
    Some(out)
}

/// A traffic-key record's AES key (system, algorithm, key ID as a u16 whose high byte is 0, flags, 32 key bytes), or
/// `None` when it is none this editor reads (kept as stored then).
pub fn decode_aes_key(v: &[u8]) -> Option<AesKey> {
    if v.len() != AES_RECORD || v[1] != AES_ALG || v[3] != 0 {
        return None;
    }
    let mut key = [0u8; AES_KEY_BYTES];
    key.copy_from_slice(&v[5..]);
    Some(AesKey { slot: None, id: v[2], key, flags: v[4] })
}

/// A key's record for system `system` (0-based).
pub fn encode_aes_key(system: u8, k: &AesKey) -> Vec<u8> {
    let mut v = vec![system, AES_ALG, k.id, 0, k.flags];
    v.extend_from_slice(&k.key);
    v
}

/// A traffic-key record's owner-type check: is `v` a key this editor reads for a system of `type_id`?
fn reads_traffic(type_id: u8, v: &[u8]) -> bool {
    match type_id {
        SYSTEM_NXDN => decode_traffic(v).is_some(),
        SYSTEM_AES => decode_aes_key(v).is_some(),
        _ => false,
    }
}

pub fn type_name(t: u8) -> &'static str {
    match t {
        SYSTEM_T3 => "T3",
        SYSTEM_P25_TRUNK => "P25T",
        SYSTEM_NXDN => "NXDN",
        SYSTEM_AES => "AES",
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

// --- The DMR Tier III system's block (dmr-trunking-codeplug.md 1) ---

pub const T3_BLOCK: usize = 39;
pub const T3_SITES: usize = 8;
pub const T3_LCN: u16 = 4094;
/// A system takes this many control channels; more are skipped.
pub const T3_CTRL: usize = 4;
const T3_ID_LEN: u8 = 4;
/// The network model's NET and site bits: tiny, small, large, huge.
const NET_BITS: [u32; 4] = [9, 7, 4, 2];

/// The most a NET number can be in a network model.
pub fn net_max(model: u8) -> u16 {
    ((1u32 << NET_BITS[usize::from(model & 3)]) - 1) as u16
}

/// The most a site value can be in a network model.
pub fn site_max(model: u8) -> u16 {
    ((1u32 << (12 - NET_BITS[usize::from(model & 3)])) - 1) as u16
}

/// The options word's named bits (the others are kept as stored).
pub const OPT_ALL_CALLS: u16 = 0x0002;
pub const OPT_EMERGENCY_ANY: u16 = 0x0004;
pub const OPT_FOLLOW_ALL: u16 = 0x0008;
pub const OPT_LEARN_SYSTEM: u16 = 0x0010;
pub const OPT_PRIVATE_CALLS: u16 = 0x0020;
pub const OPT_REGISTER_TALKGROUPS: u16 = 0x0040;
pub const OPT_ACCEPT_ALS: u16 = 0x0080;
pub const OPT_ACCEPT_KILL: u16 = 0x0100;
pub const OPT_NO_ROAMING: u16 = 0x0200;
pub const OPT_ACCEPT_STUN: u16 = 0x0400;
pub const OPT_IP_GATEWAY: u16 = 0x1000;
const OPT_MASK: u16 = OPT_ALL_CALLS
    | OPT_EMERGENCY_ANY
    | OPT_FOLLOW_ALL
    | OPT_LEARN_SYSTEM
    | OPT_PRIVATE_CALLS
    | OPT_REGISTER_TALKGROUPS
    | OPT_ACCEPT_ALS
    | OPT_ACCEPT_KILL
    | OPT_NO_ROAMING
    | OPT_ACCEPT_STUN
    | OPT_IP_GATEWAY;

/// A DMR Tier III system: the network, the options, the fixed channel plan and the sites the radio may use.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct T3System {
    /// 0 tiny, 1 small, 2 large, 3 huge.
    pub model: u8,
    pub net: u16,
    /// Category B (else A).
    pub category_b: bool,
    pub color_code: u8,
    pub all_calls: bool,
    pub emergency_any: bool,
    /// Play calls on every talkgroup of the system, not only the channel's RX group list.
    pub follow_all: bool,
    /// Take the first control channel heard as this system (only while the radio listens).
    pub learn_system: bool,
    pub private_calls: bool,
    pub register_talkgroups: bool,
    pub accept_als: bool,
    pub accept_kill: bool,
    pub no_roaming: bool,
    pub accept_stun: bool,
    pub ip_gateway: bool,
    /// Seconds on the fallback channel before the system is tried again; 0 = 30.
    pub fallback_hold: u8,
    /// The fixed plan: the MS transmit frequency of the first channel (0 = no fixed plan), its LCN, the separation
    /// code (an index into 5, 6.25, 10, 12.5, 15, 20, 25, 30 kHz), the duplex split and the two plan flags.
    pub plan_base_hz: u32,
    pub plan_first_lcn: u16,
    pub plan_sep: u8,
    pub plan_tx_low: bool,
    pub plan_no_learn: bool,
    pub plan_split_hz: u32,
    /// Weak below this dBm; 0 = the radio's roaming threshold.
    pub leave_dbm: i8,
    /// Power save asked for at registration: 0 off, 1-7 = 1:2 to 1:128.
    pub power_save: u8,
    /// The site values the radio may use (8 at most); empty = any.
    pub sites: Vec<u16>,
}

impl Default for T3System {
    /// A new system, as the web editor makes one: model tiny, NET 0, category A, color code 1, private calls on.
    fn default() -> Self {
        Self {
            model: 0,
            net: 0,
            category_b: false,
            color_code: 1,
            all_calls: false,
            emergency_any: false,
            follow_all: false,
            learn_system: false,
            private_calls: true,
            register_talkgroups: false,
            accept_als: false,
            accept_kill: false,
            no_roaming: false,
            accept_stun: false,
            ip_gateway: false,
            fallback_hold: 0,
            plan_base_hz: 0,
            plan_first_lcn: 0,
            plan_sep: 0,
            plan_tx_low: false,
            plan_no_learn: false,
            plan_split_hz: 0,
            leave_dbm: 0,
            power_save: 0,
            sites: Vec::new(),
        }
    }
}

pub fn decode_t3(v: &[u8]) -> T3System {
    let opts = u16_at(v, 5);
    let has = |bit: u16| opts & bit != 0;
    let ps = u8_at(v, 21);
    T3System {
        model: u8_at(v, 0) & 3,
        net: u16_at(v, 1),
        category_b: u8_at(v, 3) == 2,
        color_code: u8_at(v, 4) & 0x0F,
        all_calls: has(OPT_ALL_CALLS),
        emergency_any: has(OPT_EMERGENCY_ANY),
        follow_all: has(OPT_FOLLOW_ALL),
        learn_system: has(OPT_LEARN_SYSTEM),
        private_calls: has(OPT_PRIVATE_CALLS),
        register_talkgroups: has(OPT_REGISTER_TALKGROUPS),
        accept_als: has(OPT_ACCEPT_ALS),
        accept_kill: has(OPT_ACCEPT_KILL),
        no_roaming: has(OPT_NO_ROAMING),
        accept_stun: has(OPT_ACCEPT_STUN),
        ip_gateway: has(OPT_IP_GATEWAY),
        fallback_hold: u8_at(v, 7),
        plan_base_hz: u32_at(v, 8),
        plan_first_lcn: u16_at(v, 12),
        plan_sep: u8_at(v, 14) & 7,
        plan_tx_low: u8_at(v, 15) & 1 != 0,
        plan_no_learn: u8_at(v, 15) & 2 != 0,
        plan_split_hz: u32_at(v, 16),
        leave_dbm: s8_at(v, 20),
        power_save: if ps <= 7 { ps } else { 0 },
        sites: (0..usize::from(u8_at(v, 22)).min(T3_SITES)).map(|i| u16_at(v, 23 + 2 * i)).collect(),
    }
}

pub fn encode_t3(s: &T3System, prev: Option<&[u8]>) -> Vec<u8> {
    let mut v = grow(prev, T3_BLOCK);
    v[0] = (v[0] & !3) | (s.model & 3);
    v[1..3].copy_from_slice(&s.net.to_le_bytes());
    v[3] = if s.category_b { 2 } else { 1 };
    v[4] = s.color_code & 0x0F;
    let bits = [
        (s.all_calls, OPT_ALL_CALLS),
        (s.emergency_any, OPT_EMERGENCY_ANY),
        (s.follow_all, OPT_FOLLOW_ALL),
        (s.learn_system, OPT_LEARN_SYSTEM),
        (s.private_calls, OPT_PRIVATE_CALLS),
        (s.register_talkgroups, OPT_REGISTER_TALKGROUPS),
        (s.accept_als, OPT_ACCEPT_ALS),
        (s.accept_kill, OPT_ACCEPT_KILL),
        (s.no_roaming, OPT_NO_ROAMING),
        (s.accept_stun, OPT_ACCEPT_STUN),
        (s.ip_gateway, OPT_IP_GATEWAY),
    ]
    .iter()
    .fold(0u16, |a, &(on, bit)| a | if on { bit } else { 0 });
    let opts = (u16_at(&v, 5) & !OPT_MASK) | bits;
    v[5..7].copy_from_slice(&opts.to_le_bytes());
    v[7] = s.fallback_hold;
    v[8..12].copy_from_slice(&s.plan_base_hz.to_le_bytes());
    v[12..14].copy_from_slice(&s.plan_first_lcn.to_le_bytes());
    v[14] = s.plan_sep & 7;
    v[15] = (v[15] & !3) | u8::from(s.plan_tx_low) | (u8::from(s.plan_no_learn) << 1);
    v[16..20].copy_from_slice(&s.plan_split_hz.to_le_bytes());
    v[20] = s.leave_dbm as u8;
    v[21] = s.power_save & 7;
    let sites = &s.sites[..s.sites.len().min(T3_SITES)];
    v[22] = sites.len() as u8;
    v[23..39].fill(0);
    for (i, x) in sites.iter().enumerate() {
        v[23 + 2 * i..25 + 2 * i].copy_from_slice(&x.to_le_bytes());
    }
    v
}

// --- The Tier III key (dmr-trunking-codeplug.md 3) ---

/// The system's key: 16 octets (the first the most significant), and the PSN (3 octets; 0 = none).
#[derive(Clone, PartialEq, Eq)]
pub struct T3Key {
    pub key: [u8; 16],
    pub psn: u32,
}

// A key never prints: it is a secret.
impl std::fmt::Debug for T3Key {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("T3Key(..)")
    }
}

/// The key a secret holds; `None` when it is shorter than 16 octets (no key).
pub fn decode_t3_key(v: &[u8]) -> Option<T3Key> {
    if v.len() < 16 {
        return None;
    }
    let mut key = [0u8; 16];
    key.copy_from_slice(&v[..16]);
    let psn = if v.len() >= 19 { (u32::from(v[16]) << 16) | (u32::from(v[17]) << 8) | u32::from(v[18]) } else { 0 };
    Some(T3Key { key, psn })
}

/// The secret for a key: 16 octets with no PSN, else 19; octets after the PSN of a stored longer secret are kept.
pub fn encode_t3_key(k: &T3Key, prev: Option<&[u8]>) -> Vec<u8> {
    let mut v = if k.psn != 0 {
        grow(prev.filter(|p| p.len() >= 19), 19)
    } else {
        vec![0u8; 16]
    };
    v[..16].copy_from_slice(&k.key);
    if k.psn != 0 {
        v[16] = (k.psn >> 16) as u8;
        v[17] = (k.psn >> 8) as u8;
        v[18] = k.psn as u8;
    }
    v
}

/// The key as 32 hex digits, upper case.
pub fn key_hex(k: &[u8; 16]) -> String {
    k.iter().map(|b| format!("{b:02X}")).collect()
}

/// 32 hex digits (spaces, colons and dashes ignored) as a key; `None` for anything else.
pub fn parse_key_hex(text: &str) -> Option<[u8; 16]> {
    let t: String = text.chars().filter(|c| !matches!(c, ' ' | ':' | '-' | '\t')).collect();
    if t.len() != 32 || !t.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let mut out = [0u8; 16];
    for (i, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(&t[2 * i..2 * i + 2], 16).ok()?;
    }
    Some(out)
}

/// A system's type-specific fields: those of a type this editor edits, else the block as stored (kept).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SystemFields {
    P25Trunk(P25Trunk),
    T3(T3System),
    /// An NXDN conventional system: an empty parameter block (nxdn-codeplug.md 3).
    Nxdn,
    /// A DMR AES-256 system: one identity byte, the network (channel-modes.md 4).
    Aes { network: u8 },
    Unknown,
}

fn decode_fields(type_id: u8, block: &[u8]) -> SystemFields {
    match type_id {
        SYSTEM_P25_TRUNK => SystemFields::P25Trunk(decode_p25_trunk(block)),
        SYSTEM_T3 => SystemFields::T3(decode_t3(block)),
        SYSTEM_NXDN => SystemFields::Nxdn,
        SYSTEM_AES => SystemFields::Aes { network: block.first().copied().unwrap_or(1) },
        _ => SystemFields::Unknown,
    }
}

fn encode_fields(f: &SystemFields, prev: Option<&[u8]>) -> Option<Vec<u8>> {
    match f {
        SystemFields::P25Trunk(x) => Some(encode_p25_trunk(x, prev)),
        SystemFields::T3(x) => Some(encode_t3(x, prev)),
        SystemFields::Nxdn => Some(grow(prev, 0)),
        SystemFields::Aes { network } => {
            let mut v = grow(prev, 1);
            v[0] = *network;
            Some(v)
        }
        SystemFields::Unknown => None,
    }
}

/// The identity length of a type the editor knows (7.1).
fn id_len(type_id: u8) -> Option<u8> {
    match type_id {
        SYSTEM_P25_TRUNK => Some(P25_ID_LEN),
        SYSTEM_T3 => Some(T3_ID_LEN),
        SYSTEM_NXDN => Some(0),
        SYSTEM_AES => Some(1),
        _ => None,
    }
}

/// Whether a type is one this editor edits.
pub fn type_known(type_id: u8) -> bool {
    id_len(type_id).is_some()
}

/// The fields a new system of `type_id` starts with.
pub fn new_fields(type_id: u8) -> SystemFields {
    match type_id {
        SYSTEM_P25_TRUNK => SystemFields::P25Trunk(P25Trunk::default()),
        SYSTEM_T3 => SystemFields::T3(T3System::default()),
        SYSTEM_NXDN => SystemFields::Nxdn,
        SYSTEM_AES => SystemFields::Aes { network: 1 },
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
    /// Tier III channel (class 0): the LCN, the frequencies of the mobile station's receive and transmit sides, and its
    /// color code (`None` = the system's). A control channel is flagged on the entry.
    T3Channel { lcn: u16, rx_hz: u32, tx_hz: u32, cc: Option<u8> },
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
    } else if type_id == SYSTEM_T3 && cls == 0 {
        EntryBody::T3Channel {
            lcn: u16_at(body, 0),
            rx_hz: u32_at(body, 2),
            tx_hz: u32_at(body, 6),
            cc: (body.len() >= 11 && u8_at(body, 10) != 0xFF).then(|| u8_at(body, 10) & 0x0F),
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
        EntryBody::T3Channel { lcn, rx_hz, tx_hz, cc } => {
            body = grow(Some(&body), 11);
            body[0..2].copy_from_slice(&lcn.to_le_bytes());
            body[2..6].copy_from_slice(&rx_hz.to_le_bytes());
            body[6..10].copy_from_slice(&tx_hz.to_le_bytes());
            body[10] = cc.map_or(0xFF, |c| c & 0x0F);
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
    /// The scrambler keys of an NXDN system (the keys of other types are kept as stored, unread).
    pub traffic_keys: Vec<TrafficKey>,
    /// The AES-256 keys of a DMR AES system (the keys of other types are kept as stored, unread).
    pub aes_keys: Vec<AesKey>,
    /// The system's own secret, as stored (never shown): `None` for none.
    pub secret: Option<Vec<u8>>,
    /// The record as read; empty for a system made here.
    pub raw: Vec<u8>,
}

impl System {
    /// A new system of `type_id`.
    pub fn new(type_id: u8, name: &str) -> Self {
        Self {
            number: 0,
            type_id,
            name: name.to_string(),
            sharing: SHARE_IDENTITY,
            own_id: 0,
            // As the web editor makes them: a Tier III system is transmit-capable (a site grants every
            // transmission), a P25 trunked one listen only until the user turns the transmit side on.
            tx: matches!(type_id, SYSTEM_T3 | SYSTEM_NXDN | SYSTEM_AES),
            needs_key: false,
            flags_raw: 0,
            fallback: Fallback::None,
            fields: new_fields(type_id),
            entries: Vec::new(),
            traffic_keys: Vec::new(),
            aes_keys: Vec::new(),
            secret: None,
            raw: Vec::new(),
        }
    }

    /// Two systems with the same identity are the same system (7.1); `None` for a type this editor doesn't know.
    pub fn identity(&self) -> Option<String> {
        let len = usize::from(id_len(self.type_id)?);
        if len == 0 {
            return None; // no identity bytes: the system can't be matched by them (7.1)
        }
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
            traffic_keys: Vec::new(),
            aes_keys: Vec::new(),
            secret: kv::get_value(entries, SECRET_KEY + n as u16).map(<[u8]>::to_vec),
            raw: raw.to_vec(),
        });
    }
    for e in entries.iter().filter(|e| (TRAFFIC_KEY..TRAFFIC_KEY + MAX_TRAFFIC as u16).contains(&e.key)) {
        let Some(&sys) = e.value.first() else { continue };
        if let Some(s) = systems.iter_mut().find(|s| s.number == u16::from(sys) + 1 && has_traffic(s.type_id)) {
            if s.type_id == SYSTEM_AES {
                if let Some(k) = decode_aes_key(&e.value) {
                    s.aes_keys.push(AesKey { slot: Some(e.key), ..k });
                }
            } else if let Some(k) = decode_traffic(&e.value) {
                s.traffic_keys.push(TrafficKey { slot: Some(e.key), ..k });
            }
        }
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
    let stored = out.clone();
    let valid: Vec<&System> =
        systems.iter().filter(|s| (1..=MAX_SYSTEMS as u16).contains(&s.number)).collect();
    for n in 0..MAX_SYSTEMS {
        let key = SYSTEM_KEY + n as u16;
        let prev = kv::get_value(&out, key).map(<[u8]>::to_vec);
        let Some(s) = valid.iter().find(|s| s.number == n as u16 + 1) else {
            // A system the model no longer has was deleted: its secret with it.
            if prev.as_deref().and_then(decode_header).is_some() {
                let was_t3 = prev.as_deref().and_then(decode_header).is_some_and(|h| h.type_id == SYSTEM_T3);
                crate::modes::del(&mut out, key);
                crate::modes::del(&mut out, SECRET_KEY + n as u16);
                // Tier III's own records (the short data poll and the supplementary data) go with the system.
                if was_t3 {
                    crate::modes::del(&mut out, T3_POLL_KEY + n as u16);
                    crate::modes::del(&mut out, T3_SUPL_KEY + n as u16);
                }
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
    // Traffic keys: a key read keeps its slot; a new one takes the first free one. A stored record of a type this
    // editor reads goes when its key is gone from the model or its system is.
    let mut pending: Vec<Vec<u8>> = Vec::new();
    for s in valid.iter().filter(|s| has_traffic(s.type_id)) {
        let keys: Vec<(Option<u16>, Vec<u8>)> = if s.type_id == SYSTEM_AES {
            s.aes_keys.iter().map(|k| (k.slot, encode_aes_key((s.number - 1) as u8, k))).collect()
        } else {
            s.traffic_keys.iter().map(|k| (k.slot, encode_traffic((s.number - 1) as u8, k))).collect()
        };
        for (slot, v) in keys {
            match slot {
                Some(slot) => crate::modes::set(&mut out, slot, &v),
                None => pending.push(v),
            }
        }
    }
    for e in stored.iter().filter(|e| (TRAFFIC_KEY..TRAFFIC_KEY + MAX_TRAFFIC as u16).contains(&e.key)) {
        let Some(&sys) = e.value.first() else { continue };
        let owner = kv::get_value(&stored, SYSTEM_KEY + u16::from(sys)).and_then(decode_header);
        if !owner.as_ref().is_some_and(|h| has_traffic(h.type_id)) {
            continue;
        }
        let s = valid.iter().find(|s| s.number == u16::from(sys) + 1);
        let owner_type = owner.as_ref().map_or(0, |h| h.type_id);
        let kept = |s: &&System| {
            s.traffic_keys.iter().any(|k| k.slot == Some(e.key)) || s.aes_keys.iter().any(|k| k.slot == Some(e.key))
        };
        if s.is_none_or(|s| s.raw.is_empty()) || (reads_traffic(owner_type, &e.value) && !s.is_some_and(kept)) {
            crate::modes::del(&mut out, e.key);
        }
    }
    let occupied: std::collections::HashSet<u16> = out.iter().map(|e| e.key).collect();
    let mut slot = TRAFFIC_KEY;
    let mut overflow = false;
    for v in pending {
        while occupied.contains(&slot) {
            slot += 1;
        }
        if usize::from(slot - TRAFFIC_KEY) < MAX_TRAFFIC {
            crate::modes::set(&mut out, slot, &v);
            slot += 1;
        } else {
            overflow = true;
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
    if overflow {
        // More new keys than slots: the writer sees the sentinel and refuses (the area can't hold them).
        out.push(Entry { key: OVERFLOW_KEY, value: Vec::new() });
    }
    out
}

/// The recommended ham Tier III system's network and the control channel number of a site (ham-trunking.md 2): one
/// network for every ham site (Huge NET 1, category A, color code 1), and 4 x site + 1 as the control channel's LCN so
/// no two sites' channel numbers clash.
pub const HAM_T3_MODEL: u8 = 3;
pub const HAM_T3_NET: u16 = 1;
pub const HAM_T3_SITE_MAX: u16 = 1022;

pub fn ham_t3_lcn(site: u16) -> u16 {
    4 * site + 1
}

/// The recommended ham P25 trunked system's identity (ham-trunking.md 3): WACN 0x32350, any system ID, control channel
/// NAC 0x293, encrypted calls skipped.
pub const HAM_P25_WACN: u32 = 0x32350;
pub const HAM_P25_CC_NAC: u16 = 0x293;

/// A system of the type's recommended ham setup, with the control channel of the site the user uses: `rx_hz`, `tx_hz`
/// (0: as RX) and, for Tier III, the site's number. `None` for a type without a recommended setup.
pub fn new_ham_system(type_id: u8, name: &str, rx_hz: u32, tx_hz: u32, site: u16) -> Option<System> {
    let tx_hz = if tx_hz == 0 { rx_hz } else { tx_hz };
    let (default_name, fields, body, group) = match type_id {
        SYSTEM_P25_TRUNK => (
            "Ham P25",
            SystemFields::P25Trunk(P25Trunk {
                wacn: Some(HAM_P25_WACN),
                sysid: None,
                cc_nac: Some(HAM_P25_CC_NAC),
                skip_enc: true,
                ..P25Trunk::default()
            }),
            EntryBody::P25Control { iden: 0, chan_no: 0, rx_hz, tx_hz },
            0u8,
        ),
        SYSTEM_T3 => (
            "Ham Tier III",
            SystemFields::T3(T3System {
                model: HAM_T3_MODEL,
                net: HAM_T3_NET,
                category_b: false,
                color_code: 1,
                register_talkgroups: true,
                ..T3System::default()
            }),
            EntryBody::T3Channel { lcn: ham_t3_lcn(site), rx_hz, tx_hz, cc: None },
            0xFF,
        ),
        _ => return None,
    };
    let mut s = System::new(type_id, if name.is_empty() { default_name } else { name });
    s.tx = true;
    s.needs_key = false;
    s.fields = fields;
    s.entries = vec![SysEntry { cls: 0, group, control: true, flags: 0, body, raw: Vec::new() }];
    Some(s)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tier_iii_block_round_trips_and_keeps_the_option_bits_it_does_not_know() {
        let s = T3System {
            model: 2,
            net: 9,
            category_b: true,
            color_code: 7,
            all_calls: true,
            accept_kill: true,
            ip_gateway: true,
            fallback_hold: 5,
            plan_base_hz: 450_000_000,
            plan_first_lcn: 3,
            plan_sep: 6,
            plan_tx_low: true,
            plan_no_learn: true,
            plan_split_hz: 5_000_000,
            leave_dbm: -90,
            power_save: 3,
            sites: vec![1, 2, 15],
            ..T3System::default()
        };
        let v = encode_t3(&s, None);
        assert_eq!(v.len(), T3_BLOCK);
        assert_eq!(decode_t3(&v), s);
        // Option bits this editor doesn't know (0x8001 here) survive an edit of the ones it does.
        let mut prev = v.clone();
        prev[5] |= 0x01;
        prev[6] |= 0x80;
        let mut t = s.clone();
        t.all_calls = false;
        let w = encode_t3(&t, Some(&prev));
        assert_eq!(u16_at(&w, 5) & 0x8001, 0x8001);
        assert_eq!(u16_at(&w, 5) & OPT_ALL_CALLS, 0);
        assert!(!decode_t3(&w).all_calls);
    }

    #[test]
    fn the_network_model_limits_the_net_and_the_site_values() {
        assert_eq!([net_max(0), net_max(1), net_max(2), net_max(3)], [511, 127, 15, 3]);
        assert_eq!([site_max(0), site_max(1), site_max(2), site_max(3)], [7, 31, 255, 1023]);
    }

    #[test]
    fn a_power_save_value_out_of_range_reads_as_off_and_sites_stop_at_eight() {
        let mut v = vec![0u8; T3_BLOCK];
        v[21] = 9;
        v[22] = 20;
        assert_eq!(decode_t3(&v).power_save, 0);
        assert_eq!(decode_t3(&v).sites.len(), T3_SITES);
    }

    #[test]
    fn the_key_has_no_psn_in_16_octets_and_a_psn_in_19() {
        let key = parse_key_hex("00 11 22 33:44-55 66 77 88 99 AA BB CC DD EE FF").unwrap();
        assert_eq!(key_hex(&key), "00112233445566778899AABBCCDDEEFF");
        let none = encode_t3_key(&T3Key { key, psn: 0 }, None);
        assert_eq!(none.len(), 16);
        let with = encode_t3_key(&T3Key { key, psn: 0x010203 }, None);
        assert_eq!((with.len(), &with[16..]), (19, &[1u8, 2, 3][..]));
        assert_eq!(decode_t3_key(&with), Some(T3Key { key, psn: 0x010203 }));
        assert_eq!(decode_t3_key(&none).unwrap().psn, 0);
        // Octets after the PSN of a longer stored secret are kept when the PSN stays.
        let mut longer = with.clone();
        longer.extend_from_slice(&[9, 9]);
        assert_eq!(&encode_t3_key(&T3Key { key, psn: 0x040506 }, Some(&longer))[19..], &[9, 9]);
        // Shorter than 16 octets is no key; bad hex is none.
        assert_eq!(decode_t3_key(&[0; 15]), None);
        assert_eq!(parse_key_hex("00112233"), None);
        assert_eq!(parse_key_hex(&"G".repeat(32)), None);
    }

    #[test]
    fn a_key_never_prints() {
        let k = T3Key { key: [0xAB; 16], psn: 7 };
        let text = format!("{k:?}");
        assert!(!text.to_uppercase().contains("AB"), "{text}");
    }

    #[test]
    fn a_tier_iii_channel_entry_round_trips_and_a_missing_color_code_is_the_systems() {
        let e = |cc| SysEntry {
            cls: 0,
            group: 0xFF,
            control: true,
            flags: 0,
            body: EntryBody::T3Channel { lcn: 12, rx_hz: 438_100_000, tx_hz: 443_100_000, cc },
            raw: Vec::new(),
        };
        for cc in [None, Some(5)] {
            let rec = encode_entry(SYSTEM_T3, 0, &e(cc));
            let (_, cls, group, flags, body) = decode_entry_record(&rec).unwrap();
            assert_eq!(decode_entry(SYSTEM_T3, cls, group, flags, body, &rec).fields(), e(cc));
        }
        // A 10-octet body (no color code octet) reads as the system's.
        let mut rec = encode_entry(SYSTEM_T3, 0, &e(None));
        rec.truncate(ENTRY_HDR + 10);
        let (_, cls, group, flags, body) = decode_entry_record(&rec).unwrap();
        assert!(matches!(decode_entry(SYSTEM_T3, cls, group, flags, body, &rec).body, EntryBody::T3Channel { cc: None, .. }));
    }

    #[test]
    fn a_new_tier_iii_system_transmits_and_a_new_p25_trunked_one_listens() {
        assert!(System::new(SYSTEM_T3, "T").tx);
        assert!(!System::new(SYSTEM_P25_TRUNK, "P").tx);
    }
}
