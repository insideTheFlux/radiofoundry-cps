// SPDX-License-Identifier: MIT
// Copyright (c) 2026 The RadioFoundry Authors
// Copyright (c) 2026 insideTheFlux

//! The fast codeplug protocol (RadioFoundry `docs/spec/cps-fast.md`): the debug link's CODEPLUG command, 4 KB blocks,
//! CRC-32 sums, LZ4 streams. Reading and writing; INFO is checked against the editor's layout first.

use crate::radio::{LinkError, Radio, Result};
use rfcps_core::image::{editor_blocks, read_blocks, Image, BLOCK, CODEPLUG_SPAN, FL};
use std::io::{Read, Write};

const CMD_CODEPLUG: u8 = 0x1C;
const OP_INFO: u8 = 0;
const OP_SUMS: u8 = 1;
const OP_READ: u8 = 2;
const FORMAT_LZ4: u8 = 1;
const FORMAT_BLANK: u8 = 2;

/// The EEPROM image's size (cps-fast.md 3.1); the gap up to `FL` is never served.
const EEPROM_IMAGE: u32 = 0x10000;

/// The CRC-32 of a block of 0xFF.
pub const BLANK_CRC: u32 = 0xF154_670A;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Info {
    pub version: u8,
    pub staged_writes: bool,
    pub flash_writable: bool,
    pub block: u16,
    pub sums_per_request: u8,
    pub data_per_write: u8,
    pub stream_per_read: u16,
    pub eeprom_size: u32,
    pub flash_start: u32,
    pub image_end: u32,
    pub eeprom_base: u32,
    pub stage_max: u16,
}

