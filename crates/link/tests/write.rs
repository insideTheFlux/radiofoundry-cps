// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux

use rfcps_core::fields::put_freq;
use rfcps_core::records::{blank_channel, channel_addr, decode_channel, CH_SIZE};
use rfcps_core::{Codeplug, Image};
use rfcps_link::fake::FakeRadio;
use rfcps_link::{LinkError, Radio};

/// An image with a few channels, as a radio would hold it.
fn image() -> Image {
    let mut cp = Codeplug::decode(&Image::blank());
    for n in 1..=3u16 {
        let mut b = blank_channel();
        put_freq(&mut b, 0x10, 146_000_000 + u64::from(n) * 10_000);
        put_freq(&mut b, 0x14, 146_000_000 + u64::from(n) * 10_000);
        let mut c = decode_channel(n, &b).unwrap();
        c.name = format!("Chan {n}");
        cp.channels.push(c);
    }
    cp.general.radio_name = "N0CALL".into();
    cp.general.dmr_id = 3_100_123;
    cp.encode(&Image::blank())
}

/// A radio holding `base`, the image read from it, and a codeplug to edit.
fn setup() -> (Radio<FakeRadio>, Image, Codeplug) {
    let base = image();
    let mut radio = Radio::new(FakeRadio::new(base.bytes()));
    let read = radio.read_image(&mut |_, _| {}).unwrap();
    let cp = Codeplug::decode(&read);
    (radio, read, cp)
}

fn fake(radio: &mut Radio<FakeRadio>) -> &mut FakeRadio {
    radio.port_mut()
}

#[test]
fn an_edit_writes_only_the_block_it_changes() {
    let (mut radio, base, mut cp) = setup();
    cp.channels[1].name = "Renamed".into();
    let edited = cp.encode(&base);
    let report = radio.write_image(&base, &edited, &mut |_, _| {}).unwrap();
    assert_eq!(report.blocks, 1);
    assert_eq!(
        fake(&mut radio).written_blocks,
        vec![channel_addr(2) / 4096 * 4096]
    );
    let now = Codeplug::decode(&Image::from_bytes(fake(&mut radio).image()));
    assert_eq!(now.channels[1].name, "Renamed");
    assert_eq!(now.channels[0].name, "Chan 1");
    assert!(radio.verify(&edited).unwrap().is_empty());
}

#[test]
fn nothing_changed_writes_nothing() {
    let (mut radio, base, cp) = setup();
    let same = cp.encode(&base);
    let report = radio.write_image(&base, &same, &mut |_, _| {}).unwrap();
    assert_eq!((report.blocks, report.stream_bytes), (0, 0));
    assert!(fake(&mut radio).written_blocks.is_empty());
}

#[test]
fn a_radio_that_changed_since_it_was_read_is_not_written() {
    let (mut radio, base, mut cp) = setup();
    // someone edits a channel on the radio itself
    let a = channel_addr(3) + CH_SIZE - 1;
    let old = base.bytes()[a];
    fake(&mut radio).poke(a, old ^ 0x01);
    cp.channels[0].name = "Mine".into();
    let edited = cp.encode(&base);
    match radio.write_image(&base, &edited, &mut |_, _| {}) {
        Err(LinkError::RadioChanged(n)) => assert!(n >= 1),
        other => panic!("{other:?}"),
    }
    assert!(fake(&mut radio).written_blocks.is_empty());
}

#[test]
fn the_radios_own_bytes_outside_the_regions_are_kept() {
    let (mut radio, base, mut cp) = setup();
    // block 0 holds codeplug data from 0x80 on and bytes the codeplug doesn't cover before it
    fake(&mut radio).poke(0x10, 0x55);
    fake(&mut radio).poke(0x6F00, 0x77);
    cp.general.radio_name = "NEWNAME".into();
    let edited = cp.encode(&base);
    let report = radio.write_image(&base, &edited, &mut |_, _| {}).unwrap();
    assert_eq!(report.blocks, 1);
    assert_eq!(fake(&mut radio).image()[0x10], 0x55);
    assert_eq!(fake(&mut radio).image()[0x6F00], 0x77);
    let now = Codeplug::decode(&Image::from_bytes(fake(&mut radio).image()));
    assert_eq!(now.general.radio_name, "NEWNAME");
}

