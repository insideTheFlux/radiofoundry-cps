// SPDX-License-Identifier: MIT
// Copyright (c) 2026 The RadioFoundry Authors
// Copyright (c) 2026 insideTheFlux
//
// The checks a codeplug has to pass before it is written: what the web editor's `validate()` (RadioFoundry's
// `site/static/js/codeplug/model.js`) reports, as a list of problems a person can read. An error stops a write; a
// warning is worth a look.

use crate::fields::{name_bytes, parse_tone};
use crate::freqs::{is_weather, listen_band, ListenBand};
use crate::model::Codeplug;
use crate::modes::{Fallback, Params, NAC_ANY, NAC_REPEAT_ANY, NXDN_UNIT_MAX, P25_UNIT_MAX, P25_UNIT_WARN};
use crate::systems::{self, EntryBody, SystemFields};
use crate::records::{
    valid_dtmf, CallType, Mode, APRS_COMMENT_MAX, APRS_NAME_MAX, MAX_APRS, MAX_CHANNELS, MAX_CONTACTS, MAX_DTMF, MAX_RX_LISTS,
    MAX_ZONES,
};

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
    /// A system of a trunked mode (channel-modes.md 7).
    Systems,
    Dtmf,
    Aprs,
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
/// The highest DMR ID a system's own ID may be (the settings area's `ID_MAX`).
const ID_MAX_T3: u32 = 16_776_415;

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
    // The settings area's keys (codeplug-settings.md 3): an absent key says nothing, so only a value is checked.
    if !crate::kv::keys_valid(&g.keys) {
        add(
            err,
            Table::General,
            None,
            "",
            format!(
                "A settings-area value is out of range: IDs go up to {}, signaling tones up to {} Hz in steps of {} Hz and {} ms.",
                crate::kv::ID_MAX,
                crate::kv::SIG_HZ_MAX,
                crate::kv::SIG_HZ_UNIT,
                crate::kv::SIG_MS_MAX
            ),
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
        if name_bytes(name) > NAME_MAX {
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

    // DTMF contacts and APRS configurations.
    if cp.dtmf_contacts.len() > MAX_DTMF {
        add(err, Table::Dtmf, None, "", format!("{} DTMF contacts; the most is {MAX_DTMF}.", cp.dtmf_contacts.len()));
    }
    if cp.aprs.len() > MAX_APRS {
        add(err, Table::Aprs, None, "", format!("{} APRS configurations; the most is {MAX_APRS}.", cp.aprs.len()));
    }
    for d in &cp.dtmf_contacts {
        let (n, name) = (Some(u16::from(d.number)), d.name.as_str());
        if d.name.is_empty() {
            add(err, Table::Dtmf, n, name, "The DTMF contact has no name.".into());
        }
        if !valid_dtmf(&d.code) {
            add(err, Table::Dtmf, n, name, "The DTMF code must be 1 to 16 digits: 0-9, A-D, * or #.".into());
        }
    }
    for a in &cp.aprs {
        let (n, name) = (Some(u16::from(a.number)), a.name.as_str());
        if a.name.is_empty() {
            add(err, Table::Aprs, n, name, "The APRS configuration has no name.".into());
        }
        if name_bytes(&a.name) > APRS_NAME_MAX {
            add(err, Table::Aprs, n, name, format!("The name is longer than {APRS_NAME_MAX} characters."));
        }
        if name_bytes(&a.comment) > APRS_COMMENT_MAX {
            add(err, Table::Aprs, n, name, format!("The comment is longer than {APRS_COMMENT_MAX} characters."));
        }
    }
    for c in &cp.channels {
        if c.aprs != 0 && !cp.aprs.iter().any(|a| a.number == c.aprs) {
            add(
                warn,
                Table::Channels,
                Some(c.number),
                &c.name,
                format!("The channel names APRS configuration {}, which is not there.", c.aprs),
            );
        }
    }

    // Channel modes (channel-modes.md, p25-codeplug.md, nxdn-codeplug.md).
    for c in &cp.channels {
        let Some(t) = &c.tagged else { continue };
        let (n, name) = (Some(c.number), c.name.as_str());
        let target = c.contact_ref(&cp.contacts);
        match &t.params {
            Params::P25(p) => {
                if p.rx_nac == NAC_REPEAT_ANY {
                    add(err, Table::Channels, n, name, "The RX NAC 0xF7F is not allowed (0xF7E is any NAC).".into());
                }
                if p.tx_nac == NAC_ANY || p.tx_nac == NAC_REPEAT_ANY {
                    add(err, Table::Channels, n, name, "A TX NAC can't be 0xF7E or 0xF7F.".into());
                }
                if target.is_some_and(|k| k.call_type == CallType::Group && k.id > 0xFFFF) {
                    add(warn, Table::Channels, n, name, "The talk group is above 65535: the channel calls talk group 1.".into());
                }
            }
            Params::Nxdn(_) => {
                if target.is_some_and(|k| k.call_type == CallType::Group && k.id > 0xFFFF) {
                    add(warn, Table::Channels, n, name, "The group is above 65535: the channel calls group 1.".into());
                }
                if target.is_some_and(|k| k.call_type == CallType::Private && k.id > NXDN_UNIT_MAX) {
                    add(warn, Table::Channels, n, name, format!("The unit ID is above {NXDN_UNIT_MAX}: the channel calls group 1."));
                }
                if c.dmr_id > NXDN_UNIT_MAX {
                    add(warn, Table::Channels, n, name, format!("The channel's ID is above {NXDN_UNIT_MAX}: the radio's NXDN unit ID is used."));
                }
            }
            Params::P25Trunk(_) => {
                if target.is_some_and(|k| k.call_type == CallType::Group && k.id > 0xFFFF) {
                    add(warn, Table::Channels, n, name, "The talk group is above 65535: the channel calls talk group 1.".into());
                }
            }
            Params::Aes(p) => {
                if target.is_some_and(|k| k.call_type == CallType::Private && k.id > 0xFF_FFFF) {
                    add(warn, Table::Channels, n, name, "The unit ID is above 16777215: the channel calls group 1.".into());
                }
                // A missing key inhibits PTT (channel-modes.md 4): none picked, or one its system doesn't hold.
                let sys = (t.system != 0xFF).then(|| cp.systems.iter().find(|s| s.number == u16::from(t.system) + 1)).flatten();
                if !p.key_id.is_some_and(|id| sys.is_some_and(|s| s.aes_keys.iter().any(|k| k.id == id))) {
                    add(warn, Table::Channels, n, name, "The channel's AES key is missing or not one of its system's keys: the channel can't use AES.".into());
                }
            }
            // A Tier III channel has no parameters to check.
            Params::T3 => {}
            Params::Unknown => {
                if !(1..=4).contains(&t.kind) {
                    add(warn, Table::Channels, n, name, format!("The channel is of a mode this editor does not know (kind {}); it is kept as it is.", t.kind));
                }
            }
        }
    }
    // A channel of a kind that runs on a system: the system it names, and its fallback.
    for c in &cp.channels {
        let Some(t) = &c.tagged else { continue };
        let (n, name) = (Some(c.number), c.name.as_str());
        let want = match t.kind {
            crate::modes::KIND_P25_TRUNK => Some(systems::SYSTEM_P25_TRUNK),
            crate::modes::KIND_T3 => Some(1),
            crate::modes::KIND_NXDN => Some(systems::SYSTEM_NXDN),
            crate::modes::KIND_AES => Some(systems::SYSTEM_AES),
            _ => None,
        };
        if let Some(want) = want {
            // An NXDN conventional channel needs no system (its system is optional).
            if t.system == 0xFF {
                if t.kind != crate::modes::KIND_NXDN {
                    add(err, Table::Channels, n, name, "The channel names no system.".into());
                }
            } else {
                match cp.systems.iter().find(|s| s.number == u16::from(t.system) + 1) {
                    None => add(warn, Table::Channels, n, name, format!("The system the channel names (number {}) is not there.", u16::from(t.system) + 1)),
                    Some(s) if s.type_id != want => add(err, Table::Channels, n, name, "The channel's system is of another type.".into()),
                    _ => {}
                }
            }
        }
        if let crate::modes::Params::Nxdn(p) = &t.params {
            if let Some(id) = p.tx_key_id {
                let sys = (t.system != 0xFF).then(|| cp.systems.iter().find(|s| s.number == u16::from(t.system) + 1)).flatten();
                if !sys.is_some_and(|s| s.traffic_keys.iter().any(|k| k.id == u16::from(id))) {
                    add(err, Table::Channels, n, name, "The transmit key is not one of the channel's system's keys.".into());
                }
            }
        }
        // Encrypted or scrambled voice on an amateur frequency (user guide, Encryption on amateur frequencies): a warning,
        // nothing is blocked. A channel that may not transmit gets none.
        let encrypts = match &t.params {
            Params::Aes(p) => p.key_id.is_some(),
            Params::Nxdn(p) => p.tx_key_id.is_some(),
            _ => false,
        };
        let system_tx = (t.system != 0xFF)
            .then(|| cp.systems.iter().find(|s| s.number == u16::from(t.system) + 1))
            .flatten()
            .is_none_or(|s| s.tx);
        if encrypts && t.tx && system_tx && amateur_freq(c.tx_hz) {
            add(warn, Table::Channels, n, name, "This channel transmits encrypted or scrambled voice on an amateur frequency. Most countries, including the US, do not allow encrypted messages on amateur bands.".into());
        }
        match &t.fallback {
            Fallback::Stale(k) => add(warn, Table::Channels, n, name, format!("The fallback channel {k} is gone or renamed: the system's fallback is used.")),
            Fallback::Channel(k) if cp.channel(*k).is_some_and(|f| f.tagged.is_some()) => {
                add(warn, Table::Channels, n, name, "The fallback channel is itself of a mode; use a plain channel.".into())
            }
            _ => {}
        }
    }
    // Systems (channel-modes.md 7, p25-codeplug.md 3).
    if cp.systems.len() > systems::MAX_SYSTEMS {
        add(err, Table::Systems, None, "", format!("{} systems; the most is {}.", cp.systems.len(), systems::MAX_SYSTEMS));
    }
    let n_entries: usize = cp.systems.iter().map(|s| s.entries.len()).sum();
    if n_entries > systems::MAX_ENTRIES {
        add(err, Table::Systems, None, "", format!("{n_entries} system entries; the most is {}.", systems::MAX_ENTRIES));
    }
    let mut ids: Vec<(String, &str)> = Vec::new();
    for s in &cp.systems {
        let (n, name) = (Some(s.number), s.name.as_str());
        if !systems::type_known(s.type_id) {
            add(warn, Table::Systems, n, name, format!("A system of a type this editor does not know ({}); it is kept as it is.", s.type_id));
            continue;
        }
        if s.name.is_empty() {
            add(err, Table::Systems, n, name, "The system has no name.".into());
        }
        if s.name.chars().count() > NAME_MAX {
            add(err, Table::Systems, n, name, "The system name is longer than 16 characters.".into());
        }
        if let SystemFields::P25Trunk(_) = &s.fields {
            let sites: Vec<u8> = s.entries.iter().filter(|e| e.cls == 1).map(|e| e.group).collect();
            let mut dedup = sites.clone();
            dedup.sort_unstable();
            dedup.dedup();
            if dedup.len() != sites.len() {
                add(err, Table::Systems, n, name, "Two sites have the same number.".into());
            }
            let idens: Vec<u8> = s.entries.iter().filter(|e| e.cls == 2).map(|e| e.group).collect();
            let mut d2 = idens.clone();
            d2.sort_unstable();
            d2.dedup();
            if d2.len() != idens.len() {
                add(err, Table::Systems, n, name, "Two plan rows have the same identifier.".into());
            }
            for e in s.entries.iter().filter(|e| e.cls == 0 && e.control) {
                if !sites.is_empty() && !sites.contains(&e.group) {
                    add(warn, Table::Systems, n, name, format!("A control channel names site {}, which is not in the list.", e.group));
                }
            }
            if !s.entries.iter().any(|e| e.cls == 0 && e.control) {
                add(warn, Table::Systems, n, name, "The system has no control channel.".into());
            }
            for e in &s.entries {
                if let EntryBody::P25Control { rx_hz, .. } = &e.body {
                    if *rx_hz == 0 && e.control {
                        add(warn, Table::Systems, n, name, "A control channel has no frequency.".into());
                    }
                }
            }
        }
        if let SystemFields::T3(f) = &s.fields {
            if f.net > systems::net_max(f.model) {
                add(err, Table::Systems, n, name, format!("The NET is above {} for this network model.", systems::net_max(f.model)));
            }
            if f.sites.len() > systems::T3_SITES {
                add(err, Table::Systems, n, name, format!("{} sites; the most is {}.", f.sites.len(), systems::T3_SITES));
            }
            if f.sites.iter().any(|x| *x > systems::site_max(f.model)) {
                add(err, Table::Systems, n, name, format!("A site value is above {} for this network model.", systems::site_max(f.model)));
            }
            let ctl = s.entries.iter().filter(|e| e.cls == 0 && e.control).count();
            if ctl > systems::T3_CTRL {
                add(warn, Table::Systems, n, name, format!("More than {} control channels: the extra ones are skipped.", systems::T3_CTRL));
            }
            if ctl == 0 {
                add(warn, Table::Systems, n, name, "The system has no control channel.".into());
            }
            if s.own_id > ID_MAX_T3 {
                add(err, Table::Systems, n, name, format!("The radio's ID on the system is above {ID_MAX_T3}."));
            }
            for e in &s.entries {
                if let EntryBody::T3Channel { lcn, rx_hz, .. } = &e.body {
                    if !(1..=systems::T3_LCN).contains(lcn) {
                        add(err, Table::Systems, n, name, format!("An LCN is outside 1 to {}.", systems::T3_LCN));
                    }
                    if *rx_hz == 0 {
                        add(warn, Table::Systems, n, name, "A channel has no receive frequency.".into());
                    }
                }
            }
            if s.secret.as_deref().is_some_and(|v| systems::decode_t3_key(v).is_none()) {
                add(err, Table::Systems, n, name, "The key is shorter than 16 octets.".into());
            }
        }
        if s.type_id == systems::SYSTEM_NXDN && s.own_id > crate::modes::NXDN_UNIT_MAX {
            add(err, Table::Systems, n, name, format!("The radio's ID on the system is above {}.", crate::modes::NXDN_UNIT_MAX));
        }
        if s.needs_key && s.secret.is_none() {
            add(warn, Table::Systems, n, name, "The system needs its secret but has none.".into());
        }
        match &s.fallback {
            Fallback::Stale(k) => add(warn, Table::Systems, n, name, format!("The fallback channel {k} is gone or renamed.")),
            Fallback::Channel(k) if cp.channel(*k).is_some_and(|f| f.tagged.is_some()) => {
                add(warn, Table::Systems, n, name, "The fallback channel is itself of a mode; use a plain channel.".into())
            }
            _ => {}
        }
        if let Some(id) = s.identity() {
            if let Some((_, other)) = ids.iter().find(|(i, _)| *i == id) {
                add(warn, Table::Systems, n, name, format!("The same system as \"{other}\" (the same identity)."));
            } else {
                ids.push((id, name));
            }
        }
    }
    let ms = &cp.mode_settings;
    if ms.p25_unit_id > P25_UNIT_MAX {
        add(err, Table::General, None, "", format!("The P25 unit ID is above {P25_UNIT_MAX}."));
    } else if ms.p25_unit_id > P25_UNIT_WARN {
        add(warn, Table::General, None, "", format!("The P25 unit ID is longer than {P25_UNIT_WARN}: unusual."));
    }
    if u32::from(ms.nxdn_unit_id) > NXDN_UNIT_MAX {
        add(err, Table::General, None, "", format!("The NXDN unit ID is above {NXDN_UNIT_MAX}."));
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
        if name_bytes(&z.name) > NAME_MAX {
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
        if name_bytes(&c.name) > NAME_MAX {
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

/// The common amateur 2 m, 1.25 m and 70 cm allocations in Hz, edges included (RadioFoundry's `AMATEUR_HZ`).
const AMATEUR_HZ: [(u64, u64); 3] = [(144_000_000, 148_000_000), (222_000_000, 225_000_000), (420_000_000, 450_000_000)];

fn amateur_freq(hz: u64) -> bool {
    AMATEUR_HZ.iter().any(|&(lo, hi)| hz >= lo && hz <= hi)
}

fn band_name(b: ListenBand) -> &'static str {
    match b {
        ListenBand::Air => "air band",
        ListenBand::Broadcast => "broadcast FM band",
    }
}

/// Problems of the settings area (channel-modes.md 8): the radio's area is of a version this editor doesn't know, so
/// modes and systems set here are not written, or what is to be written is more than the 4 KB area holds.
pub fn area_problems(cp: &Codeplug, base: &crate::Image) -> Vec<Problem> {
    use crate::kv::{self, AreaState};
    let mut out = Vec::new();
    let any = !cp.systems.is_empty()
        || cp.channels.iter().any(|c| c.tagged.is_some())
        || cp.mode_settings != crate::modes::ModeSettings::default()
        || cp.general.keys != kv::Keys::default();
    if !any {
        return out;
    }
    if kv::decode_area(base.bytes()).state == AreaState::Newer {
        out.push(Problem {
            level: Level::Warning,
            table: Table::General,
            number: None,
            item: String::new(),
            message: "The settings area is from a newer version of RadioFoundry: modes, systems and keys set here are not written.".into(),
        });
    } else if !cp.settings_area_fits(base) {
        out.push(Problem {
            level: Level::Error,
            table: Table::Systems,
            number: None,
            item: String::new(),
            message: "The settings area is full (4 KB): what the modes, systems and keys need does not fit. Remove some.".into(),
        });
    }
    out
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

    #[test]
    fn channel_mode_problems() {
        use crate::modes::{self, NxdnConv, P25Conv, Params};
        let mut c = cp();
        let mut a = chan(1, "P25 A", 146_520_000);
        modes::set_kind(&mut a, modes::KIND_P25, Mode::Dmr, &[]);
        a.tagged.as_mut().unwrap().params = Params::P25(P25Conv { squelch: 0, rx_nac: 0xF7F, tx_nac: 0xF7E });
        let mut b = chan(2, "NXDN B", 146_540_000);
        modes::set_kind(&mut b, modes::KIND_NXDN, Mode::Dmr, &[]);
        b.tagged.as_mut().unwrap().params = Params::Nxdn(NxdnConv::default());
        b.dmr_id = 70_000;
        c.channels = vec![a, b];
        c.mode_settings.p25_unit_id = 0x100_0000;
        c.mode_settings.nxdn_unit_id = 65_500;
        let p = validate(&c, None);
        let m = msgs(&p);
        assert!(m.iter().any(|x| x.contains("RX NAC 0xF7F")), "{m:?}");
        assert!(m.iter().any(|x| x.contains("TX NAC")), "{m:?}");
        assert!(m.iter().any(|x| x.contains("channel's ID is above 65519")), "{m:?}");
        assert!(m.iter().any(|x| x.contains("P25 unit ID is above")), "{m:?}");
        // A good one has none of them.
        let mut ok = cp();
        let mut a = chan(1, "P25 A", 146_520_000);
        modes::set_kind(&mut a, modes::KIND_P25, Mode::Dmr, &[]);
        ok.channels = vec![a];
        assert!(validate(&ok, None).iter().all(|x| !x.message.contains("NAC") && !x.message.contains("unit ID")));
    }

    #[test]
    fn a_full_or_newer_settings_area_is_reported() {
        use crate::systems::{EntryBody, SysEntry, System, SYSTEM_P25_TRUNK};
        let base = crate::Image::blank();
        let mut c = Codeplug::decode(&base);
        assert!(area_problems(&c, &base).is_empty(), "nothing in the area: nothing to say");
        // A few systems fit.
        for i in 0..3u16 {
            let mut s = System::new(SYSTEM_P25_TRUNK, &format!("Sys {i}"));
            s.number = i + 1;
            c.systems.push(s);
        }
        assert!(c.settings_area_fits(&base));
        assert!(area_problems(&c, &base).is_empty());
        // Far too many entries do not.
        for s in &mut c.systems {
            for k in 0..100u8 {
                s.entries.push(SysEntry {
                    cls: 0,
                    group: 0,
                    control: k == 0,
                    flags: 0,
                    body: EntryBody::P25Control { iden: 1, chan_no: u16::from(k), rx_hz: 851_000_000 + u32::from(k), tx_hz: 0 },
                    raw: Vec::new(),
                });
            }
        }
        assert!(!c.settings_area_fits(&base));
        let p = area_problems(&c, &base);
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].level, Level::Error);
        // An area of a newer version is a warning, and never counts as full.
        let mut newer = crate::Image::blank();
        newer.bytes_mut()[crate::kv::KV_ADDR..crate::kv::KV_ADDR + 4].copy_from_slice(b"RFKV");
        newer.bytes_mut()[crate::kv::KV_ADDR + 4] = 2;
        let p = area_problems(&c, &newer);
        assert_eq!((p.len(), p[0].level), (1, Level::Warning));
    }

    #[test]
    fn system_problems() {
        use crate::systems::{EntryBody, SysEntry, System, SYSTEM_P25_TRUNK};
        let mut c = cp();
        let mut a = System::new(SYSTEM_P25_TRUNK, "Net A");
        a.number = 1;
        a.needs_key = true; // but no secret
        let site = |g| SysEntry { cls: 1, group: g, control: false, flags: 0, body: EntryBody::P25Site { rfss: None, site: None }, raw: Vec::new() };
        a.entries = vec![site(3), site(3)]; // the same site twice, and no control channel
        let mut b = System::new(SYSTEM_P25_TRUNK, "Net B");
        b.number = 2;
        b.name = String::new();
        c.systems = vec![a, b];
        let m: Vec<String> = validate(&c, None).into_iter().map(|p| p.message).collect();
        assert!(m.iter().any(|x| x.contains("Two sites have the same number")), "{m:?}");
        assert!(m.iter().any(|x| x.contains("no control channel")), "{m:?}");
        assert!(m.iter().any(|x| x.contains("needs its secret")), "{m:?}");
        assert!(m.iter().any(|x| x.contains("has no name")), "{m:?}");
        // The same identity (both any) is the same system.
        assert!(m.iter().any(|x| x.contains("same identity")), "{m:?}");
    }

    #[test]
    fn tier_iii_system_problems() {
        use crate::systems::{EntryBody, SysEntry, System, SystemFields, T3System, SYSTEM_T3};
        let mut c = cp();
        let mut s = System::new(SYSTEM_T3, "Net T3");
        s.number = 1;
        s.fields = SystemFields::T3(T3System { model: 3, net: 9, sites: vec![2000; 9], ..T3System::default() });
        s.secret = Some(vec![1; 8]); // shorter than 16 octets: not a key
        let ch = |control, lcn| SysEntry { cls: 0, group: 0xFF, control, flags: 0, body: EntryBody::T3Channel { lcn, rx_hz: 0, tx_hz: 0, cc: None }, raw: Vec::new() };
        s.entries = (0..5).map(|i| ch(true, i + 1)).collect();
        s.entries.push(ch(false, 5000));
        c.systems = vec![s];
        let m: Vec<String> = validate(&c, None).into_iter().map(|p| p.message).collect();
        assert!(m.iter().any(|x| x.contains("NET is above 3")), "{m:?}");
        assert!(m.iter().any(|x| x.contains("9 sites")), "{m:?}");
        assert!(m.iter().any(|x| x.contains("site value is above 1023")), "{m:?}");
        assert!(m.iter().any(|x| x.contains("More than 4 control channels")), "{m:?}");
        assert!(m.iter().any(|x| x.contains("LCN is outside")), "{m:?}");
        assert!(m.iter().any(|x| x.contains("shorter than 16 octets")), "{m:?}");
        // A tidy one has none of them.
        let mut ok = cp();
        let mut s = System::new(SYSTEM_T3, "Good");
        s.number = 1;
        s.entries = vec![ch(true, 1)];
        if let EntryBody::T3Channel { rx_hz, .. } = &mut s.entries[0].body {
            *rx_hz = 438_100_000;
        }
        ok.systems = vec![s];
        assert!(validate(&ok, None).iter().all(|x| x.table != Table::Systems), "{:?}", validate(&ok, None));
    }

    #[test]
    fn dtmf_and_aprs_problems() {
        use crate::records::{Aprs, DtmfContact};
        let mut c = cp();
        c.dtmf_contacts = vec![
            DtmfContact { number: 1, name: String::new(), code: "12".into() },
            DtmfContact { number: 2, name: "Bad".into(), code: "12E4".into() },
            DtmfContact { number: 3, name: "Fine".into(), code: "*#0".into() },
        ];
        let mut a = Aprs::new(1, "NINECHARS");
        a.comment = "x".repeat(24);
        let named = Aprs::new(2, "");
        c.aprs = vec![a, named];
        let mut ch = chan(1, "On aprs", 146_520_000);
        ch.aprs = 5; // there is no configuration 5
        c.channels = vec![ch];
        let m: Vec<String> = validate(&c, None).into_iter().map(|p| p.message).collect();
        assert!(m.iter().any(|x| x.contains("DTMF contact has no name")), "{m:?}");
        assert!(m.iter().any(|x| x.contains("DTMF code must be")), "{m:?}");
        assert_eq!(m.iter().filter(|x| x.contains("DTMF code must be")).count(), 1);
        assert!(m.iter().any(|x| x.contains("longer than 8")), "{m:?}");
        assert!(m.iter().any(|x| x.contains("comment is longer than 23")), "{m:?}");
        assert!(m.iter().any(|x| x.contains("APRS configuration has no name")), "{m:?}");
        assert!(m.iter().any(|x| x.contains("names APRS configuration 5")), "{m:?}");
    }
}
