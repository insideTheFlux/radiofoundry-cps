// SPDX-License-Identifier: MIT
// Copyright (c) 2026 The RadioFoundry Authors
// Copyright (c) 2026 insideTheFlux
//
// The DMR ID database in format C, the compact layout (RadioFoundry `docs/spec/iddb-compact.md`): a dictionary of the
// strings that repeat and record pages that a refresh changes few of. Ported from `site/static/js/codeplug/iddb.js`.
// Database offsets are image addresses from `AREA1`, running on into `AREA2` after area 1.

use crate::iddb::{clean, le, read_header, Record, Written, ALPHABET, AREA1, AREA1_SIZE, AREA2, C_HEADER, PAGE, TEXT_MAX};
use crate::image::Image;
use std::collections::{HashMap, HashSet};

/// A fresh page is closed at 90 %: room for the records a refresh adds.
const PAGE_FILL: usize = 3686;
const DICT_MAX: usize = 16512;
/// A literal token is this plus its length; this alone is an empty token.
const LITERAL: u8 = 0xC0;
/// The index and the dictionary get their size and a quarter.
const SLACK: f64 = 1.25;

/// Database offset to image address.
pub(crate) fn db_addr(o: usize) -> usize {
    if o < AREA1_SIZE {
        AREA1 + o
    } else {
        AREA2 + (o - AREA1_SIZE)
    }
}

/// The database space a radio has: area 1, then area 2 up to `end` (an image address), in whole pages. `None`: no limit.
fn space(end: usize) -> Option<usize> {
    if end == usize::MAX {
        None
    } else {
        Some(AREA1_SIZE + (end.saturating_sub(AREA2) / PAGE) * PAGE)
    }
}

fn put(b: &mut [u8], o: usize, mut v: usize, n: usize) {
    for i in 0..n {
        b[o + i] = (v & 0xFF) as u8;
        v >>= 8;
    }
}

/// `n` bytes at a database offset (they may run from area 1 into area 2); missing bytes read as erased.
fn db_bytes(b: &[u8], o: usize, n: usize) -> Vec<u8> {
    (0..n).map(|i| b.get(db_addr(o + i)).copied().unwrap_or(0xFF)).collect()
}

fn alpha(c: u8) -> usize {
    ALPHABET.iter().position(|&x| x == c).unwrap_or(0)
}

fn pack6(text: &str) -> Vec<u8> {
    let t = text.as_bytes();
    let mut out = vec![0u8; (t.len() * 6).div_ceil(8)];
    for (j, &ch) in t.iter().enumerate() {
        let c = alpha(ch);
        for k in 0..6 {
            if c & (0x20 >> k) != 0 {
                out[(6 * j + k) >> 3] |= 0x80 >> ((6 * j + k) & 7);
            }
        }
    }
    out
}

fn unpack6(b: &[u8], o: usize, chars: usize) -> String {
    let mut s = String::new();
    for j in 0..chars {
        let mut c = 0usize;
        for k in 6 * j..6 * j + 6 {
            c = (c << 1) | usize::from((b.get(o + (k >> 3)).copied().unwrap_or(0) >> (7 - (k & 7))) & 1);
        }
        s.push(char::from(ALPHABET[c]));
    }
    s
}

struct Rec {
    id: u32,
    text: String,
    slots: [String; 3],
}

/// A text's three tokens (3.4): the encoder's split at the first two spaces.
fn slots(text: &str) -> [String; 3] {
    match text.find(' ') {
        None => [text.to_string(), String::new(), String::new()],
        Some(a) => match text[a + 1..].find(' ') {
            None => [text[..a].to_string(), text[a + 1..].to_string(), String::new()],
            Some(b) => {
                let b = a + 1 + b;
                [text[..a].to_string(), text[a + 1..b].to_string(), text[b + 1..].to_string()]
            }
        },
    }
}

