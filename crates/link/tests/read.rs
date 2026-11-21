// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux

use rfcps_core::image::{read_blocks, BLOCK};
use rfcps_core::records::{channel_addr, CH_SIZE};
use rfcps_core::{Codeplug, Image};
use rfcps_link::fake::FakeRadio;
use rfcps_link::{Block, LinkError, Radio};

/// An image with one channel on it, so the blocks are not all blank.
fn image() -> Image {
    let mut img = Image::blank();
    let b = img.bytes_mut();
    b[rfcps_core::records::bank_bitmap(0)] |= 1;
    let a = channel_addr(1);
    let r = &mut b[a..a + CH_SIZE];
    r.fill(0);
    r[..16].fill(0xFF);
    r[..4].copy_from_slice(b"Test");
    r[0x10..0x14].copy_from_slice(&[0x00, 0x20, 0x65, 0x14]);
    r[0x14..0x18].copy_from_slice(&[0x00, 0x20, 0x65, 0x14]);
    r[0x20..0x24].fill(0xFF);
    img
}

#[test]
fn ping_and_info() {
    let mut radio = Radio::new(FakeRadio::new(image().bytes()));
    radio.ping().unwrap();
    let info = radio.codeplug_info().unwrap();
    assert_eq!(
        (
            info.version,
            info.block,
            info.sums_per_request,
            info.stream_per_read
        ),
        (1, 4096, 16, 1000)
    );
    assert_eq!(
        (info.eeprom_size, info.flash_start, info.eeprom_base),
        (0x10000, 0x20000, 0x700000)
    );
}

#[test]
fn sums_match_the_blocks() {
    let img = image();
    let mut radio = Radio::new(FakeRadio::new(img.bytes()));
    let sums = radio.codeplug_sums(0x3000, 4).unwrap();
    for (i, crc) in sums.crcs.iter().enumerate() {
        let a = 0x3000 + i * BLOCK;
        assert_eq!(*crc, crc32fast::hash(&img.bytes()[a..a + BLOCK]));
    }
}

#[test]
fn reads_the_image_the_editor_needs() {
    let img = image();
    let mut radio = Radio::new(FakeRadio::new(img.bytes()));
    let mut steps = 0;
    let got = radio.read_image(&mut |_, _| steps += 1).unwrap();
    assert_eq!(steps, read_blocks().len() + 1);
    for addr in read_blocks() {
        assert_eq!(
            &got.bytes()[addr..addr + BLOCK],
            &img.bytes()[addr..addr + BLOCK],
            "block {addr:#x}"
        );
    }
    // and the codeplug decodes from it
    let cp = Codeplug::decode(&got);
    assert_eq!(cp.channels.len(), 1);
    assert_eq!(cp.channels[0].name, "Test");
}

#[test]
fn a_blank_block_reads_as_blank() {
    let mut radio = Radio::new(FakeRadio::new(image().bytes()));
    assert_eq!(radio.codeplug_read_block(0xE000).unwrap(), Block::Blank);
}

#[test]
fn a_block_with_a_bad_crc_is_read_again() {
    let img = image();
    let mut fake = FakeRadio::new(img.bytes());
    fake.corrupt_next_read = true;
    let mut radio = Radio::new(fake);
    let addr = channel_addr(1) / BLOCK * BLOCK;
    match radio.codeplug_read_block(addr as u32).unwrap() {
        Block::Data(d) => assert_eq!(d, &img.bytes()[addr..addr + BLOCK]),
        Block::Blank => panic!("not blank"),
    }
}

#[test]
fn a_block_outside_the_image_is_refused() {
    let mut radio = Radio::new(FakeRadio::new(image().bytes()));
    match radio.codeplug_read_block(0x10000) {
        Err(LinkError::Status {
            status: 7,
            reason: Some(1),
        }) => {}
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_read_brings_the_id_database_header() {
    use rfcps_core::iddb::{write_n, Record};
    let mut img = image();
    let recs = [Record { id: 3100001, text: "Test One".into() }];
    write_n(&mut img, &recs, 16, usize::MAX).unwrap();
    let mut radio = Radio::new(FakeRadio::new(img.bytes()));
    let got = radio.read_image(&mut |_, _| {}).unwrap();
    let h = Codeplug::decode(&got).iddb.expect("a header");
    assert_eq!((h.format, h.count), (b'N', 1));
    // without a database there is none
    let mut radio = Radio::new(FakeRadio::new(image().bytes()));
    let got = radio.read_image(&mut |_, _| {}).unwrap();
    assert!(Codeplug::decode(&got).iddb.is_none());
}
