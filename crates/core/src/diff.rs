// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux

//! What differs between two codeplugs, for a person to read before a write.

use crate::model::Codeplug;
use crate::problems::Table;
use crate::records::Channel;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Added,
    Changed,
    Removed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    pub table: Table,
    pub kind: Kind,
    pub number: Option<u16>,
    pub name: String,
    /// For a change: which fields.
    pub detail: String,
}

fn channel_fields(a: &Channel, b: &Channel) -> Vec<&'static str> {
    let mut v = Vec::new();
    let mut chk = |differs: bool, label: &'static str| {
        if differs {
            v.push(label);
        }
    };
    chk(a.name != b.name, "name");
    chk(a.rx_hz != b.rx_hz, "RX");
    chk(a.tx_hz != b.tx_hz, "TX");
    chk(a.mode != b.mode, "mode");
    chk(a.power != b.power, "power");
    chk(a.tot != b.tot, "TX timeout");
    chk(a.rx_tone != b.rx_tone, "RX tone");
    chk(a.tx_tone != b.tx_tone, "TX tone");
    chk(a.bw != b.bw, "bandwidth");
    chk(a.rx_only != b.rx_only, "RX only");
    chk(
        a.all_skip != b.all_skip || a.zone_skip != b.zone_skip,
        "scan skip",
    );
    chk(
        a.vox != b.vox || a.tone_burst != b.tone_burst,
        "VOX or tone burst",
    );
    chk(a.squelch != b.squelch, "squelch");
    chk(a.color_code != b.color_code, "color code");
    chk(a.timeslot != b.timeslot, "timeslot");
    chk(a.contact != b.contact, "contact");
    chk(a.rx_list != b.rx_list, "RX group list");
    chk(
        a.dmr_id != b.dmr_id || a.dmo != b.dmo,
        "DMR ID or direct mode",
    );
    v
}