/// Records as the radio takes them: sorted by ID, the first of each ID kept, texts cleaned and cut to 48.
fn prepare(records: &[Record]) -> Vec<Rec> {
    let mut seen = HashSet::new();
    let mut v: Vec<Rec> = records
        .iter()
        .filter(|r| (1..=0xFF_FFFF).contains(&r.id) && seen.insert(r.id))
        .map(|r| {
            let t: String = clean(&r.text).chars().take(TEXT_MAX).collect();
            let text = t.trim().to_string();
            Rec { id: r.id, slots: slots(&text), text }
        })
        .collect();
    v.sort_by_key(|r| r.id);
    v
}

fn literal_size(s: &str) -> usize {
    1 + (s.len() * 6).div_ceil(8)
}

fn varint(mut v: u32, out: &mut Vec<u8>) {
    while v >= 0x80 {
        out.push((v & 0x7F) as u8 | 0x80);
        v /= 128;
    }
    out.push(v as u8);
}

fn token(s: &str, codes: &HashMap<String, usize>, out: &mut Vec<u8>) {
    if s.is_empty() {
        out.push(LITERAL);
        return;
    }
    match codes.get(s) {
        None => {
            out.push(LITERAL + s.len() as u8);
            out.extend(pack6(s));
        }
        Some(&k) if k < 128 => out.push(k as u8),
        Some(&k) => {
            out.push(0x80 + ((k - 128) >> 8) as u8);
            out.push(((k - 128) & 0xFF) as u8);
        }
    }
}

/// One record's bytes: the ID (whole, or the gap from `prev`), then its three tokens.
fn encode_record(r: &Rec, prev: Option<u32>, codes: &HashMap<String, usize>) -> Vec<u8> {
    let mut out = Vec::new();
    varint(prev.map_or(r.id, |p| r.id - p), &mut out);
    for t in &r.slots {
        token(t, codes, &mut out);
    }
    out
}

/// A record page's bytes (count, then records), or `None` when they don't fit a page.
fn encode_page(records: &[Rec], codes: &HashMap<String, usize>) -> Option<Vec<u8>> {
    let mut out = vec![(records.len() & 0xFF) as u8, (records.len() >> 8) as u8];
    for (i, r) in records.iter().enumerate() {
        out.extend(encode_record(r, (i > 0).then(|| records[i - 1].id), codes));
    }
    (out.len() <= PAGE).then_some(out)
}

/// The strings worth a dictionary entry (4.1 rule 2), most used first; `known`: entries that stay, never offered again.
fn candidates(list: &[Rec], known: &HashSet<&str>) -> Vec<String> {
    let mut order: Vec<&str> = Vec::new();
    let mut uses: HashMap<&str, usize> = HashMap::new();
    for r in list {
        for s in &r.slots {
            if !s.is_empty() && !known.contains(s.as_str()) {
                let e = uses.entry(s.as_str()).or_insert_with(|| {
                    order.push(s.as_str());
                    0
                });
                *e += 1;
            }
        }
    }
    let mut v: Vec<(&str, usize)> = order.into_iter().map(|s| (s, uses[s])).filter(|&(s, n)| n * literal_size(s) > n * 2 + literal_size(s) + 3).collect();
    v.sort_by(|x, y| y.1.cmp(&x.1).then_with(|| x.0.cmp(y.0)));
    v.into_iter().map(|(s, _)| s.to_string()).collect()
}

struct Parsed {
    h: crate::iddb::Header,
    dict: Vec<String>,
    /// (first ID, page)
    index: Vec<(u32, usize)>,
}

