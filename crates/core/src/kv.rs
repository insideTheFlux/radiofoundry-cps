// SPDX-License-Identifier: MIT
// Copyright (c) 2026 The RadioFoundry Authors
// Copyright (c) 2026 insideTheFlux
//
// The codeplug settings area (RadioFoundry `docs/spec/codeplug-settings.md`): keyed values in one 4 KB sector of the
// EEPROM image, for settings the open-firmware layout has no field for. Ported from
// `site/static/js/codeplug/kv.js`. A writer keeps the keys it doesn't know byte for byte and never writes over an
// area of a version it doesn't know.

use crate::image::EE;

pub const KV_ADDR: usize = EE + 0x0E000;
pub const KV_SIZE: usize = 0x1000;
pub const KV_VERSION: u8 = 1;
/// The header length this version writes.
pub const KV_HDR: usize = 16;
const MAGIC: [u8; 4] = *b"RFKV";

/// The highest DMR ID a key may hold; above it the key reads as absent.
pub const ID_MAX: u32 = 16_776_415;

/// What a sector holds (codeplug-settings.md 2.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AreaState {
    /// A valid area: its keys can be read and written.
    Valid,
    /// An erased sector: no key, writable as a new area.
    Empty,
    /// Anything else, version 0 included: no key, writable as a new area (the old bytes are dropped).
    Bad,
    /// Our magic and a later version: no key, never written.
    Newer,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub key: u16,
    pub value: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Area {
    pub state: AreaState,
    /// In the area's order; only a valid area has any.
    pub entries: Vec<Entry>,
}

/// CRC-32 as the area uses it (IEEE 802.3, reflected).
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    !crc
}

fn u16le(b: &[u8], o: usize) -> u16 {
    u16::from(b[o]) | (u16::from(b[o + 1]) << 8)
}

fn u32le(b: &[u8], o: usize) -> u32 {
    u32::from(b[o]) | (u32::from(b[o + 1]) << 8) | (u32::from(b[o + 2]) << 16) | (u32::from(b[o + 3]) << 24)
}

/// The sector inside an image's bytes, or `None` when the image is too short to hold it.
pub fn sector(bytes: &[u8]) -> Option<&[u8]> {
    bytes.get(KV_ADDR..KV_ADDR + KV_SIZE)
}

/// The area in a 4 KB sector.
pub fn decode_sector(s: &[u8]) -> Area {
    let none = |state| Area { state, entries: Vec::new() };
    if s.len() < KV_SIZE {
        return none(AreaState::Empty);
    }
    if s[..4] != MAGIC {
        return none(if s.iter().all(|&x| x == 0xFF) { AreaState::Empty } else { AreaState::Bad });
    }
    // No version 0 was ever written: a damaged byte, not a later version's area that must be left alone.
    if s[4] != KV_VERSION {
        return none(if s[4] > KV_VERSION { AreaState::Newer } else { AreaState::Bad });
    }
    let (hdr, len) = (usize::from(s[5]), usize::from(u16le(s, 6)));
    if hdr < KV_HDR || hdr + len > KV_SIZE {
        return none(AreaState::Bad);
    }
    let data = &s[hdr..hdr + len];
    if crc32(data) != u32le(s, 8) {
        return none(AreaState::Bad);
    }
    let mut entries = Vec::new();
    let mut o = 0;
    while o < len {
        if o + 4 > len {
            return none(AreaState::Bad);
        }
        let (key, n) = (u16le(data, o), usize::from(u16le(data, o + 2)));
        if o + 4 + n > len {
            return none(AreaState::Bad);
        }
        entries.push(Entry { key, value: data[o + 4..o + 4 + n].to_vec() });
        o += 4 + n;
    }
    Area { state: AreaState::Valid, entries }
}

/// The area in an image's bytes.
pub fn decode_area(bytes: &[u8]) -> Area {
    sector(bytes).map_or(Area { state: AreaState::Empty, entries: Vec::new() }, decode_sector)
}

/// A key's value: the first entry's ("one entry per key"), or `None` when absent.
pub fn get_value(entries: &[Entry], key: u16) -> Option<&[u8]> {
    entries.iter().find(|e| e.key == key).map(|e| e.value.as_slice())
}

/// The entries with `key` set to `value`: replaced in place, appended when new, later entries of the key dropped.
pub fn set_value(entries: &[Entry], key: u16, value: &[u8]) -> Vec<Entry> {
    let mut out = Vec::new();
    let mut done = false;
    for e in entries {
        if e.key != key {
            out.push(e.clone());
        } else if !done {
            out.push(Entry { key, value: value.to_vec() });
            done = true;
        }
    }
    if !done {
        out.push(Entry { key, value: value.to_vec() });
    }
    out
}

