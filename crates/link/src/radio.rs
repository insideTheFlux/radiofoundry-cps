// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux

//! A radio on a serial port: frames out, frames in, a status byte checked on every reply.

use crate::frame::{self, Frame, Parsed};
use std::io::{self, Read, Write};
use std::time::{Duration, Instant};

#[derive(Debug)]
pub enum LinkError {
    Io(io::Error),
    /// No reply in time.
    Timeout,
    /// The radio answered with a status other than 0 (and a reason byte for a refusal).
    Status {
        status: u8,
        reason: Option<u8>,
    },
    /// A reply that makes no sense (a length, a CRC, a stream that does not decode).
    Protocol(String),
    /// The radio's codeplug is not what it was when it was read, so a write would overwrite someone's change.
    RadioChanged(usize),
    /// A write finished but reading the radio back gave something else.
    VerifyFailed(usize),
    /// The radio's image layout is not the one this editor reads and writes (its INFO reply says so), so nothing was
    /// read or written.
    Layout(String),
    /// The codeplug's settings area and the radio's keys don't fit together (the area is full).
    Settings(&'static str),
    /// The ID database reaches past the end of the radio's image (the address it needs, the radio's end).
    IddbTooBig(usize, u32),
}

impl std::fmt::Display for LinkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LinkError::Io(e) => write!(f, "serial port: {e}"),
            LinkError::Layout(m) => write!(f, "this radio's codeplug layout is not supported: {m}"),
            LinkError::IddbTooBig(need, end) => write!(
                f,
                "the ID database needs the radio's image up to {need:#x}, which ends at {end:#x}"
            ),
            LinkError::Settings(e) => write!(f, "the codeplug's settings area: {e}"),
            LinkError::Timeout => write!(f, "the radio did not answer"),
            LinkError::Status {
                status,
                reason: None,
            } => write!(f, "the radio answered status {status}"),
            LinkError::Status {
                status,
                reason: Some(r),
            } => write!(f, "the radio refused (status {status}, reason {r})"),
            LinkError::Protocol(m) => write!(f, "unexpected reply: {m}"),
            LinkError::RadioChanged(n) => write!(f, "the radio's codeplug has changed since it was read ({n} blocks differ); read it again"),
            LinkError::VerifyFailed(n) => write!(f, "the radio does not hold what was written ({n} blocks differ)"),
        }
    }
}

impl std::error::Error for LinkError {}

impl From<io::Error> for LinkError {
    fn from(e: io::Error) -> Self {
        if matches!(
            e.kind(),
            io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
        ) {
            LinkError::Timeout
        } else {
            LinkError::Io(e)
        }
    }
}

pub type Result<T> = std::result::Result<T, LinkError>;

pub struct Radio<P> {
    port: P,
    seq: u8,
    buf: Vec<u8>,
    /// How long to wait for a reply.
    pub timeout: Duration,
}

impl<P: Read + Write> Radio<P> {
    pub fn new(port: P) -> Self {
        Self {
            port,
            seq: 0,
            buf: Vec::new(),
            timeout: Duration::from_secs(2),
        }
    }

    pub fn into_inner(self) -> P {
        self.port
    }

    /// The port, for a test to reach the software radio behind it.
    pub fn port_mut(&mut self) -> &mut P {
        &mut self.port
    }

    /// Sends one command and returns the reply payload after its status byte.
    pub fn request(&mut self, cmd: u8, payload: &[u8]) -> Result<Vec<u8>> {
        if payload.len() > frame::MAX_REQUEST {
            return Err(LinkError::Protocol(format!(
                "request of {} bytes",
                payload.len()
            )));
        }
        self.seq = self.seq.wrapping_add(1);
        let seq = self.seq;
        self.port.write_all(&frame::encode(cmd, seq, payload))?;
        self.port.flush()?;
        let end = Instant::now() + self.timeout;
        loop {
            let f = self.next_frame(end)?;
            if f.cmd == 0xFF && f.seq == seq {
                // an error frame: status 5 (bad CRC) or 6 (too long)
                return Err(LinkError::Status {
                    status: f.payload.first().copied().unwrap_or(0xFF),
                    reason: None,
                });
            }
            if f.cmd != (cmd | 0x80) || f.seq != seq {
                continue; // a late answer to an earlier request
            }
            let Some((&status, rest)) = f.payload.split_first() else {
                return Err(LinkError::Protocol("a reply without a status".into()));
            };
            if status != 0 {
                return Err(LinkError::Status {
                    status,
                    reason: rest.first().copied(),
                });
            }
            return Ok(rest.to_vec());
        }
    }

