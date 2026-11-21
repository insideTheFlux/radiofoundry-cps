// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux

//! A software radio: speaks the debug link and the fast codeplug protocol's read side over a byte stream, from an
//! image in memory. For tests and for trying the app without a radio.

use crate::fast::BLANK_CRC;
use crate::frame::{self, Parsed};
use rfcps_core::image::{editor_blocks, BLOCK};
use std::collections::VecDeque;
use std::io::{self, Read, Write};

struct Open {
    addr: usize,
    format: u8,
    crc: u32,
    stream: Vec<u8>,
}

struct Session {
    staged: bool,
    open: Option<Open>,
    staged_blocks: Vec<(usize, Vec<u8>)>,
    blocks: u16,
}

pub struct FakeRadio {
    image: Vec<u8>,
    session: Option<Session>,
    done: bool,
    rx: Vec<u8>,
    tx: VecDeque<u8>,
    /// Report a wrong CRC-32 on the next block read, once (the host should read the block again).
    pub corrupt_next_read: bool,
    /// Frames received, for tests.
    pub requests: usize,
    /// Offer staged writes (a staging area of this many blocks); 0 for none, as on a DM-32UV.
    pub stage_max: u16,
    /// Report this INFO layout instead (protocol version, EEPROM image size, flash image start, image end), to test an
    /// editor against a radio whose layout differs.
    pub info_layout: Option<(u8, u32, u32, u32)>,
    /// Refuse the next END with this reason, once (the host should send the block again).
    pub refuse_next_end: Option<u8>,
    /// The blocks written so far, in order.
    pub written_blocks: Vec<usize>,
}

impl FakeRadio {
    pub fn new(image: &[u8]) -> Self {
        Self {
            image: image.to_vec(),
            session: None,
            done: false,
            rx: Vec::new(),
            tx: VecDeque::new(),
            corrupt_next_read: false,
            requests: 0,
            stage_max: 0,
            info_layout: None,
            refuse_next_end: None,
            written_blocks: Vec::new(),
        }
    }

    /// The radio's image as it is now.
    pub fn image(&self) -> &[u8] {
        &self.image
    }

    /// Changes a byte of the radio's image, as a person using the radio would.
    pub fn poke(&mut self, addr: usize, v: u8) {
        self.image[addr] = v;
    }

    fn block(&self, addr: usize) -> Option<&[u8]> {
        (addr.is_multiple_of(BLOCK)
            && addr + BLOCK <= self.image.len()
            && editor_blocks().contains(&addr))
        .then(|| &self.image[addr..addr + BLOCK])
    }

    fn reply(&mut self, cmd: u8, seq: u8, payload: Vec<u8>) {
        self.tx.extend(frame::encode(cmd | 0x80, seq, &payload));
    }

