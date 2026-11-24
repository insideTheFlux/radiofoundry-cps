// SPDX-License-Identifier: MIT
// Copyright (c) 2026 The RadioFoundry Authors
// Copyright (c) 2026 insideTheFlux
//
// The DMR ID database: names for DMR IDs that aren't contacts (open-firmware-cps.md 7.5). Ported from RadioFoundry's
// `site/static/js/codeplug/iddb.js`. Format N: a 12-byte header, then records sorted by ID, each a 3-byte LE ID and
// text packed at 6 bits per character (4 characters in 3 bytes). Area 1 holds the header and as many whole records as
// fit in 256 KB; the rest continue at the start of area 2, up to the end of the radio's codeplug space. A format C
// header (iddb-compact.md) is recognized, not yet written or read. Image addresses as in `image.rs`.

use crate::image::{Image, FL};
use unicode_normalization::UnicodeNormalization;

pub const AREA1: usize = FL + 0x30000;
pub const AREA1_SIZE: usize = 0x40000;
pub const AREA2: usize = FL + 0xB8000;
pub const HEADER: usize = 12;
/// The most characters a record keeps (`DMR_IDDB_TEXT_MAX`).
pub const TEXT_MAX: usize = 48;
pub const TEXT_LENGTHS: [usize; 4] = [16, 24, 32, 48];
pub(crate) const ALPHABET: &[u8; 64] = b" 0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz.";
pub(crate) const C_HEADER: usize = 32;
pub(crate) const PAGE: usize = 4096;

/// The bytes a record takes for a text length.
pub const fn record_size(text_len: usize) -> usize {
    3 + text_len * 3 / 4
}

/// Letters without a decomposed form, spelled in plain Latin (the rest lose their accents through NFD).
pub(crate) fn fold(c: char) -> Option<&'static str> {
    Some(match c {
        'Ł' => "L",
        'ł' => "l",
        'Đ' | 'Ð' => "D",
        'đ' | 'ð' => "d",
        'Ħ' => "H",
        'ħ' => "h",
        'ı' => "i",
        'Œ' => "OE",
        'œ' => "oe",
        'Æ' => "AE",
        'æ' => "ae",
        'Ø' => "O",
        'ø' => "o",
        'ß' => "ss",
        'Þ' => "Th",
        'þ' => "th",
        _ => return None,
    })
}

/// Text the 6-bit alphabet can hold: accents dropped (é to e), anything else outside it a space, spaces collapsed.
pub fn clean(text: &str) -> String {
    let mut folded = String::new();
    for c in text.chars() {
        match fold(c) {
            Some(f) => folded.push_str(f),
            None => folded.push(c),
        }
    }
    let mut out = String::new();
    let mut space = false;
    for c in folded.nfd() {
        // combining marks (U+0300 to U+036F) are dropped
        if ('\u{300}'..='\u{36f}').contains(&c) {
            continue;
        }
        let ok = c == ' ' || c == '.' || c.is_ascii_alphanumeric();
        let c = if ok { c } else { ' ' };
        if c == ' ' {
            if !space {
                out.push(' ');
            }
            space = true;
        } else {
            out.push(c);
            space = false;
        }
    }
    out.trim().to_string()
}

/// Records that fit between `AREA1` and `end` (an image address) for a record size.
pub fn capacity(rec: usize, end: usize) -> usize {
    let per1 = (AREA1_SIZE - HEADER) / rec;
    per1 + end.saturating_sub(AREA2) / rec
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Header {
    /// `b'N'`, `b'n'`, `b'C'`, or another letter for formats this editor only counts.
    pub format: u8,
    /// Bytes per record; 0 for format C.
    pub rec: usize,
    pub count: u32,
    pub text_len: usize,
    /// Format C: the database space in bytes; 0 otherwise.
    pub size: usize,
    /// Format C: where its sections are.
    pub compact: Option<Compact>,
}

/// A format C header's section offsets (iddb-compact.md 3.1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Compact {
    /// The first record page.
    pub first: usize,
    /// Database offsets of the index, the dictionary's table and its strings.
    pub index: usize,
    pub table: usize,
    pub strings: usize,
    /// Index entries (record pages in use) and dictionary entries.
    pub pages: usize,
    pub dict: usize,
}

