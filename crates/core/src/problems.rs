// SPDX-License-Identifier: MIT
// Copyright (c) 2026 The RadioFoundry Authors
// Copyright (c) 2026 insideTheFlux
//
// The checks a codeplug has to pass before it is written: what the web editor's `validate()` (RadioFoundry's
// `site/static/js/codeplug/model.js`) reports, as a list of problems a person can read. An error stops a write; a
// warning is worth a look.

use crate::fields::parse_tone;
use crate::freqs::{is_weather, listen_band, ListenBand};
use crate::model::Codeplug;
use crate::records::{CallType, Mode, MAX_CHANNELS, MAX_CONTACTS, MAX_RX_LISTS, MAX_ZONES};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Error,
    Warning,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Table {
    General,
    Channels,
    Zones,
    Contacts,
    RxLists,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Problem {
    pub level: Level,
    pub table: Table,
    /// The record's number (a channel's or contact's number, a zone's position from 1), if the problem is in one.
    pub number: Option<u16>,
    /// The record's name, for the list.
    pub item: String,
    pub message: String,
}

const NAME_MAX: usize = 16;
const ID_MAX: u32 = 0xFF_FFFF;

/// Everything wrong or doubtful about a codeplug. `bands` are the radio's frequency ranges in Hz, when known.
pub fn validate(cp: &Codeplug, bands: Option<&[(u64, u64)]>) -> Vec<Problem> {
    let mut out = Vec::new();
    let mut add = |level, table, number: Option<u16>, item: &str, message: String| {
        out.push(Problem {
            level,
            table,
            number,
            item: item.to_string(),
            message,
        });
    };
    let (err, warn) = (Level::Error, Level::Warning);

    let g = &cp.general;
    if g.dmr_id == 0 {
        add(
            warn,
            Table::General,
            None,
            "",
            "No DMR ID is set. DMR channels need one to transmit.".into(),
        );
    } else if g.dmr_id > ID_MAX {
        add(
            err,
            Table::General,
            None,
            "",
            "The DMR ID is more than 24 bits.".into(),
        );
    }
    if g.radio_name.chars().count() > 8 {
        add(
            err,
            Table::General,
            None,
            "",
            "The radio name is longer than 8 characters.".into(),
        );
    }

    if cp.channels.len() > MAX_CHANNELS {
        add(
            err,
            Table::Channels,
            None,
            "",
            format!(
                "{} channels; the most is {MAX_CHANNELS}.",
                cp.channels.len()
            ),
        );
    }
    if cp.zones.len() > MAX_ZONES {
        add(
            err,
            Table::Zones,
            None,
            "",
            format!("{} zones; the most is {MAX_ZONES}.", cp.zones.len()),
        );
    }
    if cp.contacts.len() > MAX_CONTACTS {
        add(
            err,
            Table::Contacts,
            None,
            "",
            format!(
                "{} contacts; the most is {MAX_CONTACTS}.",
                cp.contacts.len()
            ),
        );
    }
    if cp.rx_lists.len() > MAX_RX_LISTS {
        add(
            err,
            Table::RxLists,
            None,
            "",
            format!(
                "{} RX group lists; the most is {MAX_RX_LISTS}.",
                cp.rx_lists.len()
            ),
        );
    }

    for c in &cp.channels {
        let n = Some(c.number);
        let name = c.name.as_str();
        if name.is_empty() {
            add(
                err,
                Table::Channels,
                n,
                name,
                "The channel has no name.".into(),
            );
        }
        if name.chars().count() > NAME_MAX {
            add(
                err,
                Table::Channels,
                n,
                name,
                format!("The name is longer than {NAME_MAX} characters."),
            );
        }
        for (label, hz) in [("RX", c.rx_hz), ("TX", c.tx_hz)] {
            if hz == 0 || hz >= 1_000_000_000 {
                add(
                    err,
                    Table::Channels,
                    n,
                    name,
                    format!("The {label} frequency is not a frequency."),
                );
            } else if bands.is_some_and(|b| !b.iter().any(|&(lo, hi)| (lo..=hi).contains(&hz))) {
                add(
                    warn,
                    Table::Channels,
                    n,
                    name,
                    format!("The {label} frequency is outside the radio's bands."),
                );
            }
        }
        for (label, tone) in [("RX", &c.rx_tone), ("TX", &c.tx_tone)] {
            if parse_tone(tone).is_none() {
                add(
                    err,
                    Table::Channels,
                    n,
                    name,
                    format!("The {label} tone is not a CTCSS or DCS tone."),
                );
            }
        }
        if c.color_code > 15 {
            add(
                err,
                Table::Channels,
                n,
                name,
                "The color code is more than 15.".into(),
            );
        }
        if c.mode == Mode::Dmr && c.contact == 0 && !c.rx_only {
            add(
                warn,
                Table::Channels,
                n,
                name,
                "The DMR channel has no TX contact.".into(),
            );
        }
        match listen_band(c.rx_hz) {
            Some(b) if c.mode == Mode::Dmr => {
                add(
                    warn,
                    Table::Channels,
                    n,
                    name,
                    format!("DMR can't be received in the {}.", band_name(b)),
                );
            }
            _ => {}
        }
        if is_weather(c.rx_hz) && !c.rx_only {
            add(
                warn,
                Table::Channels,
                n,
                name,
                format!(
                    "A weather channel that can transmit on {:.4} MHz; set it to RX only.",
                    c.tx_hz as f64 / 1e6
                ),
            );
        }
        if let Some(b) = listen_band(c.tx_hz) {
            if !c.rx_only {
                add(
                    warn,
                    Table::Channels,
                    n,
                    name,
                    format!(
                        "The radio doesn't transmit in the {}; set the channel to RX only.",
                        band_name(b)
                    ),
                );
            }
        }
        if c.contact != 0 && !cp.contacts.iter().any(|k| k.number == c.contact) {
            add(
                warn,
                Table::Channels,
                n,
                name,
                format!(
                    "The TX contact (#{}) is not in the contact list.",
                    c.contact
                ),
            );
        }
        if c.rx_list != 0 && !cp.rx_lists.iter().any(|l| l.number == c.rx_list) {
            add(
                warn,
                Table::Channels,
                n,
                name,
                format!("The RX group list (#{}) does not exist.", c.rx_list),
            );
        }
        if c.dmr_id > ID_MAX {
            add(
                err,
                Table::Channels,
                n,
                name,
                "The channel's DMR ID is more than 24 bits.".into(),
            );
        }
    }
    for c in &cp.channels {
        if !c.name.is_empty() && cp.channels.iter().filter(|o| o.name == c.name).count() > 1 {
            add(
                warn,
                Table::Channels,
                Some(c.number),
                &c.name,
                "Another channel has the same name.".into(),
            );
        }
    }

    for (i, z) in cp.zones.iter().enumerate() {
        let n = Some(i as u16 + 1);
        if z.name.is_empty() {
            add(
                err,
                Table::Zones,
                n,
                &z.name,
                "The zone has no name.".into(),
            );
        }
        if z.name.chars().count() > NAME_MAX {
            add(
                err,
                Table::Zones,
                n,
                &z.name,
                format!("The name is longer than {NAME_MAX} characters."),
            );
        }
        if z.channels.len() > 80 {
            add(
                err,
                Table::Zones,
                n,
                &z.name,
                format!("{} channels in the zone; the most is 80.", z.channels.len()),
            );
        }
        if z.channels.is_empty() {
            add(
                warn,
                Table::Zones,
                n,
                &z.name,
                "The zone has no channels.".into(),
            );
        }
        for &m in &z.channels {
            if !cp.channels.iter().any(|c| c.number == m) {
                add(
                    warn,
                    Table::Zones,
                    n,
                    &z.name,
                    format!("The zone lists channel {m}, which does not exist."),
                );
            }
        }
    }
    for c in &cp.contacts {
        let n = Some(c.number);
        if c.name.is_empty() {
            add(
                err,
                Table::Contacts,
                n,
                &c.name,
                "The contact has no name.".into(),
            );
        }
        if c.name.chars().count() > NAME_MAX {
            add(
                err,
                Table::Contacts,
                n,
                &c.name,
                format!("The name is longer than {NAME_MAX} characters."),
            );
        }
        if c.call_type != CallType::All && !(1..=ID_MAX).contains(&c.id) {
            add(
                err,
                Table::Contacts,
                n,
                &c.name,
                "The DMR ID is not between 1 and 16777215.".into(),
            );
        }
    }
    for l in &cp.rx_lists {
        let n = Some(u16::from(l.number));
        if l.name.is_empty() {
            add(
                err,
                Table::RxLists,
                n,
                &l.name,
                "The RX group list has no name.".into(),
            );
        }
        if l.contacts.len() > 32 {
            add(
                err,
                Table::RxLists,
                n,
                &l.name,
                format!("{} contacts in the list; the most is 32.", l.contacts.len()),
            );
        }
    }
    out
}

fn band_name(b: ListenBand) -> &'static str {
    match b {
        ListenBand::Air => "air band",
        ListenBand::Broadcast => "broadcast FM band",
    }
}

