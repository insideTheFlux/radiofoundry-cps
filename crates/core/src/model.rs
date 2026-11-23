// SPDX-License-Identifier: MIT
// Copyright (c) 2026 The RadioFoundry Authors
// Copyright (c) 2026 insideTheFlux
//
// A whole codeplug decoded from an image. Ported from `decode()` in RadioFoundry's
// `site/static/js/codeplug/model.js`. Read-only for now.

use crate::image::Image;
use crate::records::*;

#[derive(Clone, Debug, PartialEq)]
pub struct Codeplug {
    pub general: General,
    pub channels: Vec<Channel>,
    /// The two VFOs (channel records at fixed addresses); `None` where the record doesn't decode.
    pub vfo: [Option<Channel>; 2],
    pub zones: Vec<Zone>,
    pub contacts: Vec<Contact>,
    pub rx_lists: Vec<RxList>,
    /// DTMF contacts (1-63) and APRS configurations (1-8).
    pub dtmf_contacts: Vec<DtmfContact>,
    pub aprs: Vec<Aprs>,
    /// The kinds' radio-wide records (the P25 and NXDN unit IDs) in the settings area.
    pub mode_settings: crate::modes::ModeSettings,
    /// The systems of the trunked kinds, with their entries and secrets (channel-modes.md 7).
    pub systems: Vec<crate::systems::System>,
    /// The entries of systems that aren't there: kept as found.
    pub orphan_entries: Vec<Vec<u8>>,
    /// The ID database's header as read (the web editor's `model.iddb`); `None` without a database. Read only: the
    /// codeplug's own write never touches the database.
    pub iddb: Option<crate::iddb::Header>,
}

fn channel_used(b: &[u8], n: usize) -> bool {
    let (bank, k) = ((n - 1) / CH_PER_BANK, (n - 1) % CH_PER_BANK);
    b[bank_bitmap(bank) + (k >> 3)] & (1 << (k & 7)) != 0
}

impl Codeplug {
    pub fn decode(image: &Image) -> Self {
        let b = image.bytes();
        let mut contacts = Vec::new();
        for n in 1..=MAX_CONTACTS {
            let a = CONTACTS + (n - 1) * CONTACT_SIZE;
            if let Some(c) = decode_contact(n as u16, &b[a..a + CONTACT_SIZE]) {
                contacts.push(c);
            }
        }
        let mut rx_lists = Vec::new();
        for n in 1..=MAX_RX_LISTS {
            let count = b[LIST_COUNTS + n - 1];
            if count == 0 || count == 0xFF {
                continue;
            }
            let a = LISTS + (n - 1) * LIST_SIZE;
            rx_lists.push(decode_list(n as u8, &b[a..a + LIST_SIZE], count));
        }
        let mut dtmf_contacts = Vec::new();
        for n in 1..=MAX_DTMF {
            let a = DTMF_CONTACTS + (n - 1) * DTMF_SIZE;
            if let Some(d) = decode_dtmf(n as u8, &b[a..a + DTMF_SIZE]) {
                dtmf_contacts.push(d);
            }
        }
        let mut aprs = Vec::new();
        for n in 1..=MAX_APRS {
            let a = APRS_BASE + (n - 1) * APRS_SIZE;
            if let Some(p) = decode_aprs(n as u8, &b[a..a + APRS_SIZE]) {
                aprs.push(p);
            }
        }
        let mut channels = Vec::new();
        for n in 1..=MAX_CHANNELS {
            if !channel_used(b, n) {
                continue;
            }
            let a = channel_addr(n);
            if let Some(mut c) = decode_channel(n as u16, &b[a..a + CH_SIZE]) {
                crate::freqs::follow_listen(&mut c);
                channels.push(c);
            }
        }
        let per = zone_layout(b);
        let rec = 16 + 2 * per;
        let mut zones = Vec::new();
        for slot in 0..MAX_ZONES {
            if b[ZONE_BITMAP + (slot >> 3)] & (1 << (slot & 7)) == 0 {
                continue;
            }
            let a = ZONE_DATA + slot * rec;
            let r = &b[a..a + rec];
            if r[0] == 0x00 || r[0] == 0xFF {
                continue;
            }
            zones.push(decode_zone(slot, r, per));
        }
        let vfo = [0, 1].map(|i| decode_channel(0, &b[VFO[i]..VFO[i] + CH_SIZE]));
        // The modes: the settings area's tags onto the channels, and the kinds' radio-wide records.
        let area = crate::kv::decode_area(b);
        let entries = if area.state == crate::kv::AreaState::Valid {
            crate::kv::dedupe(&area.entries)
        } else {
            Vec::new()
        };
        crate::modes::decode_tags(&mut channels, &entries);
        let mode_settings = crate::modes::decode_settings(&entries);
        let names: std::collections::HashMap<u16, String> =
            channels.iter().map(|c| (c.number, c.name.clone())).collect();
        let (systems, orphan_entries) = crate::systems::decode_systems(&entries, &names);
        Self {
            dtmf_contacts,
            aprs,
            mode_settings,
            systems,
            orphan_entries,
            iddb: crate::iddb::read_header(b),
            general: decode_general(b),
            channels,
            vfo,
            zones,
            contacts,
            rx_lists,
        }
    }

