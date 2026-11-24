// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux

//! DMR AES-256 channels (kind 5) and systems (type 5), channel-modes.md 4. The expected bytes are what the web editor's
//! `mode_aes.js` wrote for the same edits (RadioFoundry, `encode()` of a model with one AES system, one key and one
//! channel), read from the image's settings area.

use rfcps_core::kv;
use rfcps_core::modes::{self, AesChan, Params};
use rfcps_core::problems::{validate, Level};
use rfcps_core::records::{Channel, Mode};
use rfcps_core::systems::{self, AesKey, System, SystemFields, SYSTEM_AES, SYSTEM_NXDN};
use rfcps_core::{Codeplug, Image};

const KEY_BYTES: [u8; 32] = {
    let mut k = [0u8; 32];
    let mut i = 0;
    while i < 32 {
        k[i] = (i as u8).wrapping_mul(7).wrapping_add(3);
        i += 1;
    }
    k
};

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn value(img: &Image, key: u16) -> Option<String> {
    kv::get_value(&kv::decode_area(img.bytes()).entries, key).map(hex)
}

/// One AES system "Fire AES" (network 7) with key 9, and channel 1 "AES One" on 146.52 MHz picking it.
fn fixture() -> (Codeplug, Image) {
    let base = Image::blank();
    let mut cp = Codeplug::decode(&base);
    let mut s = System::new(SYSTEM_AES, "Fire AES");
    s.number = 1;
    s.fields = SystemFields::Aes { network: 7 };
    s.aes_keys.push(AesKey { slot: None, id: 9, key: KEY_BYTES, flags: 0 });
    cp.systems.push(s);
    let mut c = Channel::new(1, 146_520_000, 146_520_000);
    c.name = "AES One".into();
    cp.channels.push(c);
    let systems = cp.systems.clone();
    modes::set_kind(&mut cp.channels[0], modes::KIND_AES, Mode::Dmr, &systems);
    cp.channels[0].tagged.as_mut().unwrap().params = Params::Aes(AesChan { key_id: Some(9) });
    (cp, base)
}

#[test]
fn the_bytes_are_the_web_editors() {
    let (cp, base) = fixture();
    let img = cp.encode(&base);
    assert_eq!(
        value(&img, systems::SYSTEM_KEY).unwrap(),
        "0502010100000000000000004669726520414553ffffffffffffffff07",
        "the system record"
    );
    assert_eq!(
        value(&img, systems::TRAFFIC_KEY).unwrap(),
        "0025090000030a11181f262d343b424950575e656c737a81888f969da4abb2b9c0c7ced5dc",
        "the traffic key"
    );
    assert_eq!(value(&img, modes::TAG_KEY).unwrap(), "05002369012509", "the channel's tag");
}

#[test]
fn it_reads_back_and_rewriting_changes_nothing() {
    let (cp, base) = fixture();
    let img = cp.encode(&base);
    let back = Codeplug::decode(&img);
    let s = &back.systems[0];
    assert_eq!((s.type_id, s.name.as_str(), s.tx), (SYSTEM_AES, "Fire AES", true));
    assert_eq!(s.fields, SystemFields::Aes { network: 7 });
    assert_eq!(s.aes_keys.len(), 1);
    assert_eq!((s.aes_keys[0].id, s.aes_keys[0].key, s.aes_keys[0].slot), (9, KEY_BYTES, Some(systems::TRAFFIC_KEY)));
    let c = back.channel(1).unwrap();
    assert_eq!(c.kind(), modes::KIND_AES);
    let t = c.tagged.as_ref().unwrap();
    assert_eq!((t.system, t.tx, t.base_own), (0, true, false));
    assert_eq!(t.params, Params::Aes(AesChan { key_id: Some(9) }));
    // the record under the tag is DMR and receive only: an older reader sees a receive-only DMR channel
    assert_eq!((c.mode, c.rx_only), (Mode::Dmr, true));
    assert_eq!(back.encode(&img).bytes(), img.bytes(), "re-encoding the decoded image changes nothing");
}

#[test]
fn keys_come_and_go_with_their_system() {
    let (cp, base) = fixture();
    let one = cp.encode(&base);
    // a second key takes the next slot; the first keeps its own
    let mut cp = Codeplug::decode(&one);
    cp.systems[0].aes_keys.push(AesKey { slot: None, id: 10, key: [0x55; 32], flags: 0 });
    let two = cp.encode(&one);
    assert_eq!(value(&two, systems::TRAFFIC_KEY), value(&one, systems::TRAFFIC_KEY));
    assert!(value(&two, systems::TRAFFIC_KEY + 1).unwrap().starts_with("0025 0a00 00".replace(' ', "").as_str()));
    // removing the first leaves the second at its slot
    let mut cp = Codeplug::decode(&two);
    cp.systems[0].aes_keys.remove(0);
    let gone = cp.encode(&two);
    assert!(value(&gone, systems::TRAFFIC_KEY).is_none());
    assert!(value(&gone, systems::TRAFFIC_KEY + 1).is_some());
    // deleting the system takes its keys
    let mut cp = Codeplug::decode(&two);
    cp.systems.clear();
    cp.channels[0].tagged = None;
    let none = cp.encode(&two);
    assert!(value(&none, systems::SYSTEM_KEY).is_none());
    assert!(value(&none, systems::TRAFFIC_KEY).is_none() && value(&none, systems::TRAFFIC_KEY + 1).is_none());
}