#[test]
fn a_block_refused_once_is_sent_again() {
    let (mut radio, base, mut cp) = setup();
    cp.channels[0].name = "Again".into();
    let edited = cp.encode(&base);
    fake(&mut radio).refuse_next_end = Some(8); // a corrupt stream: send the block again
    let report = radio.write_image(&base, &edited, &mut |_, _| {}).unwrap();
    assert_eq!(report.blocks, 1);
    assert_eq!(
        Codeplug::decode(&Image::from_bytes(fake(&mut radio).image())).channels[0].name,
        "Again"
    );
}

#[test]
fn a_refusal_a_resend_cannot_fix_stops_the_write() {
    let (mut radio, base, mut cp) = setup();
    cp.channels[0].name = "Never".into();
    let edited = cp.encode(&base);
    fake(&mut radio).refuse_next_end = Some(9); // a flash failure
    match radio.write_image(&base, &edited, &mut |_, _| {}) {
        Err(LinkError::Status {
            status: 7,
            reason: Some(9),
        }) => {}
        other => panic!("{other:?}"),
    }
    assert!(fake(&mut radio).written_blocks.is_empty());
    // the session was ended: a new write works
    assert_eq!(
        radio
            .write_image(&base, &edited, &mut |_, _| {})
            .unwrap()
            .blocks,
        1
    );
}

#[test]
fn a_staged_write_lands_at_the_commit() {
    let base = image();
    let mut f = FakeRadio::new(base.bytes());
    f.stage_max = 8;
    let mut radio = Radio::new(f);
    let read = radio.read_image(&mut |_, _| {}).unwrap();
    let mut cp = Codeplug::decode(&read);
    cp.channels[2].name = "Staged".into();
    cp.general.radio_name = "STAGED".into();
    let edited = cp.encode(&read);
    let report = radio.write_image(&read, &edited, &mut |_, _| {}).unwrap();
    assert_eq!(report.blocks, 2);
    let now = Codeplug::decode(&Image::from_bytes(fake(&mut radio).image()));
    assert_eq!(
        (
            now.channels[2].name.as_str(),
            now.general.radio_name.as_str()
        ),
        ("Staged", "STAGED")
    );
}

#[test]
fn verify_reports_what_the_radio_does_not_hold() {
    let (mut radio, base, mut cp) = setup();
    cp.channels[0].name = "Not written".into();
    let edited = cp.encode(&base);
    assert_eq!(radio.verify(&edited).unwrap().len(), 1);
    assert!(radio.verify(&base).unwrap().is_empty());
}

// --- The settings area (codeplug-settings.md) ---

use rfcps_core::kv::{self, AreaState, Keys, KV_ADDR, KV_SIZE};

/// `image()` with some keys in its settings area, as a radio that has been set up would hold it.
fn image_with_keys(keys: &Keys) -> Image {
    let mut img = image();
    assert!(kv::put_keys(img.bytes_mut(), keys));
    img
}

fn setup_keys(keys: &Keys) -> (Radio<FakeRadio>, Image, Codeplug) {
    let mut radio = Radio::new(FakeRadio::new(image_with_keys(keys).bytes()));
    let read = radio.read_image(&mut |_, _| {}).unwrap();
    let cp = Codeplug::decode(&read);
    (radio, read, cp)
}

fn radio_keys(radio: &mut Radio<FakeRadio>) -> Keys {
    kv::decode_keys(fake(radio).image())
}

#[test]
fn a_key_edit_writes_only_the_settings_block() {
    let (mut radio, base, mut cp) = setup();
    assert_eq!(kv::decode_area(base.bytes()).state, AreaState::Empty);
    cp.general.keys.ars_server = Some(3_100_001);
    cp.general.keys.battery_tone = Some(false);
    let edited = cp.encode(&base);
    let report = radio.write_image(&base, &edited, &mut |_, _| {}).unwrap();
    assert_eq!(report.blocks, 1);
    assert_eq!(fake(&mut radio).written_blocks, vec![KV_ADDR]);
    let k = radio_keys(&mut radio);
    assert_eq!((k.ars_server, k.battery_tone), (Some(3_100_001), Some(false)));
    assert!(radio.verify(&edited).unwrap().is_empty());
    // And nothing else on the radio moved.
    let now = fake(&mut radio).image().to_vec();
    let diff: Vec<usize> = (0..now.len()).filter(|&i| now[i] != base.bytes()[i]).collect();
    assert!(diff.iter().all(|&i| (KV_ADDR..KV_ADDR + KV_SIZE).contains(&i)));
}