/// The entries without `key` (every entry of it).
pub fn delete_value(entries: &[Entry], key: u16) -> Vec<Entry> {
    entries.iter().filter(|e| e.key != key).cloned().collect()
}

/// The entries as a writer keeps them: the first of each key, in order.
pub fn dedupe(entries: &[Entry]) -> Vec<Entry> {
    let mut seen = std::collections::HashSet::new();
    entries.iter().filter(|e| seen.insert(e.key)).cloned().collect()
}

/// The sector for `entries` (duplicates dropped): the header, the entries, 0xFF to the end. `Err` when they don't
/// fit. `prev`, a valid area's sector, gives its header length and the header bytes past the CRC, kept as found (a
/// later version's fields and the continuation); a new area has them 0.
pub fn encode_area(entries: &[Entry], prev: Option<&[u8]>) -> Result<Vec<u8>, &'static str> {
    let entries = dedupe(entries);
    let hdr = prev.map_or(KV_HDR, |p| usize::from(p[5]));
    let len: usize = entries.iter().map(|e| 4 + e.value.len()).sum();
    if hdr + len > KV_SIZE {
        return Err("codeplug settings area full");
    }
    let mut s = vec![0xFFu8; KV_SIZE];
    let mut o = hdr;
    for e in &entries {
        s[o..o + 2].copy_from_slice(&e.key.to_le_bytes());
        s[o + 2..o + 4].copy_from_slice(&(e.value.len() as u16).to_le_bytes());
        s[o + 4..o + 4 + e.value.len()].copy_from_slice(&e.value);
        o += 4 + e.value.len();
    }
    s[..4].copy_from_slice(&MAGIC);
    s[4] = KV_VERSION;
    s[5] = hdr as u8;
    s[6..8].copy_from_slice(&(len as u16).to_le_bytes());
    let crc = crc32(&s[hdr..hdr + len]);
    s[8..12].copy_from_slice(&crc.to_le_bytes());
    match prev {
        Some(p) => s[12..hdr].copy_from_slice(&p[12..hdr]),
        None => s[12..hdr].fill(0),
    }
    Ok(s)
}

// --- The keys the editor knows (codeplug-settings.md 3) ---

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Id,
    Bool,
}

/// One scalar key: its number and type. A key's meaning never changes.
struct Row {
    key: u16,
    kind: Kind,
}

const ARS_SERVER: Row = Row { key: 0x0001, kind: Kind::Id };
const IP_GATEWAY: Row = Row { key: 0x0002, kind: Kind::Id };
const CALL_END_TONE: Row = Row { key: 0x0003, kind: Kind::Bool };
const TOT_WARN_TONE: Row = Row { key: 0x0004, kind: Kind::Bool };
const BATTERY_TONE: Row = Row { key: 0x0005, kind: Kind::Bool };
const TX_DENY_TONE: Row = Row { key: 0x0006, kind: Kind::Bool };

/// The signaling tone chain: ten steps, laid out as the spec's 3.3 gives (the first five as the first version's
/// 17-byte record had them, the other five after).
pub const SIG_STEPS: usize = 10;
const SIG_BASE: usize = 5;
pub const SIG_HZ_MAX: u16 = 3400;
pub const SIG_HZ_UNIT: u16 = 10;
/// A step's length in ms, and the chain's total.
pub const SIG_MS_MAX: u16 = 900;
pub const SIG_REC_LEN: usize = 2 + 3 * SIG_STEPS;

fn rec_hz(i: usize) -> usize {
    if i < SIG_BASE {
        2 + 2 * i
    } else {
        2 + 3 * SIG_BASE + 2 * (i - SIG_BASE)
    }
}

fn rec_ms(i: usize) -> usize {
    if i < SIG_BASE {
        2 + 2 * SIG_BASE + i
    } else {
        2 + 3 * SIG_BASE + 2 * (SIG_STEPS - SIG_BASE) + (i - SIG_BASE)
    }
}

/// One signaling tone event: where it sounds in DMR and in FM (0 the radio's own sound, 1 off, 2 speaker, 3 on air,
/// 4 both), then the chain. `ms` is in milliseconds, a multiple of 10; a step of length 0 ends the chain.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SigRecord {
    pub dmr: u8,
    pub fm: u8,
    pub hz: [u16; SIG_STEPS],
    pub ms: [u16; SIG_STEPS],
}