    /// Whether the keys, modes and systems fit the settings area of `base` (4 KB, shared with every other key). An
    /// area of a newer version is never written, so it counts as fitting (the problems list says so separately).
    pub fn settings_area_fits(&self, base: &Image) -> bool {
        use crate::kv::{self, AreaState};
        if kv::decode_area(base.bytes()).state == AreaState::Newer {
            return true;
        }
        let mut b = base.bytes().to_vec();
        let mut systems = self.systems.clone();
        crate::systems::number_systems(&mut systems);
        let keys_ok = kv::put_keys(&mut b, &self.general.keys);
        let modes_ok = crate::modes::encode_modes(
            &crate::modes::ModesIn {
                channels: &self.channels,
                settings: &self.mode_settings,
                systems: &systems,
                orphans: &self.orphan_entries,
            },
            &mut b,
            || crate::modes::channel_names(base.bytes()),
        );
        keys_ok && modes_ok
    }

    /// Takes an APRS configuration out. Channels and VFOs that named it name none.
    pub fn remove_aprs(&mut self, number: u8) {
        self.aprs.retain(|p| p.number != number);
        for c in self.channels.iter_mut().chain(self.vfo.iter_mut().flatten()) {
            if c.aprs == number {
                c.aprs = 0;
            }
        }
    }

    /// A system the codeplug has, by number (1-255).
    pub fn system(&self, number: u16) -> Option<&crate::systems::System> {
        self.systems.iter().find(|s| s.number == number)
    }

    /// Takes a system out, with its entries and secret (the writer drops them). Channels on it become plain
    /// receive-only DMR channels: their record holds the system's frequencies, where they must not transmit.
    pub fn remove_system(&mut self, number: u16) {
        self.systems.retain(|s| s.number != number);
        let systems = self.systems.clone();
        for c in &mut self.channels {
            if c.tagged.as_ref().is_some_and(|t| t.system != 0xFF && u16::from(t.system) + 1 == number) {
                // A kind whose system is optional (NXDN conventional) stays what it is, on no system.
                if c.kind() == crate::modes::KIND_NXDN {
                    if let Some(t) = &mut c.tagged {
                        t.system = 0xFF;
                    }
                    continue;
                }
                crate::modes::set_kind(c, 0, crate::records::Mode::Dmr, &systems);
                c.rx_only = true;
            }
        }
    }

    pub fn channel(&self, number: u16) -> Option<&Channel> {
        self.channels.iter().find(|c| c.number == number)
    }
}

fn same_contact(a: &Contact, b: &Contact) -> bool {
    (&a.name, a.id, a.call_type, a.timeslot) == (&b.name, b.id, b.call_type, b.timeslot)
}