pub(crate) fn le(b: &[u8], o: usize, n: usize) -> u64 {
    (0..n).rev().fold(0u64, |v, i| v * 256 + u64::from(b[o + i]))
}

fn compact_header(b: &[u8]) -> Option<Header> {
    let h = b.get(AREA1..AREA1 + C_HEADER)?;
    if h[4] != 1 {
        return None;
    }
    let (count, size) = (le(h, 8, 4), le(h, 12, 4));
    if count == 0 || size == 0 || size % PAGE as u64 != 0 {
        return None;
    }
    Some(Header {
        format: b'C',
        rec: 0,
        count: count as u32,
        text_len: usize::from(h[5]),
        size: size as usize,
        compact: Some(Compact {
            first: le(h, 6, 2) as usize,
            index: le(h, 16, 4) as usize,
            pages: le(h, 20, 2) as usize,
            dict: le(h, 22, 2) as usize,
            table: le(h, 24, 4) as usize,
            strings: le(h, 28, 4) as usize,
        }),
    })
}

/// The header as stored, or `None` without a database.
pub fn read_header(b: &[u8]) -> Option<Header> {
    let h = b.get(AREA1..AREA1 + HEADER)?;
    if h[0] == 0x49 && h[1] == 0x64 && h[2] == 0x43 && h[3] == 0 {
        return compact_header(b);
    }
    if h[0] != 0x49 || h[1] != 0x64 || h[3] < 0x4A {
        return None;
    }
    let count = le(h, 8, 4) as u32;
    let packed = h[2] == b'N' || h[2] == b'n';
    let rec = usize::from(h[3] - 0x4A);
    if count == 0 || rec <= if packed { 3 } else { 4 } {
        return None;
    }
    // Whole characters only: a record size another program chose (qdmr takes any) can end in spare bits, as the
    // radio's decoder (dmr_iddb.c) rounds down too
    let text_len = if packed { (rec - 3) * 4 / 3 } else { rec - 4 };
    Some(Header { format: h[2], rec, count, text_len, size: 0, compact: None })
}

fn rec_addr(i: usize, rec: usize) -> usize {
    let per1 = (AREA1_SIZE - HEADER) / rec;
    if i < per1 {
        AREA1 + HEADER + i * rec
    } else {
        AREA2 + (i - per1) * rec
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    pub id: u32,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Written {
    pub count: usize,
    /// The image address the database reaches.
    pub top: usize,
    /// Records that did not fit before `end`.
    pub dropped: usize,
    /// Bytes per record (format N); 0 for format C.
    pub rec: usize,
    pub format: u8,
}

#[derive(Debug, PartialEq, Eq)]
pub enum IddbError {
    TextLength(usize),
}

impl std::fmt::Display for IddbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IddbError::TextLength(n) => write!(f, "text length {n}: one of 16, 24, 32, 48"),
        }
    }
}

/// The database in format N written into the image, which grows as needed. Duplicates keep the first; texts are
/// cleaned and cut to `text_len`; at most `capacity` records are kept (the rest are counted as dropped).
pub fn write_n(image: &mut Image, records: &[Record], text_len: usize, end: usize) -> Result<Written, IddbError> {
    if !TEXT_LENGTHS.contains(&text_len) {
        return Err(IddbError::TextLength(text_len));
    }
    let rec = record_size(text_len);
    let mut seen = std::collections::HashSet::new();
    let mut list: Vec<&Record> = records.iter().filter(|r| (1..=0xFF_FFFF).contains(&r.id) && seen.insert(r.id)).collect();
    list.sort_by_key(|r| r.id);
    let fit = capacity(rec, end);
    let dropped = list.len().saturating_sub(fit);
    list.truncate(fit);
    let top = if list.is_empty() { AREA1 + HEADER } else { rec_addr(list.len() - 1, rec) + rec };
    image.ensure(top);
    let b = image.bytes_mut();
    b[AREA1..AREA1 + 8].copy_from_slice(&[0x49, 0x64, 0x4E, rec as u8 + 0x4A, 0x30, 0x30, 0x31, 0x00]);
    b[AREA1 + 8..AREA1 + 12].copy_from_slice(&(list.len() as u32).to_le_bytes());
    for (i, r) in list.iter().enumerate() {
        pack_record(b, rec_addr(i, rec), r.id, &clean(&r.text), text_len);
    }
    Ok(Written { count: list.len(), top, dropped, rec, format: b'N' })
}