#[test]
fn a_key_record_this_editor_does_not_read_is_kept() {
    let (cp, base) = fixture();
    let img = cp.encode(&base);
    // another algorithm's record of the same system, in the next slot
    let mut other = vec![0u8, 0x26, 4, 0, 0];
    other.extend_from_slice(&[0xAA; 32]);
    let mut b = img.clone();
    let entries = kv::entries_for_write(b.bytes_mut()).unwrap();
    let with = kv::set_value(&entries, systems::TRAFFIC_KEY + 1, &other);
    assert!(kv::put_entries(b.bytes_mut(), &entries, &with));
    let cp = Codeplug::decode(&b);
    assert_eq!(cp.systems[0].aes_keys.len(), 1);
    let again = cp.encode(&b);
    assert_eq!(value(&again, systems::TRAFFIC_KEY + 1).unwrap(), hex(&other));
}

#[test]
fn nxdn_and_aes_keys_share_the_family() {
    let (mut cp, base) = fixture();
    let mut n = System::new(SYSTEM_NXDN, "NXDN sys");
    n.number = 2;
    n.traffic_keys.push(systems::TrafficKey { slot: None, id: 1, seed: 99, flags: 0 });
    cp.systems.push(n);
    let img = cp.encode(&base);
    let back = Codeplug::decode(&img);
    assert_eq!(back.systems[0].aes_keys.len(), 1);
    assert_eq!(back.systems[1].traffic_keys.len(), 1);
    assert_eq!(back.encode(&img).bytes(), img.bytes());
}

#[test]
fn the_keys_are_secrets_and_never_in_a_debug_line() {
    let (cp, base) = fixture();
    let img = cp.encode(&base);
    assert_eq!(kv::count_secrets(img.bytes()), 1);
    let mut stripped = img.clone();
    kv::without_secrets(stripped.bytes_mut());
    assert!(value(&stripped, systems::TRAFFIC_KEY).is_none());
    assert!(value(&stripped, modes::TAG_KEY).is_some(), "the channel's tag is not a secret");
    let shown = format!("{:?}", cp.systems[0].aes_keys[0]);
    assert!(shown.contains("hidden"), "{shown}");
    assert!(!shown.contains("0x") && !shown.contains(&format!("{:?}", KEY_BYTES)), "{shown}");
}

#[test]
fn problems_name_a_missing_key_a_missing_system_and_encryption_on_an_amateur_frequency() {
    let (mut cp, _) = fixture();
    let msgs = |cp: &Codeplug| -> Vec<(Level, String)> { validate(cp, None).into_iter().map(|p| (p.level, p.message)).collect() };
    // 146.52 MHz with a key picked: the amateur warning, nothing else
    let m = msgs(&cp);
    assert!(m.iter().any(|(l, s)| *l == Level::Warning && s.contains("amateur frequency")), "{m:?}");
    assert!(!m.iter().any(|(_, s)| s.contains("AES key")), "{m:?}");
    // outside the amateur bands, no warning
    cp.channels[0].rx_hz = 462_562_500;
    cp.channels[0].tx_hz = 462_562_500;
    assert!(!msgs(&cp).iter().any(|(_, s)| s.contains("amateur frequency")));
    // receive only: none either
    cp.channels[0].tx_hz = 146_520_000;
    cp.channels[0].tagged.as_mut().unwrap().tx = false;
    assert!(!msgs(&cp).iter().any(|(_, s)| s.contains("amateur frequency")));
    // a key its system does not hold
    cp.channels[0].tagged.as_mut().unwrap().params = Params::Aes(AesChan { key_id: Some(3) });
    assert!(msgs(&cp).iter().any(|(l, s)| *l == Level::Warning && s.contains("AES key")));
    // no key at all
    cp.channels[0].tagged.as_mut().unwrap().params = Params::Aes(AesChan { key_id: None });
    assert!(msgs(&cp).iter().any(|(_, s)| s.contains("AES key")));
    // no system
    cp.channels[0].tagged.as_mut().unwrap().system = 0xFF;
    assert!(msgs(&cp).iter().any(|(l, s)| *l == Level::Error && s.contains("no system")));
    // another type of system
    let mut other = System::new(SYSTEM_NXDN, "N");
    other.number = 2;
    cp.systems.push(other);
    cp.channels[0].tagged.as_mut().unwrap().system = 1;
    assert!(msgs(&cp).iter().any(|(l, s)| *l == Level::Error && s.contains("another type")));
}

#[test]
fn a_new_aes_channel_starts_on_the_first_aes_system_and_transmits() {
    let (mut cp, _) = fixture();
    let mut c = Channel::new(2, 446_000_000, 446_000_000);
    c.rx_only = false;
    cp.channels.push(c);
    let systems = cp.systems.clone();
    modes::set_kind(&mut cp.channels[1], modes::KIND_AES, Mode::Dmr, &systems);
    let t = cp.channels[1].tagged.as_ref().unwrap();
    assert_eq!((t.system, t.tx), (0, true));
    assert_eq!(t.params, Params::Aes(AesChan { key_id: None }));
    assert!(modes::own_freqs(modes::KIND_AES));
    assert_eq!(modes::kind_name(modes::KIND_AES), "AES");
}
