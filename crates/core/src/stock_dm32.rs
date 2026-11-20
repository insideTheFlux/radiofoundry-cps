// SPDX-License-Identifier: MIT
// Copyright (c) 2026 The RadioFoundry Authors
// Copyright (c) 2026 insideTheFlux
//
// The stock Baofeng DM-32UV codeplug (RadioFoundry `docs/spec/dm32-stock-codeplug.md`): 4 KB pages in the data flash,
// each tagged with the logical block it holds. Builds the logical image from a data flash read, reads its records,
// and fills a codeplug from them. Ported from `site/static/js/codeplug/stock_baofeng.js`, with the DCS tone word read
// as BCD digits (checked against a stock radio's own settings).

use crate::fields::{bcd, get_freq, u16le};
use crate::model::Codeplug;
use crate::records::{
    CallType, Channel, Contact, Mode, RxList, Zone, MAX_CHANNELS, MAX_CONTACTS, MAX_RX_LISTS,
    MAX_ZONES,
};
use std::collections::{HashMap, HashSet};

const PAGES_START: usize = 0x1000;
const PAGES_END: usize = 0xC9000;
const PAGE: usize = 0x1000;

const BLK_SETTINGS: u8 = 0x04;
const BLK_LISTS: u8 = 0x0F;
const BLK_CHANNELS: u8 = 0x12;
const BLK_TX_CONTACTS: u8 = 0x42;
const BLK_CONTACTS: u8 = 0x44;
const BLK_ZONES: u8 = 0x5C;
const BLK_IDS: u8 = 0x67;
const CH: usize = 0x30;
const CONTACT: usize = 0x18;
const LIST: usize = 0x6D;
const ZONE: usize = 0x91;
const RADIO_ID: usize = 0x10;
const N_LISTS: usize = 32;
const LIST_MEMBERS: usize = 32;
const ZONE_CHANNELS: usize = 64;
const N_IDS: usize = 255;
const BLOCK_DATA: usize = PAGE - 1;
/// Channels past the first block's are in a layout not mapped yet: only the first block's are read.
const CH_PER_BLOCK: usize = (BLOCK_DATA - 0x10) / CH;
/// Power by the record's 2-bit field: low, mid, high.
const POWER: [u8; 3] = [3, 6, 9];

/// The blocks in use, by block number. `None` when two pages claim one block (not this layout) or none is in use.
pub fn page_map(bytes: &[u8]) -> Option<HashMap<u8, &[u8]>> {
    let mut out = HashMap::new();
    let mut a = PAGES_START;
    while a + PAGE <= bytes.len().min(PAGES_END) {
        let id = bytes[a + PAGE - 1];
        if id != 0x00 && id != 0xFF && out.insert(id, &bytes[a..a + PAGE]).is_some() {
            return None;
        }
        a += PAGE;
    }
    (!out.is_empty()).then_some(out)
}

/// A NUL-ended ASCII name of up to `n` bytes, and whether every character before the end is printable. What follows the
/// end is not checked: the stock software leaves old text there.
fn ascii_name(b: &[u8], off: usize, n: usize) -> (String, bool) {
    let mut text = String::new();
    for i in 0..n {
        let c = b[off + i];
        if c == 0x00 || c == 0xFF {
            break;
        }
        if !(0x20..=0x7E).contains(&c) {
            return (text, false);
        }
        text.push(char::from(c));
    }
    (text, true)
}

fn ok_name(b: &[u8], off: usize, n: usize) -> String {
    let (t, ok) = ascii_name(b, off, n);
    if ok {
        t
    } else {
        String::new()
    }
}

fn u24(b: &[u8], o: usize) -> u32 {
    u32::from(b[o]) | (u32::from(b[o + 1]) << 8) | (u32::from(b[o + 2]) << 16)
}

/// A tone word: `Some("")` none, `Some("103.5")` CTCSS (BCD tenths), `Some("D023N")` DCS (bit 15, the code's three octal
/// digits as three BCD digits). `None` for a word that is no tone, such as a DCS digit above 7.
pub fn dm32_tone(b: &[u8], off: usize) -> Option<String> {
    let v = u16le(b, off);
    if v == 0xFFFF || v == 0 {
        return Some(String::new());
    }
    if v & 0x8000 != 0 {
        let d = [(v >> 8) & 0xF, (v >> 4) & 0xF, v & 0xF];
        if d.iter().any(|&x| x > 7) {
            return None;
        }
        return Some(format!("D{}{}{}N", d[0], d[1], d[2]));
    }
    let tenths = bcd(u32::from(v), 4)?;
    (300..=3000)
        .contains(&tenths)
        .then(|| format!("{:.1}", f64::from(tenths) / 10.0))
}