/// No database: the header's tag erased (the radio then ignores the records).
pub fn erase(image: &mut Image) {
    image.bytes_mut()[AREA1..AREA1 + HEADER].fill(0xFF);
}

fn pack_record(b: &mut [u8], a: usize, id: u32, text: &str, text_len: usize) {
    b[a] = id as u8;
    b[a + 1] = (id >> 8) as u8;
    b[a + 2] = (id >> 16) as u8;
    let mut t: Vec<u8> = text.bytes().take(text_len).collect();
    t.resize(text_len, b' ');
    for k in 0..text_len / 4 {
        let mut w = 0u32;
        for j in 0..4 {
            let c = ALPHABET.iter().position(|&x| x == t[4 * k + j]).unwrap_or(0) as u32;
            w = (w << 6) | c;
        }
        b[a + 3 + 3 * k] = (w >> 16) as u8;
        b[a + 4 + 3 * k] = (w >> 8) as u8;
        b[a + 5 + 3 * k] = w as u8;
    }
}

/// Every record of a format N or C database, for a preview or a check. Other formats give none.
pub fn read(b: &[u8]) -> Vec<Record> {
    let Some(h) = read_header(b) else { return Vec::new() };
    if h.format == b'C' {
        return crate::iddb_compact::read(b);
    }
    if h.format != b'N' {
        return Vec::new();
    }
    let mut out = Vec::new();
    for i in 0..h.count as usize {
        let a = rec_addr(i, h.rec);
        if a + h.rec > b.len() {
            break;
        }
        let id = u32::from(b[a]) | u32::from(b[a + 1]) << 8 | u32::from(b[a + 2]) << 16;
        // Bit by bit, most significant first: text_len need not be a multiple of 4
        let mut text = String::new();
        for j in 0..h.text_len {
            let mut c = 0usize;
            for k in 6 * j..6 * j + 6 {
                c = (c << 1) | usize::from((b[a + 3 + (k >> 3)] >> (7 - (k & 7))) & 1);
            }
            text.push(char::from(ALPHABET[c]));
        }
        out.push(Record { id, text: text.trim_end_matches(' ').to_string() });
    }
    out
}

/// The image address just past a database (where a read must reach to hold it whole), or 0 without one.
pub fn extent(b: &[u8]) -> usize {
    match read_header(b) {
        Some(h) if h.format == b'C' => crate::iddb_compact::db_addr(h.size - 1) + 1,
        Some(h) => rec_addr(h.count as usize - 1, h.rec) + h.rec,
        None => 0,
    }
}

/// About how many bytes format C takes for these records and how many fit before `end` (`usize::MAX`: any room).
pub fn estimate_compact(records: &[Record], end: usize) -> (usize, usize) {
    crate::iddb_compact::estimate(records, end)
}

/// How `write` lays the database out.
#[derive(Clone, Copy, Debug)]
pub struct Options {
    /// `b'N'` or `b'C'`.
    pub format: u8,
    /// Format N: characters per record (one of `TEXT_LENGTHS`).
    pub text_len: usize,
    /// The image address the database may reach.
    pub end: usize,
    /// Format C: write from nothing, not against a database already in the image (iddb-compact.md 4.2).
    pub fresh: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self { format: b'N', text_len: 32, end: usize::MAX, fresh: false }
    }
}

/// The database written into the image in the format chosen; format C is written against a format C database already
/// in the image unless `fresh`.
pub fn write(image: &mut Image, records: &[Record], o: Options) -> Result<Written, IddbError> {
    if o.format == b'C' {
        Ok(crate::iddb_compact::write(image, records, o.end, o.fresh))
    } else {
        write_n(image, records, o.text_len, o.end)
    }
}

