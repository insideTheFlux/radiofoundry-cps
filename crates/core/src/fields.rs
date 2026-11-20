// SPDX-License-Identifier: MIT
// Copyright (c) 2026 The RadioFoundry Authors
// Copyright (c) 2026 insideTheFlux
//
// Field encodings of the open-firmware codeplug (open-firmware-cps.md 7.3): BCD frequencies, DMR IDs and tones,
// 0xFF-padded names, 24-bit locations. Ported from RadioFoundry's `site/static/js/codeplug/fields.js`.

/// Packed BCD, most significant digit first in the word; `None` if a nibble is not 0-9.
pub fn bcd(word: u32, digits: u32) -> Option<u32> {
    let mut v: u32 = 0;
    for i in (0..digits).rev() {
        let d = (word >> (4 * i)) & 0xF;
        if d > 9 {
            return None;
        }
        v = v * 10 + d;
    }
    Some(v)
}

pub fn u16le(b: &[u8], off: usize) -> u16 {
    u16::from(b[off]) | (u16::from(b[off + 1]) << 8)
}

/// Frequency in Hz: BCD of 10 Hz units as a little-endian word (438.500 MHz = bytes 00 00 85 43); `None` if
/// malformed or 0.
pub fn get_freq(b: &[u8], off: usize) -> Option<u64> {
    let w = u32::from_le_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]]);
    match bcd(w, 8) {
        Some(v) if v != 0 => Some(u64::from(v) * 10),
        _ => None,
    }
}

/// DMR ID or talkgroup: 8 BCD digits, bytes in reading order; 0 if malformed or above 24 bits.
pub fn get_bcd_id(b: &[u8], off: usize) -> u32 {
    let w = u32::from_be_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]]);
    match bcd(w, 8) {
        Some(v) if v <= 0xFF_FFFF => v,
        _ => 0,
    }
}

/// A tone as text: "" none, "88.5" CTCSS, "D023N" / "D023I" DCS normal or inverted. Malformed words read as none.
/// The DCS code's three octal digits are three BCD digits, like the CTCSS word.
pub fn get_tone(b: &[u8], off: usize) -> String {
    let v = u16le(b, off);
    if v == 0 || v == 0xFFFF {
        return String::new();
    }
    if v & 0x8000 != 0 {
        let mut code = String::new();
        for i in (0..3).rev() {
            let d = (v >> (4 * i)) & 0xF;
            if d > 7 {
                return String::new();
            }
            code.push(char::from(b'0' + d as u8));
        }
        return format!("D{}{}", code, if v & 0x4000 != 0 { 'I' } else { 'N' });
    }
    if v & 0x4000 != 0 {
        return String::new();
    }
    match bcd(u32::from(v), 4) {
        Some(tenths) if (300..=3000).contains(&tenths) => {
            format!("{:.1}", f64::from(tenths) / 10.0)
        }
        _ => String::new(),
    }
}

/// A name field: text up to the first 0x00 or 0xFF. Bytes are Latin-1, as the radio's font takes them.
pub fn get_name(b: &[u8], off: usize, len: usize) -> String {
    b[off..off + len]
        .iter()
        .take_while(|&&c| c != 0x00 && c != 0xFF)
        .map(|&c| char::from(c))
        .collect()
}

/// 24-bit coordinate: bit 23 sign (south / west), bits 22-15 whole degrees, bits 14-0 1/10000 degree.
pub fn get_coord(hi: u8, mid: u8, lo: u8) -> f64 {
    let v = (u32::from(hi) << 16) | (u32::from(mid) << 8) | u32::from(lo);
    let deg = (v >> 15) & 0xFF;
    let frac = (v & 0x7FFF).min(9999);
    let x = f64::from(deg) + f64::from(frac) / 10000.0;
    if v & 0x80_0000 != 0 {
        -x
    } else {
        x
    }
}

// --- Writing ---