    fn next_frame(&mut self, end: Instant) -> Result<Frame> {
        loop {
            match frame::parse(&self.buf, frame::MAX_REPLY) {
                Parsed::Frame(f, used) => {
                    self.buf.drain(..used);
                    return Ok(f);
                }
                Parsed::Bad(n) => {
                    self.buf.drain(..n);
                    continue;
                }
                Parsed::Need => {}
            }
            if Instant::now() >= end {
                return Err(LinkError::Timeout);
            }
            let mut chunk = [0u8; 1100];
            match self.port.read(&mut chunk) {
                Ok(0) => return Err(LinkError::Timeout),
                Ok(n) => self.buf.extend_from_slice(&chunk[..n]),
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
                    ) => {}
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e.into()),
            }
        }
    }

    /// One section of the radio's live view (GET_VIEW, debuglink.md 3.7): its `key=value` text. Read only.
    pub fn view(&mut self, section: u8) -> Result<String> {
        let d = self.request(0x09, &[section])?;
        if d.len() < 5 {
            return Err(LinkError::Protocol("GET_VIEW reply too short".into()));
        }
        Ok(String::from_utf8_lossy(&d[5..]).into_owned())
    }

    /// The build's feature names from VERSION (`features=`, debuglink.md 3.1), e.g. `IDDB_COMPACT`.
    pub fn features(&mut self) -> Result<Vec<String>> {
        let text = self.request(0x02, &[])?;
        let text = String::from_utf8_lossy(&text);
        let list = text
            .lines()
            .find_map(|l| l.strip_prefix("features="))
            .unwrap_or("");
        Ok(list.split(',').filter(|s| !s.is_empty()).map(String::from).collect())
    }

    /// Does the radio's firmware read a format C DMR ID database (iddb-compact.md 6)?
    pub fn reads_compact_iddb(&mut self) -> Result<bool> {
        Ok(self.features()?.iter().any(|f| f == "IDDB_COMPACT"))
    }

    /// The radio's own lookup of a DMR ID in the ID database it holds (debuglink.md 3.23). ID 0 asks only about the
    /// database. Read only.
    pub fn iddb_lookup(&mut self, id: u32) -> Result<IddbAnswer> {
        let d = self.request(0x1F, &id.to_le_bytes())?;
        if d.len() < 6 {
            return Err(LinkError::Protocol("IDDB reply too short".into()));
        }
        Ok(IddbAnswer {
            format: d[0],
            records: u32::from_le_bytes([d[1], d[2], d[3], d[4]]),
            found: d[5] != 0,
            text: String::from_utf8_lossy(&d[6..]).into_owned(),
        })
    }

    /// Asks the radio to echo some bytes; proves a RadioFoundry debug link is on the other end.
    pub fn ping(&mut self) -> Result<()> {
        let echo = self.request(0x01, b"rfcps")?;
        if echo == b"rfcps" {
            Ok(())
        } else {
            Err(LinkError::Protocol("the echo differs".into()))
        }
    }
}

/// What the radio answers to a DMR ID lookup.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IddbAnswer {
    /// The header's letter (`N`, `n`, `C`); 0 for no database, or one this build does not read.
    pub format: u8,
    pub records: u32,
    pub found: bool,
    pub text: String,
}

/// A serial port, described for a chooser.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PortInfo {
    pub name: String,
    /// What the port is, for a person: the USB product name and ids, or "serial port".
    pub description: String,
    /// A USB serial port (a programming cable or a radio's USB port), as opposed to a legacy or built-in one.
    pub usb: bool,
    /// A USB serial chip programming cables and radios use (FTDI, CH340, CP210x, Prolific, the radios' own).
    pub likely: bool,
}

/// USB vendor and product ids of the serial chips radios and their cables use. `None` for a product matches every
/// product of the vendor.
const CABLE_CHIPS: [(u16, Option<u16>); 8] = [
    (0x0403, None),         // FTDI
    (0x2100, None),         // RT Systems cables: an FTDI chip under the vendor's own id
    (0x1A86, Some(0x7523)), // CH340
    (0x1A86, Some(0x55D4)), // CH9102
    (0x10C4, Some(0xEA60)), // CP210x
    (0x067B, None),         // Prolific
    (0x1FC9, Some(0x0094)), // the open-firmware radios' USB serial
    (0x0483, Some(0x5740)), // STM32 virtual COM port
];

fn is_cable_chip(vid: u16, pid: u16) -> bool {
    CABLE_CHIPS
        .iter()
        .any(|&(v, p)| v == vid && p.is_none_or(|x| x == pid))
}

/// The serial ports on this computer: likely radio cables first, then other USB ports, then the rest.
pub fn ports() -> Vec<PortInfo> {
    let mut v: Vec<PortInfo> = serialport::available_ports()
        .unwrap_or_default()
        .into_iter()
        .map(|p| match p.port_type {
            serialport::SerialPortType::UsbPort(u) => PortInfo {
                likely: is_cable_chip(u.vid, u.pid),
                description: format!(
                    "{} ({:04X}:{:04X})",
                    u.product
                        .or(u.manufacturer)
                        .unwrap_or_else(|| "USB serial".into()),
                    u.vid,
                    u.pid
                ),
                name: p.port_name,
                usb: true,
            },
            _ => PortInfo {
                description: "serial port".into(),
                name: p.port_name,
                usb: false,
                likely: false,
            },
        })
        .collect();
    v.sort_by(|a, b| {
        b.likely
            .cmp(&a.likely)
            .then(b.usb.cmp(&a.usb))
            .then_with(|| a.name.cmp(&b.name))
    });
    v
}

/// Names of the serial ports on this computer, for the port chooser.
pub fn port_names() -> Vec<String> {
    serialport::available_ports()
        .map(|p| p.into_iter().map(|p| p.port_name).collect())
        .unwrap_or_default()
}

/// Opens a serial port at the debug link's 115200 baud, 8N1.
pub fn open(name: &str) -> Result<Radio<Box<dyn serialport::SerialPort>>> {
    let port = serialport::new(name, 115_200)
        .timeout(Duration::from_millis(100))
        .open()
        .map_err(|e| LinkError::Io(io::Error::other(e.to_string())))?;
    Ok(Radio::new(port))
}

#[cfg(test)]
mod cable_tests {
    use super::is_cable_chip;

    #[test]
    fn cable_chips_are_ranked_as_likely() {
        assert!(is_cable_chip(0x0403, 0x6001), "FTDI, any product");
        assert!(
            is_cable_chip(0x2100, 0x9E5D),
            "RT Systems USB-K4Y: FTDI under its own vendor id"
        );
        assert!(is_cable_chip(0x1A86, 0x7523), "CH340");
        assert!(
            !is_cable_chip(0x1A86, 0x0001),
            "CH340's vendor, not a listed product"
        );
        assert!(!is_cable_chip(0x046D, 0xC52B), "a mouse receiver");
    }
}