/// The database written again in another format: the records read back, then written as `format` (N: each text keeps
/// the length the longest one needs, so none is cut). `None` when the image holds no database, holds it in that format
/// already, or in one `read` can't give back (format n, other programs' formats): those are left as they are.
pub fn convert(image: &mut Image, format: u8, end: usize) -> Option<Written> {
    let h = read_header(image.bytes())?;
    if h.format == format || (h.format != b'N' && h.format != b'C') {
        return None;
    }
    let records = read(image.bytes());
    let longest = records.iter().map(|r| r.text.chars().count()).max().unwrap_or(0);
    let text_len = TEXT_LENGTHS.iter().copied().find(|&n| n >= longest).unwrap_or(TEXT_MAX);
    write(image, &records, Options { format, text_len, end, fresh: true }).ok()
}

/// A database header this editor can't parse: the tag is there, but not a format, record size or count `read_header`
/// takes. It can't be read or rebuilt, only removed (RadioFoundry's web editor offers the same).
pub fn unreadable(b: &[u8]) -> bool {
    b.get(AREA1..AREA1 + 2) == Some(&[0x49, 0x64]) && read_header(b).is_none()
}

/// How many bytes the database reaches past `end` (the radio's image end), or `None` when it fits or there is none.
pub fn overflow(b: &[u8], end: usize) -> Option<usize> {
    let top = extent(b);
    (top > end).then(|| top - end)
}

/// The database cut to the records that fit before `end`, in its own format, each text as long as the longest needs
/// (as `convert` does). `None` when there is no database, it fits already, or `read` can't give its records back
/// (format n, other programs' formats): those can only be removed.
pub fn fit(image: &mut Image, end: usize) -> Option<Written> {
    let h = read_header(image.bytes())?;
    if (h.format != b'N' && h.format != b'C') || extent(image.bytes()) <= end {
        return None;
    }
    let records = read(image.bytes());
    let longest = records.iter().map(|r| r.text.chars().count()).max().unwrap_or(0);
    let text_len = TEXT_LENGTHS.iter().copied().find(|&n| n >= longest).unwrap_or(TEXT_MAX);
    write(image, &records, Options { format: h.format, text_len, end, fresh: true }).ok()
}

/// The image address a write of the database must reach: its extent, or just the header when there is none (an erased
/// database).
pub fn end(b: &[u8]) -> usize {
    match extent(b) {
        0 => AREA1 + 0x20,
        e => e,
    }
}