#[derive(Clone, Debug)]
pub struct StockChannel {
    pub index: usize,
    pub name: String,
    pub name_ok: bool,
    pub rx_hz: Option<u64>,
    pub tx_hz: Option<u64>,
    pub dmr: bool,
    pub power: u8,
    pub bw: f64,
    pub scan_list: u8,
    pub timeslot: u8,
    pub color_code: u8,
    pub rx_list: u8,
    pub rx_tone: Option<String>,
    pub tx_tone: Option<String>,
    pub contact: u8,
}

fn read_channel(r: &[u8]) -> Option<StockChannel> {
    let (name, name_ok) = ascii_name(r, 0, 16);
    if name.is_empty() {
        return None;
    }
    Some(StockChannel {
        index: 0,
        name,
        name_ok,
        rx_hz: get_freq(r, 0x10),
        tx_hz: get_freq(r, 0x14),
        dmr: r[0x18] & 0x10 != 0,
        power: POWER
            .get(usize::from((r[0x18] >> 1) & 3))
            .copied()
            .unwrap_or(POWER[2]),
        bw: if r[0x19] & 0x80 != 0 { 25.0 } else { 12.5 },
        scan_list: (r[0x19] >> 2) & 0x1F,
        timeslot: if r[0x1D] & 0x10 != 0 { 2 } else { 1 },
        color_code: r[0x1D] & 0x0F,
        rx_list: r[0x1F],
        rx_tone: dm32_tone(r, 0x21),
        tx_tone: dm32_tone(r, 0x23),
        contact: 0,
    })
}

#[derive(Clone, Debug)]
pub struct StockContact {
    pub index: usize,
    pub name: String,
    pub call_type: CallType,
    pub id: u32,
}

#[derive(Clone, Debug)]
pub struct StockList {
    pub index: usize,
    pub name: String,
    pub ids: Vec<u32>,
}

#[derive(Clone, Debug)]
pub struct StockZone {
    pub name: String,
    pub channels: Vec<u16>,
}

#[derive(Clone, Debug, Default)]
pub struct Stock {
    pub dmr_id: u32,
    pub radio_name: String,
    pub boot_line1: String,
    pub boot_line2: String,
    pub channel_count: usize,
    pub channels: Vec<StockChannel>,
    pub contacts: Vec<StockContact>,
    pub rx_lists: Vec<StockList>,
    pub zones: Vec<StockZone>,
}

/// Is this data flash a stock DM-32UV's, with a codeplug in it?
pub fn detect(bytes: &[u8]) -> bool {
    let Some(m) = page_map(bytes) else {
        return false;
    };
    let Some(ch) = m.get(&BLK_CHANNELS) else {
        return false;
    };
    let n = usize::from(u16le(ch, 0)).min(CH_PER_BLOCK);
    let (mut good, mut bad) = (0, 0);
    for i in 0..n {
        match read_channel(&ch[0x10 + i * CH..0x10 + (i + 1) * CH]) {
            Some(c) if c.name_ok && c.rx_hz.is_some() && c.tx_hz.is_some() => good += 1,
            _ => bad += 1,
        }
    }
    good > 0 && bad * 4 <= good
}

