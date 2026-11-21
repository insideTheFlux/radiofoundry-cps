// SPDX-License-Identifier: MIT
// Copyright (c) 2026 The RadioFoundry Authors
// Copyright (c) 2026 insideTheFlux

//! The debug link's frame (RadioFoundry `docs/spec/debuglink.md` 2): `A5 5A`, command, sequence number, payload
//! length (u16 little endian), payload, CRC-16/CCITT-FALSE over the command to the end of the payload.

pub const MAX_REQUEST: usize = 64;
pub const MAX_REPLY: usize = 1024;

/// CRC-16/CCITT-FALSE: polynomial 0x1021, initial value 0xFFFF, no reflection, no final XOR.
pub fn crc16(data: &[u8]) -> u16 {
    let mut crc: u16 = 0xFFFF;
    for &b in data {
        crc ^= u16::from(b) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
        }
    }
    crc
}

pub fn encode(cmd: u8, seq: u8, payload: &[u8]) -> Vec<u8> {
    let mut f = Vec::with_capacity(payload.len() + 8);
    f.extend_from_slice(&[0xA5, 0x5A, cmd, seq]);
    f.extend_from_slice(&(payload.len() as u16).to_le_bytes());
    f.extend_from_slice(payload);
    let crc = crc16(&f[2..]);
    f.extend_from_slice(&crc.to_le_bytes());
    f
}

#[derive(Debug, PartialEq, Eq)]
pub struct Frame {
    pub cmd: u8,
    pub seq: u8,
    pub payload: Vec<u8>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Parsed {
    /// A whole frame; the bytes it used.
    Frame(Frame, usize),
    /// A frame whose CRC is wrong or whose length is out of range; the bytes to drop.
    Bad(usize),
    /// More bytes are needed.
    Need,
}

/// Finds the next frame in `buf`. Bytes before an `A5 5A` are noise and are dropped by the caller via `Bad`.
pub fn parse(buf: &[u8], max_payload: usize) -> Parsed {
    let Some(start) = buf.windows(2).position(|w| w == [0xA5, 0x5A]) else {
        // keep a trailing A5 in case its partner is still to come
        return if buf.last() == Some(&0xA5) && buf.len() > 1 {
            Parsed::Bad(buf.len() - 1)
        } else if buf.len() > 1 {
            Parsed::Bad(buf.len())
        } else {
            Parsed::Need
        };
    };
    if start > 0 {
        return Parsed::Bad(start);
    }
    if buf.len() < 6 {
        return Parsed::Need;
    }
    let len = usize::from(u16::from_le_bytes([buf[4], buf[5]]));
    if len > max_payload {
        return Parsed::Bad(2);
    }
    let total = 6 + len + 2;
    if buf.len() < total {
        return Parsed::Need;
    }
    let crc = u16::from_le_bytes([buf[6 + len], buf[7 + len]]);
    if crc16(&buf[2..6 + len]) != crc {
        return Parsed::Bad(2);
    }
    Parsed::Frame(
        Frame {
            cmd: buf[2],
            seq: buf[3],
            payload: buf[6..6 + len].to_vec(),
        },
        total,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc16_check_value() {
        assert_eq!(crc16(b"123456789"), 0x29B1);
    }

    #[test]
    fn a_frame_round_trips() {
        let f = encode(0x01, 7, b"hello");
        assert_eq!(
            parse(&f, MAX_REPLY),
            Parsed::Frame(
                Frame {
                    cmd: 1,
                    seq: 7,
                    payload: b"hello".to_vec()
                },
                f.len()
            )
        );
        assert_eq!(parse(&f[..f.len() - 1], MAX_REPLY), Parsed::Need);
    }

    #[test]
    fn noise_and_bad_crc_are_dropped() {
        let mut f = encode(0x01, 1, b"x");
        let mut noisy = vec![0x00, 0x11];
        noisy.extend_from_slice(&f);
        assert_eq!(parse(&noisy, MAX_REPLY), Parsed::Bad(2));
        let n = f.len();
        f[n - 1] ^= 0xFF;
        assert_eq!(parse(&f, MAX_REPLY), Parsed::Bad(2));
    }
}