impl Default for SigRecord {
    fn default() -> Self {
        Self { dmr: 0, fm: 0, hz: [0; SIG_STEPS], ms: [0; SIG_STEPS] }
    }
}

/// A signaling tone event's row: the key, and whether the event has an on-air copy, in DMR and in FM.
struct RecRow {
    key: u16,
    air: bool,
    fm_air: bool,
}

const SIG_BEGIN: RecRow = RecRow { key: 0x0007, air: true, fm_air: true };
const SIG_END: RecRow = RecRow { key: 0x0008, air: true, fm_air: true };
// The end of a reception has no on-air output.
const SIG_IDLE: RecRow = RecRow { key: 0x0009, air: false, fm_air: false };

const SIG_ROWS: [&RecRow; 3] = [&SIG_BEGIN, &SIG_END, &SIG_IDLE];

/// Whether `rec` is what an event takes: the outputs it has, a frequency up to 3400 Hz in steps of 10, each length
/// a multiple of 10 ms up to 900.
pub fn sig_valid(event: usize, rec: &SigRecord) -> bool {
    let Some(row) = SIG_ROWS.get(event) else { return false };
    let out = |v: u8, air: bool| v <= 4 && (air || (v != 3 && v != 4));
    out(rec.dmr, row.air)
        && out(rec.fm, row.fm_air)
        && rec.hz.iter().all(|&x| x <= SIG_HZ_MAX && x % SIG_HZ_UNIT == 0)
        && rec.ms.iter().all(|&x| x <= SIG_MS_MAX && x % 10 == 0)
}

/// The keyed settings of the area: `None` is an absent key (the codeplug says nothing; the radio keeps its own).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Keys {
    pub ars_server: Option<u32>,
    pub ip_gateway: Option<u32>,
    pub call_end_tone: Option<bool>,
    pub tot_warn_tone: Option<bool>,
    pub battery_tone: Option<bool>,
    pub tx_deny_tone: Option<bool>,
    /// The start, end and idle signaling tones.
    pub sig: [Option<SigRecord>; 3],
}

/// A scalar key as a number (a bool is 0 or 1); `None` when absent or holding what the setting can't take (empty,
/// longer than a scalar, past 32 bits, outside the range), which reads as absent.
fn get_scalar(entries: &[Entry], row: &Row) -> Option<u32> {
    let v = get_value(entries, row.key)?;
    if v.is_empty() || v.len() > 8 {
        return None;
    }
    if row.kind == Kind::Bool {
        return Some(u32::from(v.iter().any(|&x| x != 0)));
    }
    let mut n: u64 = 0;
    for (i, &x) in v.iter().enumerate() {
        if i < 4 {
            n |= u64::from(x) << (8 * i);
        } else if x != 0 {
            return None;
        }
    }
    (n <= u64::from(ID_MAX)).then_some(n as u32)
}

/// The value a writer stores for a scalar: a whole value, little endian.
fn scalar_value(row: &Row, v: u32) -> Vec<u8> {
    match row.kind {
        Kind::Bool => vec![u8::from(v != 0)],
        Kind::Id => v.to_le_bytes().to_vec(),
    }
}

/// A signaling tone record: `None` when the key is absent or holds what the event can't take. A shorter value reads
/// with the missing fields zero, a longer one (a later version's) from the front.
fn get_record(entries: &[Entry], event: usize) -> Option<SigRecord> {
    let v = get_value(entries, SIG_ROWS[event].key)?;
    if v.is_empty() {
        return None;
    }
    let mut b = [0u8; SIG_REC_LEN];
    let n = v.len().min(SIG_REC_LEN);
    b[..n].copy_from_slice(&v[..n]);
    let mut rec = SigRecord { dmr: b[0], fm: b[1], ..SigRecord::default() };
    for i in 0..SIG_STEPS {
        rec.hz[i] = u16le(&b, rec_hz(i));
        rec.ms[i] = u16::from(b[rec_ms(i)]) * 10;
    }
    sig_valid(event, &rec).then_some(rec)
}