/// The changes that take `old` to `new`.
pub fn changes(old: &Codeplug, new: &Codeplug) -> Vec<Change> {
    let mut out = Vec::new();
    let g = (&old.general, &new.general);
    let mut gf = Vec::new();
    if g.0.radio_name != g.1.radio_name {
        gf.push("radio name");
    }
    if g.0.dmr_id != g.1.dmr_id {
        gf.push("DMR ID");
    }
    if g.0.group_hang != g.1.group_hang || g.0.private_hang != g.1.private_hang {
        gf.push("hang time");
    }
    if g.0.boot_line1 != g.1.boot_line1 || g.0.boot_line2 != g.1.boot_line2 {
        gf.push("boot text");
    }
    let (k0, k1) = (&g.0.keys, &g.1.keys);
    if k0.ars_server != k1.ars_server {
        gf.push("ARS server");
    }
    if k0.ip_gateway != k1.ip_gateway {
        gf.push("IP gateway");
    }
    if (k0.call_end_tone, k0.tot_warn_tone, k0.battery_tone, k0.tx_deny_tone)
        != (k1.call_end_tone, k1.tot_warn_tone, k1.battery_tone, k1.tx_deny_tone)
    {
        gf.push("tones");
    }
    if k0.sig != k1.sig {
        gf.push("signaling tones");
    }
    if !gf.is_empty() {
        out.push(Change {
            table: Table::General,
            kind: Kind::Changed,
            number: None,
            name: String::new(),
            detail: gf.join(", "),
        });
    }

    for c in &new.channels {
        match old.channel(c.number) {
            None => out.push(Change {
                table: Table::Channels,
                kind: Kind::Added,
                number: Some(c.number),
                name: c.name.clone(),
                detail: String::new(),
            }),
            Some(o) => {
                let f = channel_fields(o, c);
                if !f.is_empty() || o.lat != c.lat || o.lon != c.lon {
                    let detail = if f.is_empty() {
                        "location".to_string()
                    } else {
                        f.join(", ")
                    };
                    out.push(Change {
                        table: Table::Channels,
                        kind: Kind::Changed,
                        number: Some(c.number),
                        name: c.name.clone(),
                        detail,
                    });
                }
            }
        }
    }
    for o in &old.channels {
        if new.channel(o.number).is_none() {
            out.push(Change {
                table: Table::Channels,
                kind: Kind::Removed,
                number: Some(o.number),
                name: o.name.clone(),
                detail: String::new(),
            });
        }
    }

    for (i, z) in new.zones.iter().enumerate() {
        let n = Some(i as u16 + 1);
        match old.zones.get(i) {
            None => out.push(Change {
                table: Table::Zones,
                kind: Kind::Added,
                number: n,
                name: z.name.clone(),
                detail: String::new(),
            }),
            Some(o) if o.name != z.name || o.channels != z.channels => {
                let detail = if o.name != z.name && o.channels == z.channels {
                    "name"
                } else if o.name == z.name {
                    "channels"
                } else {
                    "name, channels"
                };
                out.push(Change {
                    table: Table::Zones,
                    kind: Kind::Changed,
                    number: n,
                    name: z.name.clone(),
                    detail: detail.into(),
                });
            }
            _ => {}
        }
    }
    for (i, o) in old.zones.iter().enumerate().skip(new.zones.len()) {
        out.push(Change {
            table: Table::Zones,
            kind: Kind::Removed,
            number: Some(i as u16 + 1),
            name: o.name.clone(),
            detail: String::new(),
        });
    }

    for c in &new.contacts {
        match old.contacts.iter().find(|o| o.number == c.number) {
            None => out.push(Change {
                table: Table::Contacts,
                kind: Kind::Added,
                number: Some(c.number),
                name: c.name.clone(),
                detail: String::new(),
            }),
            Some(o) if o != c => out.push(Change {
                table: Table::Contacts,
                kind: Kind::Changed,
                number: Some(c.number),
                name: c.name.clone(),
                detail: String::new(),
            }),
            _ => {}
        }
    }
    for o in &old.contacts {
        if !new.contacts.iter().any(|c| c.number == o.number) {
            out.push(Change {
                table: Table::Contacts,
                kind: Kind::Removed,
                number: Some(o.number),
                name: o.name.clone(),
                detail: String::new(),
            });
        }
    }

    for l in &new.rx_lists {
        match old.rx_lists.iter().find(|o| o.number == l.number) {
            None => out.push(Change {
                table: Table::RxLists,
                kind: Kind::Added,
                number: Some(u16::from(l.number)),
                name: l.name.clone(),
                detail: String::new(),
            }),
            Some(o) if o != l => out.push(Change {
                table: Table::RxLists,
                kind: Kind::Changed,
                number: Some(u16::from(l.number)),
                name: l.name.clone(),
                detail: String::new(),
            }),
            _ => {}
        }
    }
    for o in &old.rx_lists {
        if !new.rx_lists.iter().any(|l| l.number == o.number) {
            out.push(Change {
                table: Table::RxLists,
                kind: Kind::Removed,
                number: Some(u16::from(o.number)),
                name: o.name.clone(),
                detail: String::new(),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Image;

    fn two() -> (Codeplug, Codeplug) {
        let mut a = Codeplug::decode(&Image::blank());
        let mut raw = crate::records::blank_channel();
        crate::fields::put_freq(&mut raw, 0x10, 146_520_000);
        crate::fields::put_freq(&mut raw, 0x14, 146_520_000);
        let mut c = crate::records::decode_channel(1, &raw).unwrap();
        c.name = "One".into();
        a.channels.push(c);
        let b = a.clone();
        (a, b)
    }

    #[test]
    fn identical_codeplugs_have_no_changes() {
        let (a, b) = two();
        assert!(changes(&a, &b).is_empty());
    }

    #[test]
    fn a_settings_area_key_is_a_general_change() {
        let (a, mut b) = two();
        b.general.keys.ars_server = Some(3_100_001);
        b.general.keys.battery_tone = Some(false);
        let ch = changes(&a, &b);
        assert_eq!(ch.len(), 1);
        assert_eq!(ch[0].table, Table::General);
        assert_eq!(ch[0].detail, "ARS server, tones");
    }

    #[test]
    fn edits_are_described() {
        let (a, mut b) = two();
        b.channels[0].name = "Uno".into();
        b.channels[0].rx_tone = "100.0".into();
        let mut c2 = b.channels[0].clone();
        c2.number = 2;
        c2.name = "Two".into();
        b.channels.push(c2);
        b.general.radio_name = "NEW".into();
        let ch = changes(&a, &b);
        assert_eq!(ch.len(), 3);
        assert!(ch
            .iter()
            .any(|c| c.table == Table::General && c.detail == "radio name"));
        assert!(ch.iter().any(|c| c.kind == Kind::Changed
            && c.number == Some(1)
            && c.detail == "name, RX tone"));
        assert!(ch
            .iter()
            .any(|c| c.kind == Kind::Added && c.number == Some(2) && c.name == "Two"));
        let removed = changes(&b, &a);
        assert!(removed
            .iter()
            .any(|c| c.kind == Kind::Removed && c.number == Some(2)));
    }
}