pub fn error_count(problems: &[Problem]) -> usize {
    problems.iter().filter(|p| p.level == Level::Error).count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fields::put_freq;
    use crate::records::{blank_channel, decode_channel, Contact, Zone};

    fn chan(n: u16, name: &str, rx: u64) -> crate::records::Channel {
        let mut b = blank_channel();
        put_freq(&mut b, 0x10, rx);
        put_freq(&mut b, 0x14, rx);
        let mut c = decode_channel(n, &b).unwrap();
        c.name = name.into();
        c
    }

    fn cp() -> Codeplug {
        let mut cp = Codeplug::decode(&crate::Image::blank());
        cp.general.dmr_id = 3_100_123;
        cp.general.radio_name = "N0CALL".into();
        cp
    }

    fn msgs(p: &[Problem]) -> Vec<&str> {
        p.iter().map(|p| p.message.as_str()).collect()
    }

    #[test]
    fn a_clean_codeplug_has_no_problems() {
        let mut cp = cp();
        cp.channels.push(chan(1, "Simplex", 146_520_000));
        cp.zones.push(Zone {
            slot: 0,
            name: "Main".into(),
            channels: vec![1],
        });
        assert!(validate(&cp, None).is_empty());
    }

    #[test]
    fn channel_errors_and_warnings() {
        let mut cp = cp();
        let mut c = chan(1, "", 146_520_000);
        c.rx_tone = "banana".into();
        c.color_code = 20;
        cp.channels.push(c);
        let mut d = chan(2, "Dup", 98_100_000);
        d.mode = Mode::Dmr;
        cp.channels.push(d);
        cp.channels.push(chan(3, "Dup", 446_000_000));
        let p = validate(&cp, None);
        let m = msgs(&p);
        assert!(m.contains(&"The channel has no name."));
        assert!(m.contains(&"The RX tone is not a CTCSS or DCS tone."));
        assert!(m.contains(&"The color code is more than 15."));
        assert!(m
            .iter()
            .any(|s| s.starts_with("DMR can't be received in the broadcast FM")));
        assert!(m.contains(&"The DMR channel has no TX contact."));
        assert_eq!(
            m.iter()
                .filter(|s| **s == "Another channel has the same name.")
                .count(),
            2
        );
        assert_eq!(error_count(&p), 3);
    }

    #[test]
    fn frequencies_outside_the_radios_bands_are_warned() {
        let mut cp = cp();
        cp.channels.push(chan(1, "Low", 50_000_000));
        let p = validate(&cp, Some(&[(136_000_000, 174_000_000)]));
        assert!(msgs(&p).contains(&"The RX frequency is outside the radio's bands."));
        assert_eq!(error_count(&p), 0);
    }

    #[test]
    fn references_to_missing_records_are_warned() {
        let mut cp = cp();
        let mut c = chan(1, "Dmr", 446_000_000);
        c.mode = Mode::Dmr;
        c.contact = 9;
        cp.channels.push(c);
        cp.zones.push(Zone {
            slot: 0,
            name: "Z".into(),
            channels: vec![1, 7],
        });
        cp.zones.push(Zone {
            slot: 1,
            name: "Empty".into(),
            channels: vec![],
        });
        let p = validate(&cp, None);
        let m = msgs(&p);
        assert!(m.iter().any(|s| s.contains("TX contact (#9)")));
        assert!(m.iter().any(|s| s.contains("lists channel 7")));
        assert!(m.contains(&"The zone has no channels."));
    }

    #[test]
    fn contacts_and_general() {
        let mut cp = cp();
        cp.general.dmr_id = 0;
        cp.contacts.push(Contact {
            number: 1,
            name: "".into(),
            id: 0,
            call_type: CallType::Group,
            timeslot: 0,
        });
        cp.contacts.push(Contact {
            number: 2,
            name: "All".into(),
            id: 0xFF_FFFF,
            call_type: CallType::All,
            timeslot: 0,
        });
        let p = validate(&cp, None);
        let m = msgs(&p);
        assert!(m.iter().any(|s| s.starts_with("No DMR ID is set")));
        assert!(m.contains(&"The contact has no name."));
        assert!(m.contains(&"The DMR ID is not between 1 and 16777215."));
        assert_eq!(error_count(&p), 2);
    }
}