/// The value a writer stores for a record: its 32 bytes. A stored value longer than that keeps the bytes after them.
fn record_value(rec: &SigRecord, old: Option<&[u8]>) -> Vec<u8> {
    let mut b = vec![0u8; SIG_REC_LEN.max(old.map_or(0, <[u8]>::len))];
    if let Some(o) = old {
        b[..o.len()].copy_from_slice(o);
    }
    b[0] = rec.dmr;
    b[1] = rec.fm;
    for i in 0..SIG_STEPS {
        b[rec_hz(i)..rec_hz(i) + 2].copy_from_slice(&rec.hz[i].to_le_bytes());
        b[rec_ms(i)] = (rec.ms[i] / 10) as u8;
    }
    b
}

/// The keys an area holds (all absent unless it is valid).
pub fn keys_of(area: &Area) -> Keys {
    let e = &area.entries;
    let flag = |r: &Row| get_scalar(e, r).map(|v| v != 0);
    Keys {
        ars_server: get_scalar(e, &ARS_SERVER),
        ip_gateway: get_scalar(e, &IP_GATEWAY),
        call_end_tone: flag(&CALL_END_TONE),
        tot_warn_tone: flag(&TOT_WARN_TONE),
        battery_tone: flag(&BATTERY_TONE),
        tx_deny_tone: flag(&TX_DENY_TONE),
        sig: [0, 1, 2].map(|i| get_record(e, i)),
    }
}

/// The keys in an image's bytes.
pub fn decode_keys(bytes: &[u8]) -> Keys {
    keys_of(&decode_area(bytes))
}

/// Whether `keys` hold values their keys take: IDs in range, tone records valid.
pub fn keys_valid(keys: &Keys) -> bool {
    let id = |v: Option<u32>| v.is_none_or(|v| v <= ID_MAX);
    id(keys.ars_server)
        && id(keys.ip_gateway)
        && keys.sig.iter().enumerate().all(|(i, r)| r.as_ref().is_none_or(|r| sig_valid(i, r)))
}

/// Writes `entries` into an image's bytes as the area, only when the sector's bytes change. Refuses (false, nothing
/// written) over an area of a newer version, and when the entries don't fit. True when the sector holds them.
fn put_area(bytes: &mut [u8], entries: &[Entry]) -> bool {
    let Some(have) = sector(bytes) else { return false };
    let state = decode_sector(have).state;
    if state == AreaState::Newer {
        return false;
    }
    let Ok(s) = encode_area(entries, (state == AreaState::Valid).then_some(have)) else { return false };
    bytes[KV_ADDR..KV_ADDR + KV_SIZE].copy_from_slice(&s);
    true
}

/// The key families that hold secrets (channel-modes.md 8): system secrets (`0x0300`-`0x03FF`) and traffic keys
/// (`0x1900`-`0x1CFF`).
const SECRET_FAMILIES: [(u16, u16); 2] = [(0x0300, 0x100), (0x1900, 0x400)];

/// Whether a key holds a secret.
pub fn is_secret(key: u16) -> bool {
    SECRET_FAMILIES.iter().any(|&(base, n)| (base..base + n).contains(&key))
}

/// How many secrets an image's settings area holds.
pub fn count_secrets(bytes: &[u8]) -> usize {
    let area = decode_area(bytes);
    if area.state != AreaState::Valid {
        return 0;
    }
    dedupe(&area.entries).iter().filter(|e| is_secret(e.key)).count()
}

/// An image's bytes with every secret taken out of its settings area (a file saved without keys). An area that is
/// not valid has none, and one of a newer version is left alone.
pub fn without_secrets(bytes: &mut [u8]) {
    let area = decode_area(bytes);
    if area.state != AreaState::Valid {
        return;
    }
    let entries = dedupe(&area.entries);
    let kept: Vec<Entry> = entries.iter().filter(|e| !is_secret(e.key)).cloned().collect();
    if kept.len() != entries.len() {
        put_area(bytes, &kept);
    }
}

/// The entries of an image's area as a writer starts from them: the first of each key, in order; none when the area
/// isn't valid. `None` for an area of a newer version, which is never written.
pub fn entries_for_write(bytes: &[u8]) -> Option<Vec<Entry>> {
    let area = decode_area(bytes);
    (area.state != AreaState::Newer).then(|| dedupe(&area.entries))
}

/// Writes `entries` as the area when they differ from `before`. False (nothing written) when the area is of a newer
/// version or the entries don't fit.
pub fn put_entries(bytes: &mut [u8], before: &[Entry], entries: &[Entry]) -> bool {
    if before == entries {
        return true;
    }
    put_area(bytes, entries)
}