/// The digits of `v` as packed BCD, least significant digit in the low nibble.
pub fn to_bcd(mut v: u32, digits: u32) -> u32 {
    let mut w = 0;
    for i in 0..digits {
        w |= (v % 10) << (4 * i);
        v /= 10;
    }
    w
}

pub fn put16(b: &mut [u8], off: usize, v: u16) {
    b[off..off + 2].copy_from_slice(&v.to_le_bytes());
}

pub fn put_freq(b: &mut [u8], off: usize, hz: u64) {
    let w = to_bcd(((hz + 5) / 10) as u32, 8);
    b[off..off + 4].copy_from_slice(&w.to_le_bytes());
}

pub fn put_bcd_id(b: &mut [u8], off: usize, id: u32) {
    b[off..off + 4].copy_from_slice(&to_bcd(id, 8).to_be_bytes());
}

/// The text, then 0xFF padding. Characters outside Latin-1 (and 0xFF itself) become '?'.
pub fn put_name(b: &mut [u8], off: usize, len: usize, text: &str) {
    b[off..off + len].fill(0xFF);
    for (i, ch) in text.chars().take(len).enumerate() {
        let c = u32::from(ch);
        b[off + i] = if c < 0x100 && c != 0xFF {
            c as u8
        } else {
            b'?'
        };
    }
}

/// A tone's normalized text, or `None` if it can't be a tone. Accepts "", "none", "88.5", "D23", "D023", "D023N",
/// "D023I".
pub fn parse_tone(text: &str) -> Option<String> {
    let s = text.trim().to_uppercase();
    if s.is_empty() || s == "NONE" || s == "OFF" {
        return Some(String::new());
    }
    if let Some(rest) = s
        .strip_prefix('D')
        .or(Some(s.as_str()))
        .filter(|r| r.chars().next().is_some_and(|c| c.is_ascii_digit()))
    {
        // "D023", "D023N", "023N" (the D may be left off when the polarity is given)
        let (digits, pol) = match rest.chars().last() {
            Some(c @ ('N' | 'I')) => (&rest[..rest.len() - 1], Some(c)),
            _ => (rest, None),
        };
        let octal = !digits.is_empty()
            && digits.len() <= 3
            && digits.chars().all(|c| ('0'..='7').contains(&c));
        if octal && (s.starts_with('D') || pol.is_some()) {
            return Some(format!("D{:0>3}{}", digits, pol.unwrap_or('N')));
        }
    }
    // CTCSS: 2-3 digits, optionally one decimal
    let (whole, frac) = match s.split_once('.') {
        Some((w, f)) => (w, Some(f)),
        None => (s.as_str(), None),
    };
    if (2..=3).contains(&whole.len()) && whole.chars().all(|c| c.is_ascii_digit()) {
        let f = match frac {
            None => 0,
            Some(f) if f.len() == 1 && f.chars().all(|c| c.is_ascii_digit()) => {
                f.parse::<u32>().ok()?
            }
            _ => return None,
        };
        let tenths = whole.parse::<u32>().ok()? * 10 + f;
        if (300..=3000).contains(&tenths) {
            return Some(format!("{:.1}", f64::from(tenths) / 10.0));
        }
    }
    None
}

/// Writes a tone word: 0xFFFF for none (or text that is no tone).
pub fn put_tone(b: &mut [u8], off: usize, text: &str) {
    let t = parse_tone(text).unwrap_or_default();
    let mut w: u16 = 0xFFFF;
    if let Some(code) = t.strip_prefix('D') {
        let d: Vec<u16> = code
            .chars()
            .take(3)
            .map(|c| c.to_digit(8).unwrap_or(0) as u16)
            .collect();
        w = 0x8000
            | if code.ends_with('I') { 0x4000 } else { 0 }
            | (d[0] << 8)
            | (d[1] << 4)
            | d[2];
    } else if !t.is_empty() {
        let tenths = (t.parse::<f64>().unwrap_or(0.0) * 10.0).round() as u32;
        w = to_bcd(tenths, 4) as u16;
    }
    put16(b, off, w);
}