#[test]
fn an_edit_that_leaves_the_keys_alone_never_writes_the_area() {
    let keys = Keys { ars_server: Some(5), ..Keys::default() };
    let (mut radio, base, mut cp) = setup_keys(&keys);
    cp.channels[1].name = "Renamed".into();
    let edited = cp.encode(&base);
    radio.write_image(&base, &edited, &mut |_, _| {}).unwrap();
    assert_eq!(fake(&mut radio).written_blocks, vec![channel_addr(2) / 4096 * 4096]);
    assert_eq!(radio_keys(&mut radio), keys);
}

#[test]
fn a_key_changed_on_the_radio_since_the_read_is_kept() {
    let keys = Keys { ars_server: Some(5), ..Keys::default() };
    let (mut radio, base, mut cp) = setup_keys(&keys);
    // The radio's own menu sets the IP gateway and a tone after the read: its area changes.
    let mut now = fake(&mut radio).image().to_vec();
    let theirs = Keys { ars_server: Some(5), ip_gateway: Some(9), tx_deny_tone: Some(true), ..Keys::default() };
    assert!(kv::put_keys(&mut now, &theirs));
    for (i, &v) in now.iter().enumerate().skip(KV_ADDR).take(KV_SIZE) {
        fake(&mut radio).poke(i, v);
    }
    // The edit sets the ARS server and renames a channel.
    cp.general.keys.ars_server = Some(77);
    cp.channels[0].name = "Mine".into();
    let edited = cp.encode(&base);
    radio.write_image(&base, &edited, &mut |_, _| {}).unwrap();
    let k = radio_keys(&mut radio);
    assert_eq!(k.ars_server, Some(77), "the edit");
    assert_eq!(k.ip_gateway, Some(9), "the radio's own change kept");
    assert_eq!(k.tx_deny_tone, Some(true), "the radio's own change kept");
    let c = Codeplug::decode(&Image::from_bytes(fake(&mut radio).image()));
    assert_eq!(c.channels[0].name, "Mine");
}

#[test]
fn an_edit_with_no_key_change_does_not_undo_a_key_set_on_the_radio() {
    let (mut radio, base, mut cp) = setup();
    let mut now = fake(&mut radio).image().to_vec();
    assert!(kv::put_keys(&mut now, &Keys { ip_gateway: Some(9), ..Keys::default() }));
    for (i, &v) in now.iter().enumerate().skip(KV_ADDR).take(KV_SIZE) {
        fake(&mut radio).poke(i, v);
    }
    cp.channels[0].name = "Mine".into();
    let edited = cp.encode(&base);
    radio.write_image(&base, &edited, &mut |_, _| {}).unwrap();
    assert_eq!(radio_keys(&mut radio).ip_gateway, Some(9));
}

#[test]
fn an_area_of_a_newer_version_is_never_overwritten() {
    let keys = Keys { ars_server: Some(5), ..Keys::default() };
    let (mut radio, base, mut cp) = setup_keys(&keys);
    fake(&mut radio).poke(KV_ADDR + 4, 2); // a later version's area
    let sector = fake(&mut radio).image()[KV_ADDR..KV_ADDR + KV_SIZE].to_vec();
    cp.general.keys.ars_server = Some(99); // the edit is lost, by design
    cp.channels[0].name = "Mine".into();
    let edited = cp.encode(&base);
    radio.write_image(&base, &edited, &mut |_, _| {}).unwrap();
    assert_eq!(fake(&mut radio).image()[KV_ADDR..KV_ADDR + KV_SIZE], sector[..]);
    let c = Codeplug::decode(&Image::from_bytes(fake(&mut radio).image()));
    assert_eq!(c.channels[0].name, "Mine");
}

#[test]
fn an_image_without_an_area_does_not_erase_the_radios_keys() {
    let keys = Keys { ars_server: Some(5), call_end_tone: Some(false), ..Keys::default() };
    let (mut radio, _read, _cp) = setup_keys(&keys);
    // A file from another tool: the same codeplug with 0xFF where the area goes.
    let base = image();
    let mut cp = Codeplug::decode(&base);
    cp.channels[0].name = "FromFile".into();
    let file = cp.encode(&base);
    assert_eq!(kv::decode_area(file.bytes()).state, AreaState::Empty);
    radio.write_image(&base, &file, &mut |_, _| {}).unwrap();
    assert_eq!(radio_keys(&mut radio), keys);
}