impl Codeplug {
    /// The codeplug written onto a copy of the image it was decoded from. Only records that differ from what the
    /// image holds are rewritten, so everything this model doesn't cover (vendor bytes, APRS, DTMF, the keyed
    /// settings, the channel modes) comes through unchanged, and an unedited codeplug encodes to the same bytes.
    pub fn encode(&self, base: &Image) -> Image {
        let mut img = base.clone();
        let b = img.bytes_mut();
        let mut systems = self.systems.clone();
        crate::systems::number_systems(&mut systems);

        // Channels: the records, then the in-use bitmaps. A channel without a number is not stored.
        let mut bits = [0u8; MAX_CHANNELS / 8];
        for c in self.channels.iter().filter(|c| c.number != 0) {
            let n = usize::from(c.number);
            bits[(n - 1) >> 3] |= 1 << ((n - 1) & 7);
            let a = channel_addr(n);
            // A channel of a kind keeps its safe stand-in record (channel-modes.md 2.3).
            let mut synced;
            let c = if c.tagged.is_some() {
                synced = c.clone();
                crate::modes::stand_in(&mut synced, &systems);
                &synced
            } else {
                c
            };
            put_channel(&mut b[a..a + CH_SIZE], c);
        }
        for bank in 0..8 {
            let a = bank_bitmap(bank);
            b[a..a + 16].copy_from_slice(&bits[bank * 16..bank * 16 + 16]);
        }
        for (i, v) in self.vfo.iter().enumerate() {
            if let Some(c) = v {
                put_channel(&mut b[VFO[i]..VFO[i] + CH_SIZE], c);
            }
        }

        // Zones: packed into slots 0..n-1 in order, the 80-channel layout. A 16-channel table is rewritten whole.
        let per = zone_layout(b);
        if per != 80 && !self.zones.is_empty() {
            b[ZONE_DATA..ZONE_DATA + MAX_ZONES * ZONE_RECORD].fill(0);
        }
        let zones = &self.zones[..self.zones.len().min(MAX_ZONES)];
        for (slot, z) in zones.iter().enumerate() {
            let a = ZONE_DATA + slot * ZONE_RECORD;
            let want = Zone {
                slot,
                name: z.name.clone(),
                channels: z.channels.iter().copied().filter(|&n| n != 0).collect(),
            };
            let have = if per == 80 {
                Some(decode_zone(slot, &b[a..a + ZONE_RECORD], 80))
            } else {
                None
            };
            if have.is_none_or(|h| h.name != want.name || h.channels != want.channels) {
                encode_zone(&want, &mut b[a..a + ZONE_RECORD]);
            }
        }
        for slot in 0..MAX_ZONES {
            let (a, bit) = (ZONE_BITMAP + (slot >> 3), 1u8 << (slot & 7));
            b[a] = if slot < zones.len() {
                b[a] | bit
            } else {
                b[a] & !bit
            };
        }

        // Contacts: a number without a contact is erased, as the radio deletes one.
        for n in 1..=MAX_CONTACTS {
            let a = CONTACTS + (n - 1) * CONTACT_SIZE;
            let have = decode_contact(n as u16, &b[a..a + CONTACT_SIZE]);
            match (
                self.contacts.iter().find(|c| usize::from(c.number) == n),
                have,
            ) {
                (Some(c), h) if h.as_ref().is_none_or(|h| !same_contact(h, c)) => {
                    encode_contact(c, &mut b[a..a + CONTACT_SIZE]);
                }
                (None, Some(_)) => b[a..a + CONTACT_SIZE].fill(0xFF),
                _ => {}
            }
        }

        // RX group lists: the record, then the count that makes it visible.
        for n in 1..=MAX_RX_LISTS {
            let a = LISTS + (n - 1) * LIST_SIZE;
            let count = b[LIST_COUNTS + n - 1];
            let shown = count != 0 && count != 0xFF;
            let Some(l) = self.rx_lists.iter().find(|l| usize::from(l.number) == n) else {
                if shown {
                    b[LIST_COUNTS + n - 1] = 0;
                }
                continue;
            };
            let want = RxList {
                number: l.number,
                name: l.name.clone(),
                contacts: l
                    .contacts
                    .iter()
                    .copied()
                    .filter(|&x| x != 0)
                    .take(32)
                    .collect(),
            };
            let have = if shown {
                Some(decode_list(l.number, &b[a..a + LIST_SIZE], count))
            } else {
                None
            };
            if have.is_none_or(|h| h.name != want.name || h.contacts != want.contacts) {
                encode_list(&want, &mut b[a..a + LIST_SIZE]);
                b[LIST_COUNTS + n - 1] = (want.contacts.len() + 1) as u8;
            }
        }

        // DTMF contacts and APRS configurations: an edited one is rewritten, a number without one is erased.
        for n in 1..=MAX_DTMF {
            let a = DTMF_CONTACTS + (n - 1) * DTMF_SIZE;
            let have = decode_dtmf(n as u8, &b[a..a + DTMF_SIZE]);
            match (self.dtmf_contacts.iter().find(|d| usize::from(d.number) == n), have) {
                (Some(d), h) if h.as_ref().is_none_or(|h| h.name != d.name || h.code != d.code) => {
                    encode_dtmf(d, &mut b[a..a + DTMF_SIZE]);
                }
                (None, Some(_)) => b[a] = 0xFF,
                _ => {}
            }
        }
        for n in 1..=MAX_APRS {
            let a = APRS_BASE + (n - 1) * APRS_SIZE;
            let have = decode_aprs(n as u8, &b[a..a + APRS_SIZE]);
            match (self.aprs.iter().find(|p| usize::from(p.number) == n), have) {
                (Some(p), h) if h.as_ref().is_none_or(|h| !same_aprs(h, p)) => {
                    encode_aprs(p, &mut b[a..a + APRS_SIZE]);
                }
                (None, Some(_)) => b[a] = 0xFF,
                _ => {}
            }
        }

        encode_general(&self.general, b);
        // The modes' records after the records they name (channel-modes.md 2.4: the record first, then its tag).
        crate::modes::encode_modes(
            &crate::modes::ModesIn {
                channels: &self.channels,
                settings: &self.mode_settings,
                systems: &systems,
                orphans: &self.orphan_entries,
            },
            b,
            || crate::modes::channel_names(base.bytes()),
        );
        self.fix_lucz(b);
        img
    }