/// What a format C database in the image holds: header, dictionary, index.
fn parse(b: &[u8]) -> Option<Parsed> {
    let h = read_header(b)?;
    let c = h.compact.clone()?;
    let table = db_bytes(b, c.table, 3 * c.dict);
    let mut dict = Vec::new();
    for k in 0..c.dict {
        let o = c.strings + le(&table, 3 * k, 3) as usize;
        let n = usize::from(b.get(db_addr(o)).copied().unwrap_or(0xFF));
        dict.push(unpack6(&db_bytes(b, o + 1, (n * 6).div_ceil(8)), 0, n));
    }
    let ix = db_bytes(b, c.index, 5 * c.pages);
    let index = (0..c.pages).map(|i| (le(&ix, 5 * i, 3) as u32, le(&ix, 5 * i + 3, 2) as usize)).collect();
    Some(Parsed { h, dict, index })
}

/// Every record of a format C database.
pub(crate) fn read(b: &[u8]) -> Vec<Record> {
    let Some(db) = parse(b) else { return Vec::new() };
    let mut out = Vec::new();
    for &(_, page) in &db.index {
        let a = db_addr(page * PAGE);
        let p = b.get(a..(a + PAGE).min(b.len())).unwrap_or(&[]);
        let at = |i: usize| p.get(i).copied().unwrap_or(0);
        let count = usize::from(at(0)) | usize::from(at(1)) << 8;
        let (mut i, mut id) = (2usize, 0u64);
        let mut n = 0;
        while n < count && i < PAGE {
            let (mut v, mut shift) = (0u64, 1u64);
            loop {
                let x = at(i);
                i += 1;
                v = v.wrapping_add(u64::from(x & 0x7F).wrapping_mul(shift));
                shift = shift.wrapping_mul(128);
                if x & 0x80 == 0 {
                    break;
                }
            }
            id = if n == 0 { v } else { id.wrapping_add(v) };
            let mut parts: Vec<String> = Vec::new();
            for _ in 0..3 {
                let t = at(i);
                i += 1;
                if t < 0x80 {
                    parts.push(db.dict.get(usize::from(t)).cloned().unwrap_or_default());
                } else if t < 0xC0 {
                    let k = 128 + (usize::from(t - 0x80) << 8) + usize::from(at(i));
                    i += 1;
                    parts.push(db.dict.get(k).cloned().unwrap_or_default());
                } else if t > 0xC0 {
                    parts.push(unpack6(p, i, usize::from(t - 0xC0)));
                    i += (usize::from(t - 0xC0) * 6).div_ceil(8);
                }
            }
            let text = parts.into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" ");
            out.push(Record { id: id as u32, text });
            n += 1;
        }
    }
    out
}

/// A page and the records on it, as a range of the sorted list.
struct PageRecs {
    page: usize,
    from: usize,
    to: usize,
}

/// Records into pages: fresh (each closed at `PAGE_FILL`), or into the old pages' ranges (one that overflows splits in
/// two, the second half to a new page from `first_new` on).
fn paginate(list: &[Rec], codes: &HashMap<String, usize>, old: Option<&[(u32, usize)]>, first_new: usize) -> Vec<PageRecs> {
    let mut pages = Vec::new();
    let mut next = first_new;
    let Some(old) = old else {
        let (mut start, mut used) = (0usize, 2usize);
        for (i, r) in list.iter().enumerate() {
            let prev = (i > start).then(|| list[i - 1].id);
            let n = encode_record(r, prev, codes).len();
            if i > start && used + n > PAGE_FILL {
                pages.push(PageRecs { page: next, from: start, to: i });
                next += 1;
                start = i;
                used = 2 + encode_record(r, None, codes).len();
            } else {
                used += n;
            }
        }
        if start < list.len() {
            pages.push(PageRecs { page: next, from: start, to: list.len() });
        }
        return pages;
    };
    // the records of each old page's range, in order
    let mut groups: Vec<(usize, usize, usize)> = old.iter().map(|e| (e.1, 0, 0)).collect();
    let mut g = 0;
    let mut i = 0;
    while i < list.len() {
        while g + 1 < old.len() && old[g + 1].0 <= list[i].id {
            g += 1;
        }
        // records are sorted, so each group is one run; its start is where it first got a record
        if groups[g].2 == groups[g].1 {
            groups[g].1 = i;
        }
        groups[g].2 = i + 1;
        i += 1;
    }
    fn place(list: &[Rec], codes: &HashMap<String, usize>, from: usize, to: usize, page: usize, pages: &mut Vec<PageRecs>, next: &mut usize) {
        let n = to - from;
        if n < 2 || encode_page(&list[from..to], codes).is_some() {
            pages.push(PageRecs { page, from, to });
            return;
        }
        let half = n.div_ceil(2);
        place(list, codes, from, from + half, page, pages, next);
        let p = *next;
        *next += 1;
        place(list, codes, from + half, to, p, pages, next);
    }
    for &(page, from, to) in &groups {
        if to > from {
            place(list, codes, from, to, page, &mut pages, &mut next);
        }
    }
    pages
}

