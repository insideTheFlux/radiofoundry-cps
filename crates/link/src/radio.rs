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
}

impl std::fmt::Display for LinkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LinkError::Io(e) => write!(f, "serial port: {e}"),
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
const CABLE_CHIPS: [(u16, Option<u16>); 7] = [
    (0x0403, None),         // FTDI
    (0x1A86, Some(0x7523)), // CH340
    (0x1A86, Some(0x55D4)), // CH9102
    (0x10C4, Some(0xEA60)), // CP210x
    (0x067B, None),         // Prolific
    (0x1FC9, Some(0x0094)), // the open-firmware radios' USB serial
    (0x0483, Some(0x5740)), // STM32 virtual COM port
];

/// The serial ports on this computer: likely radio cables first, then other USB ports, then the rest.
pub fn ports() -> Vec<PortInfo> {
    let mut v: Vec<PortInfo> = serialport::available_ports()
        .unwrap_or_default()
        .into_iter()
        .map(|p| match p.port_type {
            serialport::SerialPortType::UsbPort(u) => PortInfo {
                likely: CABLE_CHIPS
                    .iter()
                    .any(|&(v, pid)| v == u.vid && pid.is_none_or(|x| x == u.pid)),
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