/// The keys an edit changes, written onto the area in an image's bytes. The area is left alone when every key
/// already reads as asked, and refused (false) when it is of a newer version or the keys don't fit.
pub fn put_keys(bytes: &mut [u8], keys: &Keys) -> bool {
    let area = decode_area(bytes);
    let have = keys_of(&area);
    if &have == keys {
        return true;
    }
    if area.state == AreaState::Newer {
        return false;
    }
    // An invalid area starts a new one: nothing in it could be read.
    let mut entries = dedupe(&area.entries);
    let scalars: [(&Row, Option<u32>, Option<u32>); 6] = [
        (&ARS_SERVER, have.ars_server, keys.ars_server),
        (&IP_GATEWAY, have.ip_gateway, keys.ip_gateway),
        (&CALL_END_TONE, have.call_end_tone.map(u32::from), keys.call_end_tone.map(u32::from)),
        (&TOT_WARN_TONE, have.tot_warn_tone.map(u32::from), keys.tot_warn_tone.map(u32::from)),
        (&BATTERY_TONE, have.battery_tone.map(u32::from), keys.battery_tone.map(u32::from)),
        (&TX_DENY_TONE, have.tx_deny_tone.map(u32::from), keys.tx_deny_tone.map(u32::from)),
    ];
    for (row, was, now) in scalars {
        if was == now {
            continue;
        }
        entries = match now {
            None => delete_value(&entries, row.key),
            Some(v) => set_value(&entries, row.key, &scalar_value(row, v)),
        };
    }
    for (row, (was, now)) in SIG_ROWS.iter().zip(have.sig.iter().zip(&keys.sig)) {
        if was == now {
            continue;
        }
        let key = row.key;
        entries = match now {
            None => delete_value(&entries, key),
            Some(r) => {
                let v = record_value(r, get_value(&entries, key));
                set_value(&entries, key, &v)
            }
        };
    }
    put_area(bytes, &entries)
}

