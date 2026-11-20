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
}
