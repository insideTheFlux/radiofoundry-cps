// SPDX-License-Identifier: MIT
// Copyright (c) 2026 The RadioFoundry Authors
// Copyright (c) 2026 insideTheFlux
//
// A whole codeplug decoded from an image. Ported from `decode()` in RadioFoundry's
// `site/static/js/codeplug/model.js`. Read-only for now.

use crate::image::Image;
use crate::records::*;

#[derive(Clone, Debug)]
pub struct Codeplug {
    pub general: General,
    pub channels: Vec<Channel>,
    pub zones: Vec<Zone>,
    pub contacts: Vec<Contact>,
    pub rx_lists: Vec<RxList>,
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
        let mut channels = Vec::new();
        for n in 1..=MAX_CHANNELS {
            if !channel_used(b, n) {
                continue;
            }
            let a = channel_addr(n);
            if let Some(c) = decode_channel(n as u16, &b[a..a + CH_SIZE]) {
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
        Self {
            general: decode_general(b),
            channels,
            zones,
            contacts,
            rx_lists,
        }
    }

    pub fn channel(&self, number: u16) -> Option<&Channel> {
        self.channels.iter().find(|c| c.number == number)
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
}