/// The three bytes of a 24-bit coordinate (high, middle, low), the inverse of [`get_coord`].
pub fn coord_bytes(x: f64) -> [u8; 3] {
    let a = x.abs().min(255.9999);
    let mut deg = a.floor() as u32;
    let mut frac = ((a - f64::from(deg)) * 10000.0).round() as u32;
    if frac > 9999 {
        deg += 1;
        frac = 0;
    }
    let v = (if x < 0.0 { 0x80_0000 } else { 0 }) | (deg << 15) | frac;
    [(v >> 16) as u8, (v >> 8) as u8, v as u8]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frequency_is_bcd_of_10_hz() {
        assert_eq!(get_freq(&[0x00, 0x00, 0x85, 0x43], 0), Some(438_500_000));
        assert_eq!(get_freq(&[0, 0, 0, 0], 0), None);
        assert_eq!(get_freq(&[0xFF, 0xFF, 0xFF, 0xFF], 0), None);
    }

    #[test]
    fn ctcss_and_dcs() {
        let t = |v: u16| get_tone(&v.to_le_bytes(), 0);
        assert_eq!(t(0xFFFF), "");
        assert_eq!(t(0x1035), "103.5");
        assert_eq!(t(0x0670), "67.0");
        // DCS: bit 15, three BCD digits; bit 14 inverted
        assert_eq!(t(0x8073), "D073N");
        assert_eq!(t(0x8265), "D265N");
        assert_eq!(t(0xC023), "D023I");
        assert_eq!(t(0x8058), ""); // a digit above 7 is not a code
    }

    #[test]
    fn names_stop_at_padding() {
        let b = [b'A', b'b', 0xFF, b'x'];
        assert_eq!(get_name(&b, 0, 4), "Ab");
        assert_eq!(get_name(&[0xE9, 0x00], 0, 2), "\u{e9}");
    }

    #[test]
    fn dmr_id_is_bcd_in_reading_order() {
        assert_eq!(get_bcd_id(&[0x09, 0x99, 0x23, 0x45], 0), 9_992_345);
        assert_eq!(get_bcd_id(&[0xFF, 0xFF, 0xFF, 0xFF], 0), 0);
    }

    #[test]
    fn tones_round_trip_through_text() {
        for text in [
            "", "88.5", "103.5", "254.1", "D023N", "D073I", "D265N", "D754I",
        ] {
            let mut b = [0u8; 2];
            put_tone(&mut b, 0, text);
            assert_eq!(get_tone(&b, 0), text, "{text}");
        }
        assert_eq!(parse_tone("d23").as_deref(), Some("D023N"));
        assert_eq!(parse_tone("023i").as_deref(), Some("D023I"));
        assert_eq!(parse_tone(" 100 ").as_deref(), Some("100.0"));
        assert_eq!(parse_tone("none").as_deref(), Some(""));
        assert_eq!(parse_tone("D889"), None);
        assert_eq!(parse_tone("12.0"), None);
        assert_eq!(parse_tone("hello"), None);
    }

    #[test]
    fn frequency_ids_names_and_coordinates_round_trip() {
        let mut b = [0u8; 16];
        put_freq(&mut b, 0, 446_760_000);
        assert_eq!(get_freq(&b, 0), Some(446_760_000));
        put_bcd_id(&mut b, 4, 3_100_123);
        assert_eq!(get_bcd_id(&b, 4), 3_100_123);
        put_name(&mut b, 8, 8, "Caf\u{e9} \u{20ac}");
        assert_eq!(get_name(&b, 8, 8), "Caf\u{e9} ?");
        for x in [0.0, 33.6777, -117.1693, 40.0001, -0.5] {
            let [h, m, l] = coord_bytes(x);
            assert!((get_coord(h, m, l) - x).abs() < 0.00005, "{x}");
        }
    }
}
