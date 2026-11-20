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
