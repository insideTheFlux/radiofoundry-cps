// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux

//! A software radio: speaks the debug link and the fast codeplug protocol's read side over a byte stream, from an
//! image in memory. For tests and for trying the app without a radio.

use crate::fast::BLANK_CRC;
use crate::frame::{self, Parsed};
use rfcps_core::image::{editor_blocks, BLOCK};
use std::collections::VecDeque;
use std::io::{self, Read, Write};

pub struct FakeRadio {
    image: Vec<u8>,
    rx: Vec<u8>,
    tx: VecDeque<u8>,
    /// Report a wrong CRC-32 on the next block read, once (the host should read the block again).
    pub corrupt_next_read: bool,
    /// Frames received, for tests.
    pub requests: usize,
}

impl FakeRadio {
    pub fn new(image: &[u8]) -> Self {
        Self {
            image: image.to_vec(),
            rx: Vec::new(),
            tx: VecDeque::new(),
            corrupt_next_read: false,
            requests: 0,
        }
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
                let mut r = vec![0, 1, 2];
                r.extend_from_slice(&(BLOCK as u16).to_le_bytes());
                r.extend_from_slice(&[16, 61]);
                r.extend_from_slice(&1000u16.to_le_bytes());
                for v in [0x10000u32, 0x20000, self.image.len() as u32, 0x700000] {
                    r.extend_from_slice(&v.to_le_bytes());
                }
                r.extend_from_slice(&0u16.to_le_bytes());
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
