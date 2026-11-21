// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux

//! Writing a codeplug image to a radio, safely: only the blocks that differ, the radio's own bytes outside the
//! regions kept, a check that the radio still holds what it did when the image was read, and a read-back after.
//! (RadioFoundry `docs/spec/cps-fast.md` 7; `tools/cps_fast.py` does the same.)

use crate::fast::Block;
use crate::radio::{LinkError, Radio, Result};
use rfcps_core::image::{Image, BLOCK, REGIONS, SETTINGS_AREA};
use rfcps_core::kv;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::time::Duration;

/// The VFO A record: the radio shows its live VFO there, so it can differ between two reads.
const VFO_A: std::ops::Range<usize> = 0x7590..0x75C8;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WriteReport {
    /// Blocks written.
    pub blocks: usize,
    /// LZ4 stream bytes sent.
    pub stream_bytes: usize,
}

/// The block of the settings area (codeplug-settings.md): one 4 KB sector no region covers.
const AREA_BLOCK: usize = SETTINGS_AREA.0;

/// The 4 KB blocks the regions touch, in address order.
fn blocks_of(regions: &[(usize, usize)]) -> Vec<usize> {
    let mut v: Vec<usize> = Vec::new();
    for &(a, b) in regions {
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

/// Does one region hold the whole block?
fn covers(regions: &[(usize, usize)], a: usize) -> bool {
    regions.iter().any(|&(s, e)| s <= a && a + BLOCK <= e)
}

/// The block list with the settings area's block added, in address order.
fn with_area(mut v: Vec<usize>) -> Vec<usize> {
    v.push(AREA_BLOCK);
    v.sort_unstable();
    v.dedup();
    v
}

fn same_but_vfo(a: &[u8], b: &[u8], base: usize) -> bool {
    a.iter()
        .zip(b)
        .enumerate()
        .all(|(i, (x, y))| x == y || VFO_A.contains(&(base + i)))
}

impl<P: Read + Write> Radio<P> {
    /// CRC-32 and blank flag of each block, asking for runs of consecutive blocks together.
    fn sums_of(&mut self, addrs: &[usize]) -> Result<HashMap<usize, (u32, bool)>> {
        let per = usize::from(self.checked_info()?.sums_per_request.max(1));
        let mut out = HashMap::new();
        let mut i = 0;
        while i < addrs.len() {
            let mut n = 1;
            while i + n < addrs.len() && n < per && addrs[i + n] == addrs[i] + n * BLOCK {
                n += 1;
            }
            let s = self.codeplug_sums(addrs[i] as u32, n as u8)?;
            for k in 0..n {
                out.insert(addrs[i + k], (s.crcs[k], s.blank_mask & (1 << k) != 0));
            }
            i += n;
        }
        Ok(out)
    }

    fn block_bytes(&mut self, addr: usize, blank: bool) -> Result<Vec<u8>> {
        if blank {
            return Ok(vec![0xFF; BLOCK]);
        }
        match self.codeplug_read_block(addr as u32)? {
            Block::Blank => Ok(vec![0xFF; BLOCK]),
            Block::Data(d) => Ok(d),
        }
    }

    /// The blocks to send: each block the regions touch where what the image says differs from what the radio holds.
    /// A block the regions only partly cover keeps the radio's bytes outside them.
    pub fn plan_write(&mut self, image: &Image) -> Result<Vec<(usize, Vec<u8>)>> {
        let addrs = with_area(blocks_of(&REGIONS));
        let sums = self.sums_of(&addrs)?;
        let mut jobs = Vec::new();
        for a in addrs {
            let (crc, blank) = sums[&a];
            let want = if a == AREA_BLOCK || covers(&REGIONS, a) {
                image.view(a, BLOCK).to_vec()
            } else {
                let mut merged = self.block_bytes(a, blank)?;
                for &(s, e) in &REGIONS {
                    let (lo, hi) = (s.max(a), e.min(a + BLOCK));
                    if lo < hi {
                        merged[lo - a..hi - a].copy_from_slice(&image.bytes()[lo..hi]);
                    }
                }
                merged
            };
            if crc32fast::hash(&want) != crc {
                jobs.push((a, want));
            }
        }
        Ok(jobs)
    }

    /// The blocks whose bytes inside the regions and the settings area differ from `image` (the VFO A record aside).
    pub fn verify(&mut self, image: &Image) -> Result<Vec<usize>> {
        self.verify_with(image, true)
    }

    /// `verify`, with or without the settings area. The check that the radio still holds what was read leaves the
    /// area out: the radio's own menu changes its keys, and a write keeps those (`image_for_write`).
    fn verify_with(&mut self, image: &Image, area: bool) -> Result<Vec<usize>> {
        let mut addrs = blocks_of(&REGIONS);
        if area {
            addrs = with_area(addrs);
        }
        let sums = self.sums_of(&addrs)?;
        let mut bad = Vec::new();
        for a in addrs {
            let (crc, blank) = sums[&a];
            let want = image.view(a, BLOCK);
            if crc32fast::hash(want) == crc {
                continue;
            }
            let got = self.block_bytes(a, blank)?;
            let differs = a == AREA_BLOCK
                || REGIONS.iter().any(|&(s, e)| {
                    let (lo, hi) = (s.max(a), e.min(a + BLOCK));
                    lo < hi && !same_but_vfo(&got[lo - a..hi - a], &want[lo - a..hi - a], lo)
                });
            if differs {
                bad.push(a);
            }
        }
        Ok(bad)
    }

    /// `image` with the settings area as the radio should end up holding it (codeplug-settings.md 6, 7): the radio's
    /// own when the edit didn't change the area or the codeplug has none, or when the radio's is of a newer version;
    /// otherwise the radio's with only the keys the edit changed. `base` is the image the edits started from.
    pub fn image_for_write(&mut self, base: &Image, image: &Image) -> Result<Image> {
        let (_, blank) = self.sums_of(&[AREA_BLOCK])?[&AREA_BLOCK];
        let radio = self.block_bytes(AREA_BLOCK, blank)?;
        let mut out = image.clone();
        kv::area_for_write(out.bytes_mut(), &radio, base.view(AREA_BLOCK, BLOCK))
            .map_err(LinkError::Settings)?;
        Ok(out)
    }

    /// Writes `image` to the radio. `base` is the image the edits started from (what was read): if the radio no longer
    /// holds it, nothing is written. After the write the radio is read back and checked against what was sent.
    /// `progress(done, total)` is called per block.
    pub fn write_image(
        &mut self,
        base: &Image,
        image: &Image,
        progress: &mut dyn FnMut(usize, usize),
    ) -> Result<WriteReport> {
        let info = self.checked_info()?;
        let changed = self.verify_with(base, false)?;
        if !changed.is_empty() {
            return Err(LinkError::RadioChanged(changed.len()));
        }
        let image = &self.image_for_write(base, image)?;
        let jobs = self.plan_write(image)?;
        if jobs.is_empty() {
            return Ok(WriteReport {
                blocks: 0,
                stream_bytes: 0,
            });
        }
        let staged = info.staged_writes && jobs.len() <= usize::from(info.stage_max);
        self.codeplug_begin(staged)?;
        let mut bytes = 0;
        for (i, (a, data)) in jobs.iter().enumerate() {
            progress(i, jobs.len());
            match self.codeplug_write_block(*a as u32, data, usize::from(info.data_per_write)) {
                Ok(n) => bytes += n,
                Err(e) => {
                    let _ = self.codeplug_abort();
                    return Err(e);
                }
            }
        }
        progress(jobs.len(), jobs.len());
        self.codeplug_commit(Duration::from_secs(10 + 2 * jobs.len() as u64))?;
        let bad = self.verify(image)?;
        if !bad.is_empty() {
            return Err(LinkError::VerifyFailed(bad.len()));
        }
        Ok(WriteReport {
            blocks: jobs.len(),
            stream_bytes: bytes,
        })
    }
}