/// The database's bytes in an image: the header and records in area 1, the rest in area 2.
pub fn ranges(b: &[u8]) -> Vec<(usize, usize)> {
    let top = end(b);
    let mut v = vec![(AREA1, top.min(AREA1 + AREA1_SIZE))];
    if top > AREA2 {
        v.push((AREA2, top));
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(id: u32, text: &str) -> Record {
        Record { id, text: text.into() }
    }

    #[test]
    fn clean_keeps_the_alphabet_and_drops_the_rest() {
        assert_eq!(clean("José  Müller-Łukasz"), "Jose Muller Lukasz");
        assert_eq!(clean("  K6ABC, Ann (CA) "), "K6ABC Ann CA");
        assert_eq!(clean("Æsir ßtraße"), "AEsir sstrasse");
        assert_eq!(clean("a.b"), "a.b");
    }

    #[test]
    fn record_sizes() {
        assert_eq!(record_size(16), 15);
        assert_eq!(record_size(32), 27);
        assert_eq!(record_size(48), 39);
    }

    #[test]
    fn written_records_read_back_sorted_with_the_first_duplicate_kept() {
        let mut img = Image::blank();
        let list = [rec(3100123, "N0CALL Ann"), rec(1, "First"), rec(3100123, "Second"), rec(0, "Zero"), rec(0x100_0000, "Big")];
        let w = write_n(&mut img, &list, 32, usize::MAX).unwrap();
        assert_eq!((w.count, w.dropped, w.rec), (2, 0, 27));
        let h = read_header(img.bytes()).unwrap();
        assert_eq!((h.format, h.rec, h.count, h.text_len), (b'N', 27, 2, 32));
        assert_eq!(read(img.bytes()), vec![rec(1, "First"), rec(3100123, "N0CALL Ann")]);
        assert_eq!(extent(img.bytes()), AREA1 + HEADER + 2 * 27);
        erase(&mut img);
        assert_eq!(read_header(img.bytes()), None);
    }

    #[test]
    fn records_continue_in_area_2_and_the_excess_is_dropped() {
        let mut img = Image::blank();
        let per1 = (AREA1_SIZE - HEADER) / 15;
        let list: Vec<Record> = (1..=per1 as u32 + 5).map(|i| rec(i, "A")).collect();
        let w = write_n(&mut img, &list, 16, usize::MAX).unwrap();
        assert_eq!((w.count, w.dropped), (per1 + 5, 0));
        assert_eq!(read(img.bytes()).len(), per1 + 5);
        assert_eq!(read(img.bytes())[per1].id, per1 as u32 + 1);
        // room for 2 records past area 1 only
        let mut img = Image::blank();
        let w = write_n(&mut img, &list, 16, AREA2 + 2 * 15).unwrap();
        assert_eq!((w.count, w.dropped), (per1 + 2, 3));
    }

    #[test]
    fn a_header_that_cannot_be_parsed_is_unreadable_and_erase_clears_it() {
        let mut img = Image::blank();
        assert!(!unreadable(img.bytes()), "no database");
        // the tag, a format letter, a record size below the minimum, a count
        img.bytes_mut()[AREA1..AREA1 + 12].copy_from_slice(&[0x49, 0x64, b'N', 0x30, 0x30, 0x30, 0x31, 0, 5, 0, 0, 0]);
        assert!(read_header(img.bytes()).is_none() && unreadable(img.bytes()));
        erase(&mut img);
        assert!(!unreadable(img.bytes()));
        write_n(&mut img, &[rec(1, "A")], 16, usize::MAX).unwrap();
        assert!(!unreadable(img.bytes()), "a valid database");
    }

    #[test]
    fn a_database_past_the_radios_end_is_cut_to_fit() {
        let per1 = (AREA1_SIZE - HEADER) / 15;
        let list: Vec<Record> = (1..=per1 as u32 + 100).map(|i| rec(i, "Ann")).collect();
        let mut img = Image::blank();
        write_n(&mut img, &list, 16, usize::MAX).unwrap();
        let end = AREA2 + 40 * 15;
        assert_eq!(overflow(img.bytes(), end), Some(60 * 15));
        assert_eq!(overflow(img.bytes(), usize::MAX), None);
        let w = fit(&mut img, end).unwrap();
        assert_eq!((w.count, w.dropped), (per1 + 40, 60));
        assert!(extent(img.bytes()) <= end && overflow(img.bytes(), end).is_none());
        let back = read(img.bytes());
        assert_eq!((back.len(), back[0].id, back[per1 + 39].id), (per1 + 40, 1, per1 as u32 + 40));
        assert_eq!(back[0].text, "Ann");
        // it fits now: nothing more to cut; no database: nothing to cut
        assert!(fit(&mut img, end).is_none());
        assert!(fit(&mut Image::blank(), 0).is_none());
    }

    #[test]
    fn a_compact_database_past_the_end_is_cut_in_its_own_format() {
        let list: Vec<Record> = (1..=90_000u32).map(|i| rec(i, &format!("Ann Smith {i}"))).collect();
        let mut img = Image::blank();
        write(&mut img, &list, Options { format: b'C', text_len: 32, end: usize::MAX, fresh: true }).unwrap();
        let top = extent(img.bytes());
        // area 1 always holds 256 KB; only what runs on into area 2 can pass the radio's end
        assert!(top > AREA2, "{top:#x}");
        let end = AREA2 + (top - AREA2) / 2;
        assert!(overflow(img.bytes(), end).is_some());
        let w = fit(&mut img, end).unwrap();
        assert!(w.count > 0 && w.count < 90_000, "{w:?} top {top:#x} end {end:#x}");
        assert_eq!(read_header(img.bytes()).unwrap().format, b'C');
        assert!(extent(img.bytes()) <= end);
    }

    #[test]
    fn a_bad_text_length_is_refused() {
        let mut img = Image::blank();
        assert_eq!(write_n(&mut img, &[], 20, usize::MAX), Err(IddbError::TextLength(20)));
    }

    #[test]
    fn a_header_that_is_not_a_database_reads_as_none() {
        assert_eq!(read_header(Image::blank().bytes()), None);
        let mut img = Image::blank();
        img.bytes_mut()[AREA1..AREA1 + 12].copy_from_slice(b"ID\x4E\x59001\0\x01\0\0\0");
        assert_eq!(read_header(img.bytes()), None, "the old ID tag is rejected");
    }
}