fn pages_for(n: usize) -> usize {
    (((n as f64 * SLACK) / PAGE as f64).ceil() as usize).max(1)
}

struct Sections {
    index: usize,
    table: usize,
    strings: usize,
    records: usize,
}

struct Layout {
    dict: Vec<String>,
    codes: HashMap<String, usize>,
    offsets: Vec<usize>,
    sbytes: usize,
    at: Sections,
    pages: Vec<PageRecs>,
}

/// Where everything goes. Against `old`: its sections and pages kept, or `None` when the index or dictionary outgrew its
/// room.
fn layout(list: &[Rec], old: Option<&Parsed>) -> Option<Layout> {
    let mut dict: Vec<String> = old.map_or_else(Vec::new, |o| o.dict.clone());
    let known: HashSet<String> = dict.iter().cloned().collect();
    let known_ref: HashSet<&str> = known.iter().map(String::as_str).collect();
    for s in candidates(list, &known_ref) {
        if dict.len() >= DICT_MAX {
            break;
        }
        dict.push(s);
    }
    let codes: HashMap<String, usize> = dict.iter().enumerate().map(|(k, s)| (s.clone(), k)).collect();
    let mut offsets = Vec::new();
    let mut sbytes = 0;
    for s in &dict {
        offsets.push(sbytes);
        sbytes += literal_size(s);
    }
    if let Some(old) = old {
        let c = old.h.compact.as_ref()?;
        let at = Sections { index: c.index / PAGE, table: c.table / PAGE, strings: c.strings / PAGE, records: c.first };
        let pages = paginate(list, &codes, Some(&old.index), old.h.size / PAGE);
        if 5 * pages.len() > (at.table.saturating_sub(at.index)) * PAGE
            || 3 * dict.len() > (at.strings.saturating_sub(at.table)) * PAGE
            || sbytes > (at.records.saturating_sub(at.strings)) * PAGE
        {
            return None;
        }
        return Some(Layout { dict, codes, offsets, sbytes, at, pages });
    }
    let first = paginate(list, &codes, None, 0);
    let index = 1;
    let table = index + pages_for(5 * first.len());
    let strings = table + pages_for(3 * dict.len());
    let records = strings + pages_for(sbytes);
    let pages = first.into_iter().map(|p| PageRecs { page: records + p.page, ..p }).collect();
    Some(Layout { dict, codes, offsets, sbytes, at: Sections { index, table, strings, records }, pages })
}

/// Pages the layout reaches.
fn extent_of(l: &Layout) -> usize {
    l.pages.iter().map(|p| p.page + 1).chain(std::iter::once(l.at.records)).max().unwrap_or(0)
}

