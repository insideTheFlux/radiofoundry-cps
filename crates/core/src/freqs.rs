// SPDX-License-Identifier: MIT
// Copyright (c) 2026 The RadioFoundry Authors
// Copyright (c) 2026 insideTheFlux

//! How a channel's two frequencies go together, the rules of RadioFoundry's web editor: typing RX takes TX along when
//! they were the same (a simplex channel stays simplex); typing TX changes only TX; and in a listen-only band (air band,
//! broadcast FM) the radio receives and does not transmit, so TX follows RX and the channel is RX only.

use crate::records::Channel;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListenBand {
    Air,
    Broadcast,
}

/// 108 to 137 MHz is the air band (AM), 65 to 108 MHz is broadcast FM.
pub fn listen_band(hz: u64) -> Option<ListenBand> {
    if (108_000_000..137_000_000).contains(&hz) {
        Some(ListenBand::Air)
    } else if (65_000_000..108_000_000).contains(&hz) {
        Some(ListenBand::Broadcast)
    } else {
        None
    }
}

/// The North American weather-radio frequencies: the seven NOAA channels from 162.400 to 162.550 MHz in 25 kHz steps
/// and the three Canadian marine-weather channels. Other regions use different ones, so this is advice, never a rule
/// the editor applies on its own.
pub fn is_weather(hz: u64) -> bool {
    matches!(hz, 161_650_000 | 161_775_000 | 163_275_000)
        || ((162_400_000..=162_550_000).contains(&hz) && hz.is_multiple_of(25_000))
}

/// Marks every channel that receives on a weather frequency as RX only, leaving its frequencies alone. Returns how
/// many channels changed.
pub fn set_weather_rx_only(cp: &mut crate::model::Codeplug) -> usize {
    let mut n = 0;
    for c in &mut cp.channels {
        if is_weather(c.rx_hz) && !c.rx_only {
            c.rx_only = true;
            n += 1;
        }
    }
    n
}

/// A channel whose RX is in a listen band is RX only with TX on RX, as the radio's channel editor saves it: no stray TX
/// frequency from before (a channel read, imported or edited elsewhere) is shown, exported or written. A TX frequency in
/// a listen band makes the channel RX only too (no radio transmits there).
pub fn follow_listen(c: &mut Channel) {
    if listen_band(c.rx_hz).is_some() {
        c.rx_only = true;
        c.tx_hz = c.rx_hz;
    } else if listen_band(c.tx_hz).is_some() {
        c.rx_only = true;
    }
}

/// Sets the receive frequency.
pub fn set_rx(c: &mut Channel, new_rx: u64) {
    let simplex = c.tx_hz == c.rx_hz;
    c.rx_hz = new_rx;
    if listen_band(new_rx).is_some() {
        c.tx_hz = new_rx;
        c.rx_only = true;
    } else if simplex {
        c.tx_hz = new_rx;
    }
}

/// Sets the transmit frequency (locked to RX in a listen-only band).
pub fn set_tx(c: &mut Channel, new_tx: u64) {
    c.tx_hz = if listen_band(c.rx_hz).is_some() {
        c.rx_hz
    } else {
        new_tx
    };
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
    fn a_simplex_channel_stays_simplex_when_rx_is_typed() {
        let mut c = chan(146_520_000, 146_520_000);
        set_rx(&mut c, 146_550_000);
        assert_eq!((c.rx_hz, c.tx_hz), (146_550_000, 146_550_000));
    }

    #[test]
    fn a_repeater_keeps_its_tx_when_rx_is_typed() {
        let mut c = chan(146_805_000, 146_205_000);
        set_rx(&mut c, 146_940_000);
        assert_eq!((c.rx_hz, c.tx_hz), (146_940_000, 146_205_000));
    }

    #[test]
    fn typing_tx_changes_only_tx() {
        let mut c = chan(446_600_000, 441_600_000);
        set_tx(&mut c, 441_700_000);
        assert_eq!((c.rx_hz, c.tx_hz), (446_600_000, 441_700_000));
        let mut s = chan(446_000_000, 446_000_000);
        set_tx(&mut s, 446_500_000);
        assert_eq!((s.rx_hz, s.tx_hz), (446_000_000, 446_500_000));
    }

    #[test]
    fn listen_only_bands_follow_rx_and_are_rx_only() {
        let mut c = chan(446_000_000, 441_000_000);
        set_rx(&mut c, 122_800_000);
        assert_eq!((c.tx_hz, c.rx_only), (122_800_000, true));
        set_tx(&mut c, 123_000_000);
        assert_eq!(c.tx_hz, 122_800_000);
        assert_eq!(listen_band(98_100_000), Some(ListenBand::Broadcast));
        assert_eq!(listen_band(137_000_000), None);
    }

    #[test]
    fn weather_frequencies_are_found_and_set_rx_only() {
        assert!(is_weather(162_400_000) && is_weather(162_550_000) && is_weather(161_650_000));
        assert!(!is_weather(162_412_500) && !is_weather(162_575_000) && !is_weather(146_940_000));
        let mut cp = crate::model::Codeplug::decode(&crate::Image::blank());
        cp.channels = vec![chan(162_400_000, 400_000_000), chan(146_940_000, 146_340_000)];
        assert_eq!(set_weather_rx_only(&mut cp), 1);
        assert!(cp.channels[0].rx_only && !cp.channels[1].rx_only);
        assert_eq!(cp.channels[0].tx_hz, 400_000_000);
        assert_eq!(set_weather_rx_only(&mut cp), 0);
    }
}
