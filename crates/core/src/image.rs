// SPDX-License-Identifier: MIT
// Copyright (c) 2026 The RadioFoundry Authors
// Copyright (c) 2026 insideTheFlux
//
// The codeplug image: one flat byte array, file offset = image address (open-firmware-cps.md 7). Ported from
// RadioFoundry's `site/static/js/codeplug/image.js`.

/// The "EEPROM" image starts here.
pub const EE: usize = 0x00000;
/// The "flash" image (contacts, lists, channels 129 and up) starts here.
pub const FL: usize = 0x20000;
/// Bytes a whole codeplug image covers.
pub const CODEPLUG_SPAN: usize = 0x120000;

/// The ranges qdmr transfers (open-firmware-cps.md 7.1): what a read from a radio needs for every record the editor
/// shows. Calibration and the radio's own settings (0x10000-0x1FFFF) are never part of it.
pub const REGIONS: [(usize, usize); 4] = [
    (EE + 0x00080, EE + 0x06060),
    (EE + 0x07500, EE + 0x0B000),
    (FL, FL + 0x011A0),
    (FL + 0x7B000, FL + 0x8EE60),
];
/// The codeplug settings area (codeplug-settings.md): one sector qdmr leaves alone.
pub const SETTINGS_AREA: (usize, usize) = (EE + 0x0E000, EE + 0x0F000);
/// The size of a block the radio's fast protocol moves.
pub const BLOCK: usize = 0x1000;

/// The 4 KB blocks a read from a radio needs: the regions above and the settings area, in address order.
pub fn editor_blocks() -> Vec<usize> {
    let mut v: Vec<usize> = Vec::new();
    for &(a, b) in REGIONS.iter().chain(std::iter::once(&SETTINGS_AREA)) {
        let mut s = a / BLOCK * BLOCK;
        while s < b {
            v.push(s);
            s += BLOCK;
        }
    }
    v.sort_unstable();
    v.dedup();
    v
}

#[derive(Clone, Debug)]
pub struct Image {
    bytes: Vec<u8>,
}

impl Image {
    /// From a file or a read: anything shorter than the span is padded with 0xFF (erased flash).
    pub fn from_bytes(bytes: &[u8]) -> Self {
        let mut b = vec![0xFF; bytes.len().max(CODEPLUG_SPAN)];
        b[..bytes.len()].copy_from_slice(bytes);
        Self { bytes: b }
    }

    /// An empty codeplug: erased flash with the in-use bitmaps, the zone table and the list counts cleared, so
    /// nothing reads as in use, and the zone area zeroed so the firmware sees the 80-channel layout.
    pub fn blank() -> Self {
        let mut b = vec![0xFFu8; CODEPLUG_SPAN];
        b[EE + 0x3780..EE + 0x3790].fill(0);
        for bank in 1..8 {
            let a = FL + 0x7B1B0 + (bank - 1) * 0x1C10;
            b[a..a + 16].fill(0);
        }
        b[EE + 0x8010..EE + 0x8030 + 68 * 0xB0].fill(0);
        b[FL + 0x8D620..FL + 0x8D620 + 76].fill(0);
        Self { bytes: b }
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn bytes_mut(&mut self) -> &mut [u8] {
        &mut self.bytes
    }

    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    pub fn view(&self, addr: usize, len: usize) -> &[u8] {
        &self.bytes[addr..addr + len]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_editor_reads_these_blocks() {
        let b = editor_blocks();
        assert_eq!(b.first(), Some(&0));
        assert!(b.contains(&0xE000) && b.contains(&0x20000) && b.contains(&(FL + 0x7B000)));
        assert!(b.windows(2).all(|w| w[0] < w[1] && w[0] % BLOCK == 0));
        assert!(b.iter().all(|&a| a + BLOCK <= CODEPLUG_SPAN));
    }
}