impl Info {
    /// Does the radio's image have the layout this editor assumes (the EEPROM image at 0, the flash image at `FL`, both
    /// inside the radio's image, 4 KB blocks, protocol version 1)? An editor that read or wrote a radio with another
    /// layout would take the wrong blocks, so every read and write checks this first.
    pub fn check_layout(&self) -> Result<()> {
        let end = editor_blocks().into_iter().max().map_or(0, |a| a + BLOCK) as u32;
        let bad = |what: String| Err(LinkError::Layout(what));
        if self.version != 1 {
            return bad(format!(
                "fast protocol version {} (this editor speaks 1)",
                self.version
            ));
        }
        if usize::from(self.block) != BLOCK {
            return bad(format!(
                "blocks of {} bytes (this editor uses {BLOCK})",
                self.block
            ));
        }
        if self.flash_start != FL as u32 {
            return bad(format!(
                "the flash image starts at {:#x} (this editor expects {:#x})",
                self.flash_start, FL
            ));
        }
        if self.eeprom_size != EEPROM_IMAGE {
            return bad(format!(
                "an EEPROM image of {:#x} bytes (this editor expects {:#x})",
                self.eeprom_size, EEPROM_IMAGE
            ));
        }
        if self.image_end < end {
            return bad(format!(
                "the image ends at {:#x}, before {end:#x} where the editor's codeplug ends",
                self.image_end
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sums {
    pub blank_mask: u16,
    pub crcs: Vec<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Block {
    Blank,
    Data(Vec<u8>),
}

fn le32(v: u32) -> [u8; 4] {
    v.to_le_bytes()
}

fn rd16(d: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([d[o], d[o + 1]])
}

fn rd32(d: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]])
}

impl<P: Read + Write> Radio<P> {
    fn codeplug(&mut self, op: u8, args: &[u8]) -> Result<Vec<u8>> {
        let mut p = vec![op];
        p.extend_from_slice(args);
        self.request(CMD_CODEPLUG, &p)
    }

    pub fn codeplug_info(&mut self) -> Result<Info> {
        let d = self.codeplug(OP_INFO, &[])?;
        if d.len() < 26 {
            return Err(LinkError::Protocol(format!("INFO of {} bytes", d.len())));
        }
        Ok(Info {
            version: d[0],
            staged_writes: d[1] & 1 != 0,
            flash_writable: d[1] & 2 != 0,
            block: rd16(&d, 2),
            sums_per_request: d[4],
            data_per_write: d[5],
            stream_per_read: rd16(&d, 6),
            eeprom_size: rd32(&d, 8),
            flash_start: rd32(&d, 12),
            image_end: rd32(&d, 16),
            eeprom_base: rd32(&d, 20),
            stage_max: rd16(&d, 24),
        })
    }

    /// `codeplug_info`, refused unless the radio's image layout is the one this editor knows.
    pub fn checked_info(&mut self) -> Result<Info> {
        let info = self.codeplug_info()?;
        info.check_layout()?;
        Ok(info)
    }

    /// The CRC-32 of up to 16 consecutive blocks from `addr`, and which of them are blank.
    pub fn codeplug_sums(&mut self, addr: u32, count: u8) -> Result<Sums> {
        let mut a = le32(addr).to_vec();
        a.push(count);
        let d = self.codeplug(OP_SUMS, &a)?;
        let n = usize::from(count);
        if d.len() < 7 + 4 * n {
            return Err(LinkError::Protocol(format!("SUMS of {} bytes", d.len())));
        }
        Ok(Sums {
            blank_mask: rd16(&d, 5),
            crcs: (0..n).map(|i| rd32(&d, 7 + 4 * i)).collect(),
        })
    }

    /// One 4 KB block: its LZ4 stream read in pieces, decoded, and checked against the radio's CRC-32. A block that
    /// changes while it is read, or a stream that does not check, is read again (three tries).
    pub fn codeplug_read_block(&mut self, addr: u32) -> Result<Block> {
        let mut last = LinkError::Protocol("no attempt".into());
        for _ in 0..3 {
            match self.read_block_once(addr) {
                Ok(b) => return Ok(b),
                Err(e @ LinkError::Protocol(_)) => last = e,
                Err(e) => return Err(e),
            }
        }
        Err(last)
    }

    fn read_block_once(&mut self, addr: u32) -> Result<Block> {
        let mut stream: Vec<u8> = Vec::new();
        let mut head: Option<(u8, u16, u32)> = None;
        loop {
            let mut a = le32(addr).to_vec();
            a.extend_from_slice(&(stream.len() as u16).to_le_bytes());
            let d = self.codeplug(OP_READ, &a)?;
            if d.len() < 14 || usize::from(rd16(&d, 11)) != stream.len() {
                return Err(LinkError::Protocol(
                    "a READ reply that is short or at another offset".into(),
                ));
            }
            let (format, length, crc) = (d[4], rd16(&d, 5), rd32(&d, 7));
            match head {
                None => head = Some((format, length, crc)),
                Some(h) if h != (format, length, crc) => {
                    return Err(LinkError::Protocol(
                        "the block changed while it was read".into(),
                    ))
                }
                _ => {}
            }
            if format == FORMAT_BLANK {
                return if crc == BLANK_CRC {
                    Ok(Block::Blank)
                } else {
                    Err(LinkError::Protocol("a blank block with another CRC".into()))
                };
            }
            if format != FORMAT_LZ4 {
                return Err(LinkError::Protocol(format!("block format {format}")));
            }
            let piece = &d[14..];
            if piece.is_empty() && stream.len() < usize::from(length) {
                return Err(LinkError::Protocol("an empty piece of a stream".into()));
            }
            stream.extend_from_slice(piece);
            if stream.len() >= usize::from(length) {
                stream.truncate(usize::from(length));
                break;
            }
        }
        let (_, _, crc) = head.unwrap();
        let data = lz4_flex::block::decompress(&stream, BLOCK)
            .map_err(|e| LinkError::Protocol(format!("LZ4: {e}")))?;
        if data.len() != BLOCK || crc32fast::hash(&data) != crc {
            return Err(LinkError::Protocol(
                "a block whose CRC-32 does not match".into(),
            ));
        }
        Ok(Block::Data(data))
    }

    /// Reads what the editor needs into an image (blank blocks as 0xFF). `progress(done, total)` is called per block.
    pub fn read_image(&mut self, progress: &mut dyn FnMut(usize, usize)) -> Result<Image> {
        self.checked_info()?;
        let blocks = read_blocks();
        let mut img = Image::from_bytes(&[]);
        debug_assert_eq!(img.len(), CODEPLUG_SPAN);
        for (i, &addr) in blocks.iter().enumerate() {
            progress(i, blocks.len());
            match self.codeplug_read_block(addr as u32)? {
                Block::Blank => img.bytes_mut()[addr..addr + BLOCK].fill(0xFF),
                Block::Data(d) => img.bytes_mut()[addr..addr + BLOCK].copy_from_slice(&d),
            }
        }
        progress(blocks.len(), blocks.len());
        Ok(img)
    }
}

// --- Writing ---

const OP_BEGIN: u8 = 3;
const OP_BLOCK: u8 = 4;
const OP_DATA: u8 = 5;
const OP_END: u8 = 6;
const OP_COMMIT: u8 = 7;
const OP_STATUS: u8 = 8;
const OP_ABORT: u8 = 9;

pub const STATE_COMMITTING: u8 = 3;
pub const STATE_DONE: u8 = 4;

/// Refusal reasons a resend can't fix: a block outside the image, storage that can't be written, no staging area,
/// a full or busy staging area, a flash failure (cps-fast.md 6).
const FINAL_REASONS: [u8; 6] = [1, 2, 3, 4, 5, 9];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Status {
    pub state: u8,
    pub mode: u8,
    pub blocks: u16,
    pub copied: u16,
    pub reason: u8,
}

impl<P: Read + Write> Radio<P> {
    /// Opens a write session: direct (each block written as it ends) or staged (all or none at COMMIT). Returns the
    /// staging area's block count (0 for direct).
    pub fn codeplug_begin(&mut self, staged: bool) -> Result<u16> {
        let d = self.codeplug(OP_BEGIN, &[u8::from(staged)])?;
        if d.len() < 2 {
            return Err(LinkError::Protocol("BEGIN reply too short".into()));
        }
        Ok(rd16(&d, 0))
    }

    /// One block: BLOCK, the LZ4 stream in DATA pieces, END. A block that fails on the way (no answer, a corrupt
    /// piece) is sent again from its start, up to three times; a refusal a resend can't fix is returned at once.
    /// Returns the stream bytes sent.
    pub fn codeplug_write_block(
        &mut self,
        addr: u32,
        data: &[u8],
        data_per_write: usize,
    ) -> Result<usize> {
        if data.len() != BLOCK {
            return Err(LinkError::Protocol("a block is 4096 bytes".into()));
        }
        let blank = data.iter().all(|&b| b == 0xFF);
        let stream = if blank {
            Vec::new()
        } else {
            lz4_flex::block::compress(data)
        };
        let crc = crc32fast::hash(data);
        let mut last = LinkError::Protocol("no attempt".into());
        for _ in 0..3 {
            match self.send_block(addr, blank, &stream, crc, data_per_write) {
                Ok(()) => return Ok(stream.len()),
                Err(LinkError::Status {
                    status: 7,
                    reason: Some(r),
                }) if FINAL_REASONS.contains(&r) => {
                    return Err(LinkError::Status {
                        status: 7,
                        reason: Some(r),
                    })
                }
                Err(e) => last = e,
            }
        }
        Err(last)
    }

    fn send_block(
        &mut self,
        addr: u32,
        blank: bool,
        stream: &[u8],
        crc: u32,
        per: usize,
    ) -> Result<()> {
        let mut h = le32(addr).to_vec();
        h.push(if blank { FORMAT_BLANK } else { FORMAT_LZ4 });
        h.extend_from_slice(&(stream.len() as u16).to_le_bytes());
        h.extend_from_slice(&le32(crc));
        self.codeplug(OP_BLOCK, &h)?;
        for (i, piece) in stream.chunks(per).enumerate() {
            let mut d = ((i * per) as u16).to_le_bytes().to_vec();
            d.extend_from_slice(piece);
            self.codeplug(OP_DATA, &d)?;
        }
        // END writes the flash: allow it more time than a read
        let keep = self.timeout;
        self.timeout = std::time::Duration::from_secs(5);
        let r = self.codeplug(OP_END, &[]);
        self.timeout = keep;
        r.map(|_| ())
    }

    pub fn codeplug_status(&mut self) -> Result<Status> {
        let d = self.codeplug(OP_STATUS, &[])?;
        if d.len() < 7 {
            return Err(LinkError::Protocol("STATUS reply too short".into()));
        }
        Ok(Status {
            state: d[0],
            mode: d[1],
            blocks: rd16(&d, 2),
            copied: rd16(&d, 4),
            reason: d[6],
        })
    }

    /// Ends the session. A staged session copies its blocks in now; this waits until it is done.
    pub fn codeplug_commit(&mut self, wait: std::time::Duration) -> Result<()> {
        self.codeplug(OP_COMMIT, &[])?;
        let end = std::time::Instant::now() + wait;
        loop {
            let s = self.codeplug_status()?;
            if s.state == STATE_DONE {
                return Ok(());
            }
            if s.state != STATE_COMMITTING || std::time::Instant::now() > end {
                return Err(LinkError::Protocol(format!(
                    "the commit did not finish (state {}, reason {})",
                    s.state, s.reason
                )));
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }

    /// Ends the session without committing.
    pub fn codeplug_abort(&mut self) -> Result<()> {
        self.codeplug(OP_ABORT, &[]).map(|_| ())
    }
}