/// The area a write to a radio sends, put into the image bytes `file`. `radio` is the radio's own sector and `base`
/// the sector of the image the editor opened, both 4 KB (codeplug-settings.md 6, 7).
/// - The radio's area is of a newer version: the radio's, as it is.
/// - The edit didn't change the area from `base`, or the file's isn't valid: the radio's. A file without an area
///   (another tool's, 0xFF there) never erases the keys.
/// - The edit changed it and the radio has a valid area: the radio's area with only the keys the edit changed, set or
///   removed. A key the radio's menu changed since the file was read keeps the radio's value.
/// - The radio has no valid area: the file's.
///
/// `Err` when the radio's keys and the edit's don't fit together.
pub fn area_for_write(file: &mut [u8], radio: &[u8], base: &[u8]) -> Result<(), &'static str> {
    if radio.len() != KV_SIZE || base.len() != KV_SIZE {
        return Err("the radio's and the base sector must each be 4 KB");
    }
    let Some(fs) = sector(file).map(<[u8]>::to_vec) else { return Err("the image is too short for the settings area") };
    let bs = base;
    let (f, r) = (decode_sector(&fs), decode_sector(radio));
    let edited = fs != bs;
    if r.state == AreaState::Newer || !edited || f.state != AreaState::Valid {
        file[KV_ADDR..KV_ADDR + KV_SIZE].copy_from_slice(radio);
        return Ok(());
    }
    if r.state != AreaState::Valid {
        return Ok(());
    }
    let was = decode_sector(bs).entries;
    let mut entries = dedupe(&r.entries);
    let mut keys: Vec<u16> = was.iter().chain(f.entries.iter()).map(|e| e.key).collect();
    keys.sort_unstable();
    keys.dedup();
    for key in keys {
        let now = get_value(&f.entries, key);
        if get_value(&was, key) == now {
            continue;
        }
        entries = match now {
            None => delete_value(&entries, key),
            Some(v) => set_value(&entries, key, v),
        };
    }
    let s = encode_area(&entries, Some(radio))?;
    file[KV_ADDR..KV_ADDR + KV_SIZE].copy_from_slice(&s);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blank() -> Vec<u8> {
        vec![0xFF; crate::image::CODEPLUG_SPAN]
    }

    fn with(entries: &[Entry]) -> Vec<u8> {
        let mut b = blank();
        let s = encode_area(entries, None).unwrap();
        b[KV_ADDR..KV_ADDR + KV_SIZE].copy_from_slice(&s);
        b
    }

    fn e(key: u16, v: &[u8]) -> Entry {
        Entry { key, value: v.to_vec() }
    }

    #[test]
    fn crc32_is_the_ieee_check_value() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn an_erased_sector_is_empty_and_has_no_keys() {
        let b = blank();
        assert_eq!(decode_area(&b).state, AreaState::Empty);
        assert_eq!(decode_keys(&b), Keys::default());
    }

    #[test]
    fn anything_else_without_our_magic_is_bad() {
        let mut b = blank();
        b[KV_ADDR] = 0x12;
        assert_eq!(decode_area(&b).state, AreaState::Bad);
    }

    #[test]
    fn a_later_version_is_newer_and_version_zero_is_bad() {
        let mut b = with(&[e(1, &[1, 0, 0, 0])]);
        b[KV_ADDR + 4] = 2;
        assert_eq!(decode_area(&b).state, AreaState::Newer);
        b[KV_ADDR + 4] = 0;
        assert_eq!(decode_area(&b).state, AreaState::Bad);
    }

    #[test]
    fn a_wrong_crc_is_bad() {
        let mut b = with(&[e(1, &[1, 0, 0, 0])]);
        b[KV_ADDR + 16] ^= 1;
        assert_eq!(decode_area(&b).state, AreaState::Bad);
    }

    #[test]
    fn keys_round_trip_and_the_rest_stay_absent() {
        let mut b = blank();
        let keys = Keys {
            ars_server: Some(3_100_001),
            call_end_tone: Some(false),
            battery_tone: Some(true),
            ..Keys::default()
        };
        assert!(put_keys(&mut b, &keys));
        assert_eq!(decode_area(&b).state, AreaState::Valid);
        assert_eq!(decode_keys(&b), keys);
        // The first scalar is a 4-byte id, the bool one byte.
        let a = decode_area(&b);
        assert_eq!(get_value(&a.entries, 1), Some(&3_100_001u32.to_le_bytes()[..]));
        assert_eq!(get_value(&a.entries, 3), Some(&[0u8][..]));
    }

    #[test]
    fn unknown_keys_are_kept_byte_for_byte_and_in_order() {
        let mut b = with(&[e(0x0100, &[9, 8, 7]), e(0x0001, &[1, 0, 0, 0]), e(0x0400, &[])]);
        let mut keys = decode_keys(&b);
        keys.ars_server = Some(2);
        keys.ip_gateway = Some(5);
        assert!(put_keys(&mut b, &keys));
        let a = decode_area(&b);
        let ks: Vec<u16> = a.entries.iter().map(|x| x.key).collect();
        assert_eq!(ks, vec![0x0100, 0x0001, 0x0400, 0x0002]);
        assert_eq!(get_value(&a.entries, 0x0100), Some(&[9u8, 8, 7][..]));
        assert_eq!(decode_keys(&b).ars_server, Some(2));
        assert_eq!(decode_keys(&b).ip_gateway, Some(5));
    }

    #[test]
    fn removing_a_key_closes_the_gap_and_an_unchanged_edit_writes_nothing() {
        let mut b = with(&[e(1, &[7, 0, 0, 0]), e(2, &[8, 0, 0, 0])]);
        let before = b.clone();
        let keys = decode_keys(&b);
        assert!(put_keys(&mut b, &keys));
        assert_eq!(b, before);
        let mut keys = keys;
        keys.ars_server = None;
        assert!(put_keys(&mut b, &keys));
        let a = decode_area(&b);
        assert_eq!(a.entries.len(), 1);
        assert_eq!(a.entries[0].key, 2);
    }

    #[test]
    fn a_newer_area_is_never_written() {
        let mut b = with(&[e(1, &[1, 0, 0, 0])]);
        b[KV_ADDR + 4] = 2;
        let before = b.clone();
        let keys = Keys { ars_server: Some(9), ..Keys::default() };
        assert!(!put_keys(&mut b, &keys));
        assert_eq!(b, before);
    }

    #[test]
    fn a_bad_area_starts_a_new_one() {
        let mut b = blank();
        b[KV_ADDR..KV_ADDR + 8].copy_from_slice(b"garbage!");
        let keys = Keys { tx_deny_tone: Some(true), ..Keys::default() };
        assert!(put_keys(&mut b, &keys));
        assert_eq!(decode_area(&b).state, AreaState::Valid);
        assert_eq!(decode_keys(&b), keys);
    }

    #[test]
    fn values_a_setting_cannot_take_read_as_absent() {
        let a = Area {
            state: AreaState::Valid,
            entries: vec![
                e(1, &[]),                                    // empty
                e(2, &[0xFF, 0xFF, 0xFF, 0xFF]),              // past the ID range
                e(3, &[0, 0, 0, 0, 0, 0, 0, 0, 1]),           // longer than a scalar
                e(4, &[5]),                                   // a bool: anything nonzero is on
                e(5, &[1, 0, 0, 0, 0, 1]),                    // a bool, longer: still on
            ],
        };
        let k = keys_of(&a);
        assert_eq!((k.ars_server, k.ip_gateway, k.call_end_tone), (None, None, None));
        assert_eq!(k.tot_warn_tone, Some(true));
        assert_eq!(k.battery_tone, Some(true));
        // A shorter id zero-extends.
        let a = Area { state: AreaState::Valid, entries: vec![e(1, &[0x10, 0x27])] };
        assert_eq!(keys_of(&a).ars_server, Some(10_000));
    }

    fn tone() -> SigRecord {
        let mut r = SigRecord { dmr: 2, fm: 1, ..SigRecord::default() };
        r.hz[0] = 1000;
        r.ms[0] = 200;
        r.hz[6] = 3400;
        r.ms[6] = 900;
        r
    }

    #[test]
    fn a_signaling_tone_record_round_trips_in_the_spec_layout() {
        let mut b = blank();
        let keys = Keys { sig: [Some(tone()), None, None], ..Keys::default() };
        assert!(put_keys(&mut b, &keys));
        assert_eq!(decode_keys(&b), keys);
        let v = get_value(&decode_area(&b).entries, 7).unwrap().to_vec();
        assert_eq!(v.len(), SIG_REC_LEN);
        assert_eq!(&v[..2], &[2, 1]);
        assert_eq!(&v[2..4], &1000u16.to_le_bytes()); // step 1 frequency
        assert_eq!(v[12], 20); // step 1 length, in 10 ms
        assert_eq!(&v[19..21], &3400u16.to_le_bytes()); // step 7 frequency (the second group)
        assert_eq!(v[28], 90); // step 7 length
    }

    #[test]
    fn a_first_version_record_reads_with_the_rest_zero_and_a_longer_one_keeps_its_tail() {
        let mut v = vec![0u8; 17];
        v[0] = 1;
        v[2..4].copy_from_slice(&500u16.to_le_bytes());
        v[12] = 10;
        let a = Area { state: AreaState::Valid, entries: vec![e(7, &v)] };
        let r = keys_of(&a).sig[0].clone().unwrap();
        assert_eq!((r.dmr, r.hz[0], r.ms[0], r.hz[9]), (1, 500, 100, 0));
        // A later version's longer record keeps the bytes after the 32nd when this version sets its fields.
        let mut long = vec![0u8; 36];
        long[32..].copy_from_slice(&[1, 2, 3, 4]);
        let mut b = with(&[e(7, &long)]);
        let mut k = decode_keys(&b);
        k.sig[0].as_mut().unwrap().ms[0] = 300;
        assert!(put_keys(&mut b, &k));
        let out = get_value(&decode_area(&b).entries, 7).unwrap().to_vec();
        assert_eq!(out.len(), 36);
        assert_eq!(&out[32..], &[1, 2, 3, 4]);
    }

    #[test]
    fn a_record_an_event_cannot_take_reads_as_absent() {
        let mut idle = tone();
        idle.dmr = 3; // the end of a reception has no on-air output
        let mut b = blank();
        // Write the bytes directly: put_keys would take any record, and validation is the model's job.
        let v = record_value(&idle, None);
        let s = encode_area(&[e(9, &v)], None).unwrap();
        b[KV_ADDR..KV_ADDR + KV_SIZE].copy_from_slice(&s);
        assert_eq!(decode_keys(&b).sig[2], None);
        assert!(!sig_valid(2, &idle));
        assert!(sig_valid(0, &idle));
        let mut odd = tone();
        odd.hz[1] = 1005;
        assert!(!sig_valid(0, &odd));
        odd = tone();
        odd.ms[0] = 910;
        assert!(!sig_valid(0, &odd));
    }

    #[test]
    fn an_area_that_does_not_fit_is_refused_and_left_alone() {
        // 16 + 4 + 4070 = 4090 bytes: a 4-byte id key (8 bytes with its header) no longer fits.
        let mut b = with(&[e(0x0100, &vec![0u8; 4070])]);
        let before = b.clone();
        let keys = Keys { ars_server: Some(1), ..Keys::default() };
        assert!(!put_keys(&mut b, &keys));
        assert_eq!(b, before);
        let huge = [e(0x0100, &[0u8; 4000]), e(0x0101, &[0u8; 200])];
        assert!(encode_area(&huge, None).is_err());
    }

    // --- area_for_write (codeplug-settings.md 6, 7) ---

    fn sec(b: &[u8]) -> Vec<u8> {
        b[KV_ADDR..KV_ADDR + KV_SIZE].to_vec()
    }

    #[test]
    fn a_file_without_an_area_never_erases_the_radios_keys() {
        let radio = with(&[e(1, &[5, 0, 0, 0])]);
        let base = blank();
        let mut file = blank();
        area_for_write(&mut file, &radio[KV_ADDR..KV_ADDR + KV_SIZE], &sec(&base)).unwrap();
        assert_eq!(sec(&file), sec(&radio));
    }

    #[test]
    fn an_unedited_area_sends_the_radios_own() {
        let base = with(&[e(1, &[5, 0, 0, 0])]);
        let radio = with(&[e(1, &[6, 0, 0, 0])]); // the radio's menu changed it since the file was read
        let mut file = base.clone();
        area_for_write(&mut file, &radio[KV_ADDR..KV_ADDR + KV_SIZE], &sec(&base)).unwrap();
        assert_eq!(sec(&file), sec(&radio));
    }

    #[test]
    fn an_edited_area_sets_only_the_changed_keys_on_the_radios_area() {
        let base = with(&[e(1, &[5, 0, 0, 0]), e(2, &[7, 0, 0, 0])]);
        // The radio's menu changed key 2 and added an unknown key since the read.
        let radio = with(&[e(1, &[5, 0, 0, 0]), e(2, &[9, 0, 0, 0]), e(0x0100, &[1])]);
        let mut file = base.clone();
        let mut keys = decode_keys(&file);
        keys.ars_server = Some(42); // the edit
        assert!(put_keys(&mut file, &keys));
        area_for_write(&mut file, &radio[KV_ADDR..KV_ADDR + KV_SIZE], &sec(&base)).unwrap();
        let k = decode_keys(&file);
        assert_eq!(k.ars_server, Some(42)); // the edit
        assert_eq!(k.ip_gateway, Some(9)); // the radio's value kept
        assert!(get_value(&decode_area(&file).entries, 0x0100).is_some()); // the radio's unknown key kept
    }

    #[test]
    fn a_newer_radio_area_is_left_as_it_is() {
        let base = with(&[e(1, &[5, 0, 0, 0])]);
        let mut radio = with(&[e(1, &[6, 0, 0, 0])]);
        radio[KV_ADDR + 4] = 2;
        let mut file = base.clone();
        let mut keys = decode_keys(&file);
        keys.ars_server = Some(42);
        assert!(put_keys(&mut file, &keys));
        area_for_write(&mut file, &radio[KV_ADDR..KV_ADDR + KV_SIZE], &sec(&base)).unwrap();
        assert_eq!(sec(&file), sec(&radio));
    }

    #[test]
    fn a_radio_with_no_valid_area_takes_the_files() {
        let base = blank();
        let radio = blank();
        let mut file = base.clone();
        let keys = Keys { ars_server: Some(42), ..Keys::default() };
        assert!(put_keys(&mut file, &keys));
        let want = sec(&file);
        area_for_write(&mut file, &radio[KV_ADDR..KV_ADDR + KV_SIZE], &sec(&base)).unwrap();
        assert_eq!(sec(&file), want);
    }

    #[test]
    fn secrets_are_counted_and_taken_out_and_the_rest_stays() {
        let mut b = with(&[e(0x0100, &[1]), e(0x0300, &[9; 19]), e(0x0001, &[5, 0, 0, 0]), e(0x1900, &[7; 6]), e(0x0400, &[2])]);
        assert_eq!(count_secrets(&b), 2);
        assert!(is_secret(0x03FF) && is_secret(0x1CFF) && !is_secret(0x0100) && !is_secret(0x1D00));
        without_secrets(&mut b);
        assert_eq!(count_secrets(&b), 0);
        let keys: Vec<u16> = decode_area(&b).entries.iter().map(|x| x.key).collect();
        assert_eq!(keys, vec![0x0100, 0x0001, 0x0400]);
        // No secrets: nothing changes. Not a valid area: nothing changes.
        let again = b.clone();
        without_secrets(&mut b);
        assert_eq!(b, again);
        let mut erased = blank();
        without_secrets(&mut erased);
        assert_eq!(erased, blank());
    }
}
