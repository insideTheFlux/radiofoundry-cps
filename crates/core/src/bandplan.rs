// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux

//! Repeater offsets by band plan, so typing one frequency of a channel can fill in the other, and the listen-only
//! bands (air band and broadcast FM), where a radio only receives and TX follows RX.
//!
//! A rule gives the range of the frequency the radio *receives* (the repeater's output) and the offset to the one it
//! transmits on (the repeater's input). A band plan is a convention, not a data format: pick the region that matches
//! where the radio is used.

use crate::records::Channel;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Region {
    /// No offsets: nothing is filled in.
    None,
    /// The ARRL band plan (United States).
    #[default]
    Us,
}

impl Region {
    pub const ALL: [Region; 2] = [Region::Us, Region::None];

    pub fn name(self) -> &'static str {
        match self {
            Region::None => "No offsets",
            Region::Us => "US (ARRL)",
        }
    }
}

struct Rule {
    lo: u64,
    hi: u64,
    offset: i64,
}

const fn r(lo_khz: u64, hi_khz: u64, offset_khz: i64) -> Rule {
    Rule {
        lo: lo_khz * 1000,
        hi: hi_khz * 1000,
        offset: offset_khz * 1000,
    }
}

/// US repeater outputs and their offsets: 2 m (+/-600 kHz), 1.25 m (-1.6 MHz), 70 cm (+/-5 MHz). On 70 cm the outputs
/// at 442 to 445 MHz pair with inputs 5 MHz up, and the ones from 445 to 450 MHz (the national simplex frequency,
/// 446.000, excepted) with inputs 5 MHz down. Local coordination can differ; the offset is a suggestion.
const US: [Rule; 9] = [
    r(144_510, 144_890, 600),
    r(145_110, 145_490, -600),
    r(146_610, 146_970, -600),
    r(147_000, 147_390, 600),
    r(223_850, 224_980, -1_600),
    r(442_000, 444_975, 5_000),
    r(445_000, 445_975, -5_000),
    r(446_100, 446_975, -5_000),
    r(447_000, 449_975, -5_000),
];

fn rules(region: Region) -> &'static [Rule] {
    match region {
        Region::None => &[],
        Region::Us => &US,
    }
}

/// The transmit frequency that pairs with a receive frequency, if it is in a repeater sub-band.
pub fn tx_for_rx(region: Region, rx: u64) -> Option<u64> {
    rules(region)
        .iter()
        .find(|x| (x.lo..=x.hi).contains(&rx))
        .map(|x| (rx as i64 + x.offset) as u64)
}