/// The database written into the image in format C, which grows as needed.
pub(crate) fn write(image: &mut Image, records: &[Record], end: usize, fresh: bool) -> Written {
    let mut list = prepare(records);
    let room = space(end);
    let old = if fresh { None } else { parse(image.bytes()) };
    let mut l = old.as_ref().and_then(|o| layout(&list, Some(o)));
    if l.as_ref().is_none_or(|l| room.is_some_and(|r| extent_of(l) * PAGE > r)) {
        l = layout(&list, None);
    }
    let mut l = l.expect("a fresh layout always exists");
    // too big for the radio: the top of the ID range goes, as with format N
    let mut dropped = 0;
    if let Some(room) = room {
        while extent_of(&l) * PAGE > room && !list.is_empty() {
            let keep = (list.len() - 1).min(((list.len() as f64 * room as f64 * 0.995) / (extent_of(&l) * PAGE) as f64).floor() as usize);
            dropped += list.len() - keep;
            list.truncate(keep);
            l = layout(&list, None).expect("a fresh layout always exists");
        }
    }
    let size = extent_of(&l);
    image.ensure(db_addr(size * PAGE - 1) + 1);
    let b = image.bytes_mut();
    let section = |b: &mut [u8], from: usize, to: usize, bytes: &[u8]| {
        for p in from..to {
            let a = db_addr(p * PAGE);
            b[a..a + PAGE].fill(0xFF);
        }
        for (i, &x) in bytes.iter().enumerate() {
            b[db_addr(from * PAGE + i)] = x;
        }
    };
    let mut sorted: Vec<&PageRecs> = l.pages.iter().collect();
    sorted.sort_by_key(|p| list[p.from].id);
    let mut index = Vec::new();
    for p in &sorted {
        let id = list[p.from].id;
        index.extend([id as u8, (id >> 8) as u8, (id >> 16) as u8, p.page as u8, (p.page >> 8) as u8]);
    }
    let mut table = Vec::new();
    for &o in &l.offsets {
        table.extend([o as u8, (o >> 8) as u8, (o >> 16) as u8]);
    }
    let mut strs = Vec::new();
    for s in &l.dict {
        strs.push(s.len() as u8);
        strs.extend(pack6(s));
    }
    section(b, l.at.index, l.at.table, &index);
    section(b, l.at.table, l.at.strings, &table);
    section(b, l.at.strings, l.at.records, &strs);
    for p in &l.pages {
        let a = db_addr(p.page * PAGE);
        b[a..a + PAGE].fill(0xFF);
        let bytes = encode_page(&list[p.from..p.to], &l.codes).expect("a page of records fits (a record is under 200 bytes)");
        b[a..a + bytes.len()].copy_from_slice(&bytes);
    }
    let a = db_addr(0);
    b[a..a + PAGE].fill(0xFF);
    let longest = list.iter().map(|r| r.text.chars().count()).max().unwrap_or(0);
    b[a..a + 6].copy_from_slice(&[0x49, 0x64, 0x43, 0, 1, longest as u8]);
    let h = &mut b[a..a + C_HEADER];
    put(h, 6, l.at.records, 2);
    put(h, 8, list.len(), 4);
    put(h, 12, size * PAGE, 4);
    put(h, 16, l.at.index * PAGE, 4);
    put(h, 20, sorted.len(), 2);
    put(h, 22, l.dict.len(), 2);
    put(h, 24, l.at.table * PAGE, 4);
    put(h, 28, l.at.strings * PAGE, 4);
    Written { count: list.len(), top: db_addr(size * PAGE - 1) + 1, dropped, rec: 0, format: b'C' }
}

/// About how many bytes format C takes for these records, and how many fit before `end` (`usize::MAX`: any): the
/// whole list written fresh.
pub(crate) fn estimate(records: &[Record], end: usize) -> (usize, usize) {
    let list = prepare(records);
    if list.is_empty() {
        return (0, 0);
    }
    let l = layout(&list, None).expect("a fresh layout always exists");
    let body = (l.sbytes + 3 * l.dict.len() + l.pages.len() * PAGE) as f64 / list.len() as f64;
    let bytes = PAGE + (body * list.len() as f64).ceil() as usize;
    let fit = match space(end) {
        None => usize::MAX,
        Some(room) => (((room as f64 - (PAGE * 4) as f64) / body).floor().max(0.0)) as usize,
    };
    (bytes, fit)
}