    /// The last used channel of each zone (7.3.10) must stay inside the zone; All Channels' must be a stored channel.
    fn fix_lucz(&self, b: &mut [u8]) {
        if &b[LUCZ..LUCZ + 4] != b"LUCZ" {
            return;
        }
        for (slot, z) in self.zones.iter().enumerate().take(MAX_ZONES) {
            if usize::from(b[LUCZ + 4 + slot]) >= z.channels.len() {
                b[LUCZ + 4 + slot] = 0;
            }
        }
        let all = u16::from(b[LUCZ + 72]) | (u16::from(b[LUCZ + 73]) << 8);
        if !self.channels.iter().any(|c| c.number == all + 1) {
            b[LUCZ + 72] = 0;
            b[LUCZ + 73] = 0;
        }
    }
}

fn put_channel(r: &mut [u8], c: &Channel) {
    let same = decode_channel(c.number, r).is_some_and(|h| same_channel(&h, c));
    if !same {
        encode_channel(c, r);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fields::get_tone;

    /// A synthetic image: one FM channel on bank 0, one zone holding it, one contact.
    fn sample() -> Image {
        let mut img = Image::blank();
        let b = img.bytes_mut();
        // channel 1: "Simplex", 146.520 MHz both ways, FM, wide, CTCSS 103.5, squelch 5
        b[bank_bitmap(0)] |= 1;
        let a = channel_addr(1);
        let r = &mut b[a..a + CH_SIZE];
        r.fill(0);
        r[..16].fill(0xFF);
        r[..7].copy_from_slice(b"Simplex");
        r[0x10..0x14].copy_from_slice(&[0x00, 0x20, 0x65, 0x14]);
        r[0x14..0x18].copy_from_slice(&[0x00, 0x20, 0x65, 0x14]);
        r[0x20..0x22].copy_from_slice(&0x1035u16.to_le_bytes());
        r[0x22..0x24].copy_from_slice(&0x8265u16.to_le_bytes());
        r[0x33] = 0x02;
        r[0x37] = 5;
        // zone slot 0 "Main" with channel 1
        b[ZONE_BITMAP] |= 1;
        let z = &mut b[ZONE_DATA..ZONE_DATA + 0xB0];
        z.fill(0);
        z[..16].fill(0xFF);
        z[..4].copy_from_slice(b"Main");
        z[16..18].copy_from_slice(&1u16.to_le_bytes());
        // contact 1: "Local" TG 2
        let c = &mut b[CONTACTS..CONTACTS + CONTACT_SIZE];
        c.fill(0xFF);
        c[..5].copy_from_slice(b"Local");
        c[5..16].fill(0xFF);
        c[0x10..0x14].copy_from_slice(&[0x00, 0x00, 0x00, 0x02]);
        c[0x14] = 0;
        c[0x17] = 0x01;
        img
    }

    #[test]
    fn decodes_the_records_of_a_small_image() {
        let cp = Codeplug::decode(&sample());
        assert_eq!(cp.channels.len(), 1);
        let ch = cp.channel(1).unwrap();
        assert_eq!(
            (ch.name.as_str(), ch.rx_hz, ch.tx_hz),
            ("Simplex", 146_520_000, 146_520_000)
        );
        assert_eq!((ch.mode, ch.bw, ch.squelch), (Mode::Fm, 25.0, 5));
        assert_eq!(
            (ch.rx_tone.as_str(), ch.tx_tone.as_str()),
            ("103.5", "D265N")
        );
        assert_eq!(cp.zones.len(), 1);
        assert_eq!(
            (cp.zones[0].name.as_str(), cp.zones[0].channels.clone()),
            ("Main", vec![1])
        );
        assert_eq!(cp.contacts.len(), 1);
        assert_eq!(
            (
                cp.contacts[0].name.as_str(),
                cp.contacts[0].id,
                cp.contacts[0].call_type
            ),
            ("Local", 2, CallType::Group)
        );
        assert!(cp.rx_lists.is_empty());
    }

    #[test]
    fn a_blank_image_has_nothing_in_use() {
        let cp = Codeplug::decode(&Image::blank());
        assert!(cp.channels.is_empty() && cp.zones.is_empty() && cp.contacts.is_empty());
        let _ = get_tone;
    }

    #[test]
    fn an_unedited_codeplug_encodes_to_the_same_bytes() {
        let img = sample();
        let out = Codeplug::decode(&img).encode(&img);
        assert_eq!(out.bytes(), img.bytes());
    }

    fn changed(a: &Image, b: &Image) -> Vec<usize> {
        a.bytes()
            .iter()
            .zip(b.bytes())
            .enumerate()
            .filter(|(_, (x, y))| x != y)
            .map(|(i, _)| i)
            .collect()
    }

    #[test]
    fn editing_a_channel_changes_only_its_record() {
        let img = sample();
        let mut cp = Codeplug::decode(&img);
        cp.channels[0].name = "Renamed".into();
        cp.channels[0].rx_tone = "D073N".into();
        let out = cp.encode(&img);
        let at = channel_addr(1);
        assert!(changed(&img, &out)
            .iter()
            .all(|&i| (at..at + CH_SIZE).contains(&i)));
        let back = Codeplug::decode(&out);
        assert_eq!(
            (
                back.channels[0].name.as_str(),
                back.channels[0].rx_tone.as_str()
            ),
            ("Renamed", "D073N")
        );
        assert_eq!(back.channels[0].tx_tone, "D265N");
    }

    #[test]
    fn vendor_bytes_survive_an_edit() {
        let mut img = sample();
        let a = channel_addr(1);
        img.bytes_mut()[a + 0x32] = 0x5A; // bytes no field covers
        img.bytes_mut()[a + 0x34] = 0xA5;
        let mut cp = Codeplug::decode(&img);
        cp.channels[0].squelch = 7;
        let out = cp.encode(&img);
        assert_eq!(
            (
                out.bytes()[a + 0x32],
                out.bytes()[a + 0x34],
                out.bytes()[a + 0x37]
            ),
            (0x5A, 0xA5, 7)
        );
    }

    #[test]
    fn adding_and_deleting_records() {
        let img = sample();
        let mut cp = Codeplug::decode(&img);
        // a new channel in slot 2, a new zone, a new contact, a new list
        let mut c2 = cp.channels[0].clone();
        c2.number = 2;
        c2.name = "Second".into();
        c2.rx_hz = 446_000_000;
        c2.tx_hz = 446_000_000;
        cp.channels.push(c2);
        cp.zones.push(Zone {
            slot: 1,
            name: "Other".into(),
            channels: vec![1, 2],
        });
        cp.contacts.push(Contact {
            number: 2,
            name: "Friend".into(),
            id: 3_100_999,
            call_type: CallType::Private,
            timeslot: 0,
        });
        cp.rx_lists.push(RxList {
            number: 1,
            name: "FAV".into(),
            contacts: vec![1, 2],
        });
        cp.general.radio_name = "N0CALL".into();
        cp.general.dmr_id = 3_100_123;
        let out = cp.encode(&img);
        let back = Codeplug::decode(&out);
        assert_eq!(back.channels.len(), 2);
        assert_eq!(back.channel(2).unwrap().name, "Second");
        assert_eq!(
            back.zones
                .iter()
                .map(|z| z.name.as_str())
                .collect::<Vec<_>>(),
            ["Main", "Other"]
        );
        assert_eq!(back.zones[1].channels, vec![1, 2]);
        assert_eq!(back.contacts[1].id, 3_100_999);
        assert_eq!(back.contacts[1].call_type, CallType::Private);
        assert_eq!(
            (
                back.rx_lists[0].name.as_str(),
                back.rx_lists[0].contacts.clone()
            ),
            ("FAV", vec![1, 2])
        );
        assert_eq!(
            (back.general.radio_name.as_str(), back.general.dmr_id),
            ("N0CALL", 3_100_123)
        );
        // and take them away again
        let mut cp = back;
        cp.channels.retain(|c| c.number != 2);
        cp.zones.pop();
        cp.contacts.retain(|c| c.number != 2);
        cp.rx_lists.clear();
        let gone = Codeplug::decode(&cp.encode(&out));
        assert_eq!(
            (
                gone.channels.len(),
                gone.zones.len(),
                gone.contacts.len(),
                gone.rx_lists.len()
            ),
            (1, 1, 1, 0)
        );
    }

    #[test]
    fn the_keyed_settings_round_trip_and_an_unedited_codeplug_leaves_the_area_alone() {
        use crate::kv::{self, Keys, SigRecord, KV_ADDR, KV_SIZE};
        let img = Image::blank();
        let mut cp = Codeplug::decode(&img);
        // Unedited: the same bytes, the area still erased.
        assert_eq!(cp.encode(&img).bytes(), img.bytes());
        let mut tone = SigRecord { dmr: 2, fm: 1, ..SigRecord::default() };
        tone.hz[0] = 1750;
        tone.ms[0] = 300;
        cp.general.keys = Keys {
            ars_server: Some(3_100_001),
            tot_warn_tone: Some(false),
            sig: [None, Some(tone), None],
            ..Keys::default()
        };
        let out = cp.encode(&img);
        assert_eq!(Codeplug::decode(&out).general.keys, cp.general.keys);
        // Only the settings sector differs from the image it started from.
        let diff: Vec<usize> = (0..out.bytes().len()).filter(|&i| out.bytes()[i] != img.bytes()[i]).collect();
        assert!(!diff.is_empty() && diff.iter().all(|&i| (KV_ADDR..KV_ADDR + KV_SIZE).contains(&i)));
        // Encoding the decoded codeplug again changes nothing.
        let again = Codeplug::decode(&out);
        assert_eq!(again.encode(&out).bytes(), out.bytes());
        // Taking a key away removes it and keeps the others.
        let mut cp = again;
        cp.general.keys.ars_server = None;
        let k = kv::decode_keys(cp.encode(&out).bytes());
        assert_eq!((k.ars_server, k.tot_warn_tone), (None, Some(false)));
    }
}
