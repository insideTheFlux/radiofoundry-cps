// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux

//! The fast codeplug protocol (RadioFoundry `docs/spec/cps-fast.md`): the debug link's CODEPLUG command, 4 KB blocks,
//! CRC-32 sums, LZ4 streams. Reading only for now.

use crate::radio::{LinkError, Radio, Result};
use rfcps_core::image::{editor_blocks, Image, BLOCK, CODEPLUG_SPAN};
use std::io::{Read, Write};

const CMD_CODEPLUG: u8 = 0x1C;
const OP_INFO: u8 = 0;
const OP_SUMS: u8 = 1;
const OP_READ: u8 = 2;
const FORMAT_LZ4: u8 = 1;
const FORMAT_BLANK: u8 = 2;

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
        let blocks = editor_blocks();
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