/// Every record in use; channel, contact and zone references as 1-based stock indexes, list members as DMR IDs.
pub fn read_stock(bytes: &[u8]) -> Result<Stock, String> {
    let m = page_map(bytes)
        .filter(|m| m.contains_key(&BLK_CHANNELS))
        .ok_or("no stock DM-32UV codeplug")?;
    let blank = [0xFFu8; PAGE];
    let blk = |id: u8| -> &[u8] { m.get(&id).copied().unwrap_or(&blank) };
    let mut out = Stock::default();

    let s = blk(BLK_SETTINGS);
    out.boot_line1 = ok_name(s, 0x01, 14);
    out.boot_line2 = ok_name(s, 0x0F, 14);
    let ids = blk(BLK_IDS);
    for i in 0..usize::from(ids[0]).min(N_IDS) {
        let a = 0x10 + i * RADIO_ID;
        if a + RADIO_ID > BLOCK_DATA {
            break;
        }
        let id = u24(ids, a);
        if (1..=16_776_415).contains(&id) {
            out.dmr_id = id;
            out.radio_name = ok_name(ids, a + 3, 13);
            break;
        }
    }

    let (ch, tx) = (blk(BLK_CHANNELS), blk(BLK_TX_CONTACTS));
    out.channel_count = usize::from(u16le(ch, 0));
    for i in 0..out.channel_count.min(CH_PER_BLOCK) {
        if let Some(mut c) = read_channel(&ch[0x10 + i * CH..0x10 + (i + 1) * CH]) {
            c.index = i + 1;
            c.contact = if c.dmr && 2 * i + 1 < BLOCK_DATA {
                tx[2 * i + 1]
            } else {
                0
            };
            out.channels.push(c);
        }
    }

    let ct = blk(BLK_CONTACTS);
    let mut i = 0;
    while (i + 1) * CONTACT <= BLOCK_DATA {
        let r = &ct[i * CONTACT..(i + 1) * CONTACT];
        let call_type = match r[0x16] {
            3 => Some(CallType::Private),
            4 => Some(CallType::Group),
            _ => None,
        };
        let (name, id) = (ok_name(r, 2, 16), u24(r, 0x13));
        if let Some(call_type) = call_type {
            if !name.is_empty() && id != 0 {
                out.contacts.push(StockContact {
                    index: i + 1,
                    name,
                    call_type,
                    id,
                });
            }
        }
        i += 1;
    }

    let gl = blk(BLK_LISTS);
    for i in 0..N_LISTS {
        let a = 0x10 + i * LIST;
        if a + LIST > BLOCK_DATA {
            break;
        }
        let name = ok_name(gl, a + 1, 11);
        if name.is_empty() {
            continue;
        }
        let mut ids = Vec::new();
        for k in 0..LIST_MEMBERS {
            let id = u24(gl, a + 0x0C + 3 * k);
            if id == 0 || id == 0xFF_FFFF {
                break;
            }
            ids.push(id);
        }
        out.rx_lists.push(StockList {
            index: i + 1,
            name,
            ids,
        });
    }

    let zb = blk(BLK_ZONES);
    for i in 0..usize::from(zb[0]).min(255) {
        let a = 0x10 + i * ZONE;
        if a + ZONE > BLOCK_DATA {
            break;
        }
        let name = ok_name(zb, a, 16);
        if name.is_empty() {
            continue;
        }
        let n = usize::from(zb[a + 0x10]).min(ZONE_CHANNELS);
        let channels = (0..n)
            .map(|k| u16le(zb, a + 0x11 + 2 * k))
            .filter(|&x| x != 0 && x != 0xFFFF)
            .collect();
        out.zones.push(StockZone { name, channels });
    }
    Ok(out)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Note {
    Shortened,
    TooManyChannels,
    BadFrequency,
    ScanLists,
    TooManyRxLists,
    TooManyContacts,
    ListCut,
    BadTone,
    DcsPolarity,
    ZoneCut,
    TooManyZones,
}

impl Note {
    pub fn text(self, n: usize) -> String {
        match self {
            Note::Shortened => format!("{n} name(s) were longer than the radio allows and were shortened."),
            Note::TooManyChannels => format!("{n} channel(s) did not fit and were left out."),
            Note::BadFrequency => format!("{n} channel(s) had no usable frequency and were left out."),
            Note::ScanLists => format!("{n} channel(s) were in a scan list; scan lists are not imported."),
            Note::TooManyRxLists => format!("{n} RX group list(s) did not fit and were left out."),
            Note::TooManyContacts => format!("{n} contact(s) did not fit and were left out."),
            Note::ListCut => format!("{n} RX group list(s) had more than 32 members and were cut."),
            Note::BadTone => format!("{n} tone(s) the radio can't hold were left out."),
            Note::DcsPolarity => format!("{n} DCS tone(s) were set to normal polarity (N): the stock polarity isn't read yet. Check any that should be inverted."),
            Note::ZoneCut => format!("{n} zone(s) had more than 80 channels and were cut."),
            Note::TooManyZones => format!("{n} zone(s) did not fit and were left out."),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Report {
    pub channels: usize,
    pub contacts: usize,
    pub rx_lists: usize,
    pub zones: usize,
    pub notes: Vec<(Note, usize)>,
}

#[derive(Default)]
struct Notes(Vec<(Note, usize)>);

impl Notes {
    fn add(&mut self, n: Note, by: usize) {
        match self.0.iter_mut().find(|(k, _)| *k == n) {
            Some((_, c)) => *c += by,
            None => self.0.push((n, by)),
        }
    }
}

/// At most `max` rows, those in `wanted` first, the rest in table order; the kept rows in table order.
fn keep_wanted<T: Clone>(
    rows: Vec<T>,
    max: usize,
    index: impl Fn(&T) -> usize,
    wanted: &HashSet<usize>,
) -> Vec<T> {
    if rows.len() <= max {
        return rows;
    }
    let mut keep: Vec<usize> = Vec::new();
    for (i, r) in rows.iter().enumerate() {
        if wanted.contains(&index(r)) && keep.len() < max {
            keep.push(i);
        }
    }
    for i in 0..rows.len() {
        if keep.len() >= max {
            break;
        }
        if !keep.contains(&i) {
            keep.push(i);
        }
    }
    keep.sort_unstable();
    keep.into_iter().map(|i| rows[i].clone()).collect()
}

fn shorten(notes: &mut Notes, t: &str, max: usize) -> String {
    if t.chars().count() <= max {
        return t.to_string();
    }
    notes.add(Note::Shortened, 1);
    t.chars().take(max).collect()
}

/// Fills `cp` from a stock codeplug (the data flash's bytes). Every table the stock codeplug has is replaced; the
/// VFOs lose their references.
pub fn import_stock(cp: &mut Codeplug, bytes: &[u8]) -> Result<Report, String> {
    let mut s = read_stock(bytes)?;
    let mut notes = Notes::default();

    if s.channel_count > CH_PER_BLOCK {
        notes.add(Note::TooManyChannels, s.channel_count - CH_PER_BLOCK);
    }
    let mut stock_channels: Vec<StockChannel> = Vec::new();
    for c in &s.channels {
        if c.rx_hz.is_some() && c.tx_hz.is_some() {
            stock_channels.push(c.clone());
        } else {
            notes.add(Note::BadFrequency, 1);
        }
    }
    if stock_channels.len() > MAX_CHANNELS {
        notes.add(Note::TooManyChannels, stock_channels.len() - MAX_CHANNELS);
        stock_channels.truncate(MAX_CHANNELS);
    }
    if stock_channels.iter().any(|c| c.scan_list != 0) {
        notes.add(Note::ScanLists, 1);
    }

    let list_refs: HashSet<usize> = stock_channels
        .iter()
        .filter(|c| c.dmr && c.rx_list != 0)
        .map(|c| usize::from(c.rx_list))
        .collect();
    let named: Vec<StockList> = s
        .rx_lists
        .iter()
        .filter(|l| !l.ids.is_empty() || list_refs.contains(&l.index))
        .cloned()
        .collect();
    let stock_lists = keep_wanted(named.clone(), MAX_RX_LISTS, |l| l.index, &list_refs);
    if named.len() > stock_lists.len() {
        notes.add(Note::TooManyRxLists, named.len() - stock_lists.len());
    }

    // A list names its members by ID: the contact with that ID, the first such one; an ID without a contact gets a
    // group contact named after it, as the stock radio shows it.
    let mut by_id: HashMap<u32, usize> = HashMap::new();
    for c in &s.contacts {
        by_id.entry(c.id).or_insert(c.index);
    }
    let mut extra = s.contacts.iter().map(|c| c.index).max().unwrap_or(0);
    for l in &stock_lists {
        for &id in &l.ids {
            if by_id.contains_key(&id) {
                continue;
            }
            extra += 1;
            s.contacts.push(StockContact {
                index: extra,
                name: id.to_string(),
                call_type: CallType::Group,
                id,
            });
            by_id.insert(id, extra);
        }
    }
    let mut contact_refs: HashSet<usize> = stock_channels
        .iter()
        .filter(|c| c.dmr && c.contact != 0)
        .map(|c| usize::from(c.contact))
        .collect();
    for l in &stock_lists {
        for id in &l.ids {
            contact_refs.insert(by_id[id]);
        }
    }
    let stock_contacts = keep_wanted(s.contacts.clone(), MAX_CONTACTS, |c| c.index, &contact_refs);
    if s.contacts.len() > stock_contacts.len() {
        notes.add(
            Note::TooManyContacts,
            s.contacts.len() - stock_contacts.len(),
        );
    }

    let mut contact_of: HashMap<usize, u16> = HashMap::new();
    let mut contacts = Vec::new();
    for (i, c) in stock_contacts.iter().enumerate() {
        let number = i as u16 + 1;
        contact_of.insert(c.index, number);
        contacts.push(Contact {
            number,
            name: shorten(&mut notes, &c.name, 16),
            id: c.id,
            call_type: c.call_type,
            timeslot: 0,
        });
    }
    let mut list_of: HashMap<usize, u8> = HashMap::new();
    let mut rx_lists = Vec::new();
    for (i, l) in stock_lists.iter().enumerate() {
        let members: Vec<u16> = l
            .ids
            .iter()
            .filter_map(|id| contact_of.get(&by_id[id]).copied())
            .collect();
        if members.len() > 32 {
            notes.add(Note::ListCut, 1);
        }
        let number = i as u8 + 1;
        list_of.insert(l.index, number);
        rx_lists.push(RxList {
            number,
            name: shorten(&mut notes, &l.name, 16),
            contacts: members.into_iter().take(32).collect(),
        });
    }

    let mut channel_of: HashMap<usize, u16> = HashMap::new();
    let mut channels = Vec::new();
    for (i, c) in stock_channels.iter().enumerate() {
        let number = i as u16 + 1;
        let mut x = Channel::new(number, c.rx_hz.unwrap_or(0), c.tx_hz.unwrap_or(0));
        x.name = shorten(&mut notes, &c.name, 16);
        x.mode = if c.dmr { Mode::Dmr } else { Mode::Fm };
        x.power = c.power;
        let mut tone = |t: &Option<String>| -> String {
            if c.dmr {
                return String::new();
            }
            match t {
                None => {
                    notes.add(Note::BadTone, 1);
                    String::new()
                }
                Some(t) => {
                    if t.starts_with('D') {
                        notes.add(Note::DcsPolarity, 1); // the polarity bit isn't mapped yet: all come as N
                    }
                    t.clone()
                }
            }
        };
        x.rx_tone = tone(&c.rx_tone);
        x.tx_tone = tone(&c.tx_tone);
        x.bw = if c.dmr { 12.5 } else { c.bw };
        x.color_code = if c.dmr { c.color_code } else { 0 };
        x.timeslot = if c.dmr { c.timeslot } else { 1 };
        x.contact = if c.dmr {
            contact_of
                .get(&usize::from(c.contact))
                .copied()
                .unwrap_or(0)
        } else {
            0
        };
        x.rx_list = if c.dmr {
            list_of.get(&usize::from(c.rx_list)).copied().unwrap_or(0)
        } else {
            0
        };
        channel_of.insert(c.index, number);
        channels.push(x);
    }

    let mut zones: Vec<Zone> = Vec::new();
    for z in &s.zones {
        let mut members: Vec<u16> = z
            .channels
            .iter()
            .filter_map(|n| channel_of.get(&usize::from(*n)).copied())
            .collect();
        if members.len() > 80 {
            notes.add(Note::ZoneCut, 1);
            members.truncate(80);
        }
        zones.push(Zone {
            slot: zones.len(),
            name: shorten(&mut notes, &z.name, 16),
            channels: members,
        });
    }
    if zones.len() > MAX_ZONES {
        notes.add(Note::TooManyZones, zones.len() - MAX_ZONES);
        zones.truncate(MAX_ZONES);
    }

    cp.channels = channels;
    cp.contacts = contacts;
    cp.rx_lists = rx_lists;
    cp.zones = zones;
    for v in cp.vfo.iter_mut().flatten() {
        v.contact = 0;
        v.rx_list = 0;
        v.aprs = 0;
    }
    cp.general.dmr_id = s.dmr_id;
    cp.general.radio_name = shorten(&mut notes, &s.radio_name, 8);
    cp.general.boot_line1 = shorten(&mut notes, &s.boot_line1, 16);
    cp.general.boot_line2 = shorten(&mut notes, &s.boot_line2, 16);

    Ok(Report {
        channels: cp.channels.len(),
        contacts: cp.contacts.len(),
        rx_lists: cp.rx_lists.len(),
        zones: cp.zones.len(),
        notes: notes.0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Image;

    fn page(flash: &mut Vec<u8>, slot: usize, block: u8) -> usize {
        let a = PAGES_START + slot * PAGE;
        if flash.len() < PAGES_END {
            flash.resize(PAGES_END, 0xFF);
        }
        flash[a..a + PAGE].fill(0xFF);
        flash[a + PAGE - 1] = block;
        a
    }

    fn put_name(b: &mut [u8], off: usize, n: usize, s: &str) {
        b[off..off + n].fill(0x00);
        b[off..off + s.len()].copy_from_slice(s.as_bytes());
    }

    fn freq(b: &mut [u8], off: usize, hz: u64) {
        crate::fields::put_freq(b, off, hz);
    }

    /// A stock data flash: 3 channels (an FM one with DCS, a DMR one, one with no frequency), contacts, a list, a zone.
    fn flash() -> Vec<u8> {
        let mut f = Vec::new();
        let a = page(&mut f, 0, BLK_CHANNELS);
        f[a..a + 2].copy_from_slice(&3u16.to_le_bytes());
        let c1 = a + 0x10;
        put_name(&mut f, c1, 16, "Shop");
        freq(&mut f, c1 + 0x10, 151_820_000);
        freq(&mut f, c1 + 0x14, 151_820_000);
        f[c1 + 0x18] = 0x04; // FM, power 2 (high)
        f[c1 + 0x19] = 0x80; // wide
        f[c1 + 0x21..c1 + 0x23].copy_from_slice(&0x8265u16.to_le_bytes());
        f[c1 + 0x23..c1 + 0x25].copy_from_slice(&0x8265u16.to_le_bytes());
        let c2 = a + 0x10 + CH;
        put_name(&mut f, c2, 16, "Local DMR");
        freq(&mut f, c2 + 0x10, 445_720_000);
        freq(&mut f, c2 + 0x14, 440_720_000);
        f[c2 + 0x18] = 0x10 | 0x02; // DMR, power 1 (mid)
        f[c2 + 0x1D] = 0x10 | 0x03; // slot 2, color code 3
        f[c2 + 0x1F] = 1; // RX group list 1
        f[c2 + 0x21..c2 + 0x23].copy_from_slice(&0xFFFFu16.to_le_bytes());
        let c3 = a + 0x10 + 2 * CH;
        put_name(&mut f, c3, 16, "Broken");
        // no frequency
        let t = page(&mut f, 1, BLK_TX_CONTACTS);
        f[t + 2 + 1] = 2; // channel 2's TX contact: contact 2
        let k = page(&mut f, 2, BLK_CONTACTS);
        for (i, (name, id, ty)) in [("Local", 2u32, 4u8), ("Parrot", 9990, 4)]
            .iter()
            .enumerate()
        {
            let r = k + i * CONTACT;
            put_name(&mut f, r + 2, 16, name);
            f[r + 0x13..r + 0x16].copy_from_slice(&id.to_le_bytes()[..3]);
            f[r + 0x16] = *ty;
        }
        let l = page(&mut f, 3, BLK_LISTS);
        put_name(&mut f, l + 0x10 + 1, 11, "Friends");
        f[l + 0x10 + 0x0C..l + 0x10 + 0x0F].copy_from_slice(&2u32.to_le_bytes()[..3]);
        f[l + 0x10 + 0x0F..l + 0x10 + 0x12].copy_from_slice(&777u32.to_le_bytes()[..3]); // an ID with no contact
        f[l + 0x10 + 0x12..l + 0x10 + 0x15].copy_from_slice(&[0xFF, 0xFF, 0xFF]);
        let z = page(&mut f, 4, BLK_ZONES);
        f[z] = 1;
        put_name(&mut f, z + 0x10, 16, "Main");
        f[z + 0x10 + 0x10] = 2;
        f[z + 0x10 + 0x11..z + 0x10 + 0x13].copy_from_slice(&1u16.to_le_bytes());
        f[z + 0x10 + 0x13..z + 0x10 + 0x15].copy_from_slice(&2u16.to_le_bytes());
        let g = page(&mut f, 5, BLK_IDS);
        f[g] = 1;
        f[g + 0x10..g + 0x13].copy_from_slice(&3_100_123u32.to_le_bytes()[..3]);
        put_name(&mut f, g + 0x10 + 3, 13, "N0CALL");
        let s = page(&mut f, 6, BLK_SETTINGS);
        put_name(&mut f, s + 1, 14, "Hello");
        f
    }

    #[test]
    fn dcs_words_are_bcd_digits() {
        let t = |v: u16| dm32_tone(&v.to_le_bytes(), 0);
        assert_eq!(t(0x8073).as_deref(), Some("D073N"));
        assert_eq!(t(0x8265).as_deref(), Some("D265N"));
        assert_eq!(t(0x8058), None);
        assert_eq!(t(0x1035).as_deref(), Some("103.5"));
        assert_eq!(t(0x0670).as_deref(), Some("67.0"));
        assert_eq!(t(0xFFFF).as_deref(), Some(""));
    }

    #[test]
    fn detects_and_reads_a_stock_flash() {
        let mut f = flash();
        assert!(!detect(&f)); // one of three channels has no frequency: too many bad ones
        let c3 = PAGES_START + 0x10 + 2 * CH;
        freq(&mut f, c3 + 0x10, 146_520_000);
        freq(&mut f, c3 + 0x14, 146_520_000);
        assert!(detect(&f));
        assert!(!detect(&[0xFF; 0x1000]));
        let s = read_stock(&f).unwrap();
        assert_eq!(
            (s.dmr_id, s.radio_name.as_str(), s.boot_line1.as_str()),
            (3_100_123, "N0CALL", "Hello")
        );
        assert_eq!(s.channels.len(), 3);
        assert_eq!(s.contacts.len(), 2);
    }

    #[test]
    fn imports_into_a_codeplug() {
        let mut cp = Codeplug::decode(&Image::blank());
        let r = import_stock(&mut cp, &flash()).unwrap();
        assert_eq!((r.channels, r.contacts, r.rx_lists, r.zones), (2, 3, 1, 1));
        assert_eq!(cp.channels[0].name, "Shop");
        assert_eq!(
            (
                cp.channels[0].rx_tone.as_str(),
                cp.channels[0].tx_tone.as_str()
            ),
            ("D265N", "D265N")
        );
        assert_eq!(cp.channels[0].power, 9);
        assert_eq!(cp.channels[0].bw, 25.0);
        let d = &cp.channels[1];
        assert_eq!(
            (d.mode, d.power, d.timeslot, d.color_code, d.bw),
            (Mode::Dmr, 6, 2, 3, 12.5)
        );
        assert_eq!((d.contact, d.rx_list), (2, 1));
        // the list's second member (ID 777) had no contact: one was made, named after it
        assert_eq!(cp.contacts[2].name, "777");
        assert_eq!(cp.rx_lists[0].contacts, vec![1, 3]);
        assert_eq!(cp.zones[0].channels, vec![1, 2]);
        assert_eq!(cp.general.dmr_id, 3_100_123);
        assert!(r
            .notes
            .iter()
            .any(|(n, c)| *n == Note::BadFrequency && *c == 1));
        assert!(r.notes.iter().any(|(n, _)| *n == Note::DcsPolarity));
    }

    #[test]
    fn what_was_imported_encodes_and_decodes() {
        let mut cp = Codeplug::decode(&Image::blank());
        import_stock(&mut cp, &flash()).unwrap();
        let img = cp.encode(&Image::blank());
        let back = Codeplug::decode(&img);
        assert_eq!(back.channels.len(), 2);
        assert_eq!(back.channels[0].rx_tone, "D265N");
        assert_eq!(back.contacts.len(), 3);
        assert!(crate::problems::validate(&back, None)
            .iter()
            .all(|p| p.level != crate::problems::Level::Error));
    }
}