/// The receive frequency that pairs with a transmit frequency, when exactly one repeater pair fits.
pub fn rx_for_tx(region: Region, tx: u64) -> Option<u64> {
    let mut found = rules(region).iter().filter_map(|x| {
        let rx = tx as i64 - x.offset;
        (rx > 0 && (x.lo as i64..=x.hi as i64).contains(&rx)).then_some(rx as u64)
    });
    let first = found.next()?;
    found.next().is_none().then_some(first)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListenBand {
    Air,
    Broadcast,
}

/// 108 to 137 MHz is the air band (AM), 65 to 108 MHz is broadcast FM: a radio receives there and does not transmit.
pub fn listen_band(hz: u64) -> Option<ListenBand> {
    if (108_000_000..137_000_000).contains(&hz) {
        Some(ListenBand::Air)
    } else if (65_000_000..108_000_000).contains(&hz) {
        Some(ListenBand::Broadcast)
    } else {
        None
    }
}

/// Is TX following RX (simplex or a standard repeater offset)? Then a new RX frequency takes TX along.
fn tx_follows(region: Region, c: &Channel) -> bool {
    c.tx_hz == c.rx_hz || tx_for_rx(region, c.rx_hz) == Some(c.tx_hz)
}

fn rx_follows(region: Region, c: &Channel) -> bool {
    c.rx_hz == c.tx_hz || rx_for_tx(region, c.tx_hz) == Some(c.rx_hz)
}

/// Sets the receive frequency. TX goes along when it was following RX (equal, or the standard offset): to the new
/// standard offset in a repeater sub-band, else to the same frequency. TX set on purpose to something else is left
/// alone. In a listen-only band the channel becomes RX only and TX follows RX.
pub fn set_rx(region: Region, c: &mut Channel, new_rx: u64) {
    let follows = tx_follows(region, c);
    c.rx_hz = new_rx;
    if listen_band(new_rx).is_some() {
        c.tx_hz = new_rx;
        c.rx_only = true;
    } else if follows {
        c.tx_hz = tx_for_rx(region, new_rx).unwrap_or(new_rx);
    }
}

/// Sets the transmit frequency. RX goes along when it was following TX, and a standard repeater pair fits.
pub fn set_tx(region: Region, c: &mut Channel, new_tx: u64) {
    let follows = rx_follows(region, c);
    let was_simplex = c.rx_hz == c.tx_hz;
    c.tx_hz = new_tx;
    if listen_band(c.rx_hz).is_some() {
        c.tx_hz = c.rx_hz;
    } else if follows {
        if let Some(rx) = rx_for_tx(region, new_tx) {
            c.rx_hz = rx;
        } else if was_simplex {
            // a simplex channel stays simplex when its TX is typed
            c.rx_hz = new_tx;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fields::put_freq;
    use crate::records::{blank_channel, decode_channel};

    fn chan(rx: u64, tx: u64) -> Channel {
        let mut b = blank_channel();
        put_freq(&mut b, 0x10, 146_520_000);
        put_freq(&mut b, 0x14, 146_520_000);
        let mut c = decode_channel(1, &b).unwrap();
        c.rx_hz = rx;
        c.tx_hz = tx;
        c
    }

    #[test]
    fn standard_pairs() {
        let us = Region::Us;
        assert_eq!(tx_for_rx(us, 146_805_000), Some(146_205_000)); // 2 m, minus
        assert_eq!(tx_for_rx(us, 147_180_000), Some(147_780_000)); // 2 m, plus
        assert_eq!(tx_for_rx(us, 446_600_000), Some(441_600_000)); // 70 cm, minus 5
        assert_eq!(tx_for_rx(us, 443_000_000), Some(448_000_000)); // 70 cm, plus 5
        assert_eq!(tx_for_rx(us, 146_520_000), None); // simplex
        assert_eq!(tx_for_rx(Region::None, 446_600_000), None);
        assert_eq!(rx_for_tx(us, 441_600_000), Some(446_600_000));
        assert_eq!(rx_for_tx(us, 146_205_000), Some(146_805_000));
        assert_eq!(rx_for_tx(us, 100_000_000), None);
    }

    #[test]
    fn typing_rx_fills_in_tx_when_tx_follows() {
        let us = Region::Us;
        let mut c = chan(146_520_000, 146_520_000); // simplex
        set_rx(us, &mut c, 146_805_000);
        assert_eq!(c.tx_hz, 146_205_000);
        set_rx(us, &mut c, 446_600_000); // TX followed the standard offset: it follows again
        assert_eq!(c.tx_hz, 441_600_000);
        set_rx(us, &mut c, 446_000_000); // simplex segment: TX = RX
        assert_eq!(c.tx_hz, 446_000_000);
    }

    #[test]
    fn a_deliberate_tx_is_left_alone() {
        let us = Region::Us;
        let mut c = chan(146_805_000, 145_000_000); // an odd split set on purpose
        set_rx(us, &mut c, 146_940_000);
        assert_eq!(c.tx_hz, 145_000_000);
    }

    #[test]
    fn typing_tx_fills_in_rx() {
        let us = Region::Us;
        let mut c = chan(446_000_000, 446_000_000);
        set_tx(us, &mut c, 441_600_000);
        assert_eq!(c.rx_hz, 446_600_000);
        let mut s = chan(446_000_000, 446_000_000);
        set_tx(us, &mut s, 446_500_000); // not a repeater pair: simplex stays simplex
        assert_eq!((s.rx_hz, s.tx_hz), (446_500_000, 446_500_000));
    }

    #[test]
    fn no_region_no_offsets() {
        let mut c = chan(446_000_000, 446_000_000);
        set_rx(Region::None, &mut c, 446_600_000);
        assert_eq!(c.tx_hz, 446_600_000);
    }

    #[test]
    fn listen_only_bands_follow_rx_and_are_rx_only() {
        let mut c = chan(446_000_000, 446_000_000);
        set_rx(Region::Us, &mut c, 122_800_000);
        assert_eq!((c.tx_hz, c.rx_only), (122_800_000, true));
        set_tx(Region::Us, &mut c, 123_000_000);
        assert_eq!(c.tx_hz, 122_800_000);
        assert_eq!(listen_band(98_100_000), Some(ListenBand::Broadcast));
        assert_eq!(listen_band(137_000_000), None);
    }
}