    fn handle(&mut self, cmd: u8, seq: u8, p: &[u8]) {
        self.requests += 1;
        match (cmd, p.first()) {
            (0x01, _) => {
                let mut r = vec![0];
                r.extend_from_slice(p);
                self.reply(cmd, seq, r);
            }
            (0x1C, Some(0)) => {
                let (ver, ee, fl, end) =
                    self.info_layout
                        .unwrap_or((1, 0x10000, 0x20000, self.image.len() as u32));
                let mut r = vec![0, ver, 2 | u8::from(self.stage_max > 0)];
                r.extend_from_slice(&(BLOCK as u16).to_le_bytes());
                r.extend_from_slice(&[16, 61]);
                r.extend_from_slice(&1000u16.to_le_bytes());
                for v in [ee, fl, end, 0x700000] {
                    r.extend_from_slice(&v.to_le_bytes());
                }
                r.extend_from_slice(&self.stage_max.to_le_bytes());
                self.reply(cmd, seq, r);
            }
            (0x1C, Some(1)) if p.len() == 6 => {
                let addr = u32::from_le_bytes([p[1], p[2], p[3], p[4]]) as usize;
                let n = usize::from(p[5]);
                let blocks: Option<Vec<&[u8]>> =
                    (0..n).map(|i| self.block(addr + i * BLOCK)).collect();
                let Some(blocks) = blocks.filter(|_| (1..=16).contains(&n)) else {
                    return self.reply(cmd, seq, vec![7, 1]);
                };
                let mut mask = 0u16;
                let mut crcs = Vec::new();
                for (i, b) in blocks.iter().enumerate() {
                    if b.iter().all(|&x| x == 0xFF) {
                        mask |= 1 << i;
                    }
                    crcs.push(crc32fast::hash(b));
                }
                let mut r = vec![0];
                r.extend_from_slice(&(addr as u32).to_le_bytes());
                r.push(n as u8);
                r.extend_from_slice(&mask.to_le_bytes());
                for c in crcs {
                    r.extend_from_slice(&c.to_le_bytes());
                }
                self.reply(cmd, seq, r);
            }
            (0x1C, Some(2)) if p.len() == 7 => {
                let addr = u32::from_le_bytes([p[1], p[2], p[3], p[4]]) as usize;
                let offset = usize::from(u16::from_le_bytes([p[5], p[6]]));
                let Some(block) = self.block(addr) else {
                    return self.reply(cmd, seq, vec![7, 1]);
                };
                let mut r = vec![0];
                r.extend_from_slice(&(addr as u32).to_le_bytes());
                if block.iter().all(|&x| x == 0xFF) {
                    r.push(2);
                    r.extend_from_slice(&0u16.to_le_bytes());
                    r.extend_from_slice(&BLANK_CRC.to_le_bytes());
                    r.extend_from_slice(&0u16.to_le_bytes());
                    r.push(0);
                } else {
                    let stream = lz4_flex::block::compress(block);
                    if offset > stream.len() {
                        return self.reply(cmd, seq, vec![2]);
                    }
                    let mut crc = crc32fast::hash(block);
                    if self.corrupt_next_read {
                        self.corrupt_next_read = false;
                        crc ^= 1;
                    }
                    r.push(1);
                    r.extend_from_slice(&(stream.len() as u16).to_le_bytes());
                    r.extend_from_slice(&crc.to_le_bytes());
                    r.extend_from_slice(&(offset as u16).to_le_bytes());
                    r.push(0);
                    r.extend_from_slice(&stream[offset..stream.len().min(offset + 1000)]);
                }
                self.reply(cmd, seq, r);
            }
            (0x1C, Some(3)) if p.len() == 2 => {
                if self.session.is_some() {
                    return self.reply(cmd, seq, vec![7, 6]);
                }
                if p[1] == 1 && self.stage_max == 0 {
                    return self.reply(cmd, seq, vec![7, 3]);
                }
                self.session = Some(Session {
                    staged: p[1] == 1,
                    open: None,
                    staged_blocks: Vec::new(),
                    blocks: 0,
                });
                self.done = false;
                let mut r = vec![0];
                r.extend_from_slice(&(if p[1] == 1 { self.stage_max } else { 0 }).to_le_bytes());
                self.reply(cmd, seq, r);
            }
            (0x1C, Some(4)) if p.len() == 12 => {
                let addr = u32::from_le_bytes([p[1], p[2], p[3], p[4]]) as usize;
                let allowed = self.block(addr).is_some();
                let Some(sess) = self.session.as_mut() else {
                    return self.reply(cmd, seq, vec![7, 6]);
                };
                if !allowed {
                    return self.reply(cmd, seq, vec![7, 1]);
                }
                let crc = u32::from_le_bytes([p[8], p[9], p[10], p[11]]);
                sess.open = Some(Open {
                    addr,
                    format: p[5],
                    crc,
                    stream: Vec::new(),
                });
                self.reply(cmd, seq, vec![0]);
            }
            (0x1C, Some(5)) if p.len() >= 3 => {
                let offset = usize::from(u16::from_le_bytes([p[1], p[2]]));
                let Some(open) = self.session.as_mut().and_then(|s| s.open.as_mut()) else {
                    return self.reply(cmd, seq, vec![7, 6]);
                };
                if offset > open.stream.len() {
                    return self.reply(cmd, seq, vec![7, 8]);
                }
                if offset + p[3..].len() > open.stream.len() {
                    open.stream.truncate(offset);
                    open.stream.extend_from_slice(&p[3..]);
                }
                let got = open.stream.len() as u16;
                let mut r = vec![0];
                r.extend_from_slice(&got.to_le_bytes());
                self.reply(cmd, seq, r);
            }
            (0x1C, Some(6)) => {
                if let Some(reason) = self.refuse_next_end.take() {
                    if let Some(s) = self.session.as_mut() {
                        s.open = None;
                    }
                    return self.reply(cmd, seq, vec![7, reason]);
                }
                let Some(open) = self.session.as_mut().and_then(|s| s.open.take()) else {
                    return self.reply(cmd, seq, vec![7, 6]);
                };
                let data = if open.format == 2 {
                    vec![0xFF; BLOCK]
                } else {
                    match lz4_flex::block::decompress(&open.stream, BLOCK) {
                        Ok(d) if d.len() == BLOCK => d,
                        _ => return self.reply(cmd, seq, vec![7, 8]),
                    }
                };
                if crc32fast::hash(&data) != open.crc {
                    return self.reply(cmd, seq, vec![7, 7]);
                }
                let sess = self.session.as_mut().unwrap();
                sess.blocks += 1;
                if sess.staged {
                    sess.staged_blocks.push((open.addr, data));
                } else {
                    self.image[open.addr..open.addr + BLOCK].copy_from_slice(&data);
                    self.written_blocks.push(open.addr);
                }
                self.reply(cmd, seq, vec![0]);
            }
            (0x1C, Some(7)) => {
                let Some(sess) = self.session.take() else {
                    return self.reply(cmd, seq, vec![7, 6]);
                };
                for (addr, data) in sess.staged_blocks {
                    self.image[addr..addr + BLOCK].copy_from_slice(&data);
                    self.written_blocks.push(addr);
                }
                self.done = true;
                self.reply(cmd, seq, vec![0]);
            }
            (0x1C, Some(8)) => {
                let (state, mode, blocks) = match &self.session {
                    Some(s) => (
                        if s.open.is_some() { 2 } else { 1 },
                        u8::from(s.staged),
                        s.blocks,
                    ),
                    None => (if self.done { 4 } else { 0 }, 0, 0),
                };
                let mut r = vec![0, state, mode];
                r.extend_from_slice(&blocks.to_le_bytes());
                r.extend_from_slice(&0u16.to_le_bytes());
                r.push(0);
                self.reply(cmd, seq, r);
            }
            (0x1C, Some(9)) => {
                self.session = None;
                self.reply(cmd, seq, vec![0]);
            }
            (0x1C, _) => self.reply(cmd, seq, vec![2]),
            _ => self.reply(cmd, seq, vec![1]),
        }
    }
}

impl Write for FakeRadio {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.rx.extend_from_slice(buf);
        loop {
            match frame::parse(&self.rx, frame::MAX_REQUEST) {
                Parsed::Frame(f, used) => {
                    self.rx.drain(..used);
                    self.handle(f.cmd, f.seq, &f.payload);
                }
                Parsed::Bad(n) => {
                    self.rx.drain(..n);
                }
                Parsed::Need => break,
            }
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Read for FakeRadio {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.tx.is_empty() {
            return Err(io::Error::from(io::ErrorKind::TimedOut));
        }
        let n = buf.len().min(self.tx.len());
        for b in buf.iter_mut().take(n) {
            *b = self.tx.pop_front().unwrap();
        }
        Ok(n)
    }
}
