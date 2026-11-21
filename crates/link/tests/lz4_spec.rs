// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux

//! The streams the editor sends (docs/spec/cps-fast.md 5): valid LZ4 blocks that decode to exactly 4096 bytes, within
//! the 4128-byte limit, obeying the format's end rules (the last 5 bytes are literals; no match starts in the last 12
//! bytes). The radio's decoder accepts any valid stream, but a stream that breaks these is not a valid LZ4 block.

use rfcps_link::fast::BLANK_CRC;

const BLOCK: usize = 4096;
const MAX_STREAM: usize = BLOCK + BLOCK / 255 + 16;

/// Walks an LZ4 block: `(literals in the last sequence, the output position of the last match's start)`. Panics on
/// anything that is not a well-formed block that ends with a literals-only sequence.
fn walk(stream: &[u8], out_len: usize) -> (usize, Option<usize>) {
    let (mut i, mut out) = (0usize, 0usize);
    let mut last_match_start = None;
    loop {
        let token = stream[i];
        i += 1;
        let mut lit = usize::from(token >> 4);
        if lit == 15 {
            loop {
                let b = stream[i];
                i += 1;
                lit += usize::from(b);
                if b != 255 {
                    break;
                }
            }
        }
        i += lit;
        out += lit;
        if i == stream.len() {
            assert_eq!(out, out_len, "the stream decodes to {out} bytes");
            return (lit, last_match_start);
        }
        let offset = usize::from(stream[i]) | (usize::from(stream[i + 1]) << 8);
        i += 2;
        assert!(offset >= 1 && offset <= out, "a match offset {offset} at output {out}");
        let mut m = usize::from(token & 15);
        if m == 15 {
            loop {
                let b = stream[i];
                i += 1;
                m += usize::from(b);
                if b != 255 {
                    break;
                }
            }
        }
        last_match_start = Some(out);
        out += m + 4;
    }
}

fn blocks() -> Vec<Vec<u8>> {
    let mut v: Vec<Vec<u8>> = Vec::new();
    // Zeros, ones, a counter, a repeating record of 56 bytes (a channel), and padding runs, as a codeplug block is.
    v.push(vec![0; BLOCK]);
    v.push((0..BLOCK).map(|i| i as u8).collect());
    v.push((0..BLOCK).map(|i| (i % 56) as u8).collect());
    let mut chan = vec![0xFFu8; BLOCK];
    for k in 0..BLOCK / 56 {
        chan[k * 56..k * 56 + 10].copy_from_slice(b"Channel 00");
        chan[k * 56 + 16] = k as u8;
    }
    v.push(chan);
    // Incompressible: a simple xorshift.
    for seed in [1u64, 0x9E37_79B9_7F4A_7C15, 42] {
        let mut s = seed;
        v.push(
            (0..BLOCK)
                .map(|_| {
                    s ^= s << 13;
                    s ^= s >> 7;
                    s ^= s << 17;
                    s as u8
                })
                .collect(),
        );
    }
    // Compressible right up to the end, and a block whose only repeat sits in the last bytes.
    let mut tail = vec![0xAAu8; BLOCK];
    tail[..64].iter_mut().enumerate().for_each(|(i, b)| *b = (i * 7) as u8);
    v.push(tail);
    let mut end_repeat: Vec<u8> = (0..BLOCK).map(|i| (i * 31 % 251) as u8).collect();
    let (a, b) = (BLOCK - 40, BLOCK - 20);
    let copy = end_repeat[a..a + 16].to_vec();
    end_repeat[b..b + 16].copy_from_slice(&copy);
    v.push(end_repeat);
    v
}

#[test]
fn the_streams_sent_are_valid_lz4_blocks_within_the_limit_and_obey_the_end_rules() {
    for (n, data) in blocks().iter().enumerate() {
        let stream = lz4_flex::block::compress(data);
        assert!(stream.len() <= MAX_STREAM, "block {n}: a stream of {} bytes", stream.len());
        assert_eq!(&lz4_flex::block::decompress(&stream, BLOCK).unwrap(), data, "block {n} round trip");
        let (last_literals, last_match) = walk(&stream, BLOCK);
        assert!(last_literals >= 5, "block {n}: the last {last_literals} bytes are literals, the format needs 5");
        if let Some(at) = last_match {
            assert!(at <= BLOCK - 12, "block {n}: a match starts at {at}, within the last 12 bytes");
        }
    }
}

#[test]
fn the_blank_block_crc_is_the_crc32_of_4096_bytes_of_ff() {
    assert_eq!(BLANK_CRC, 0xF154_670A);
    assert_eq!(crc32fast::hash(&[0xFFu8; BLOCK]), BLANK_CRC);
}
