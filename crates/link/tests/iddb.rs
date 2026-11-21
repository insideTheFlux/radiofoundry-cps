// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux

use rfcps_core::iddb::{self, Record, AREA1};
use rfcps_core::{Codeplug, Image};
use rfcps_link::fake::FakeRadio;
use rfcps_link::{LinkError, Radio};

const HEAD: usize = AREA1 / 4096 * 4096;

fn records(n: u32) -> Vec<Record> {
    (1..=n)
        .map(|i| Record {
            id: 3_100_000 + i,
            text: format!("Call{i} Name"),
        })
        .collect()
}

fn radio() -> Radio<FakeRadio> {
    let mut f = FakeRadio::new(Image::blank().bytes());
    f.serve_iddb = true;
    Radio::new(f)
}

#[test]
fn the_header_block_goes_first_erased_and_last_as_it_should_be() {
    let mut r = radio();
    let mut img = Image::blank();
    iddb::write_n(&mut img, &records(500), 16, usize::MAX).unwrap();
    let rep = r.write_iddb(&img, &mut |_, _| {}).unwrap();
    let w = r.port_mut().written_blocks.clone();
    assert_eq!(w.first(), Some(&HEAD));
    assert_eq!(w.last(), Some(&HEAD));
    assert_eq!(w.iter().filter(|&&a| a == HEAD).count(), 2);
    assert_eq!(rep.blocks, w.len());
    // the radio now holds the database exactly as the image has it
    let got = Image::from_bytes(r.port_mut().image());
    assert_eq!(iddb::read(got.bytes()).len(), 500);
    assert_eq!(
        &got.bytes()[AREA1..AREA1 + 0x20],
        &img.bytes()[AREA1..AREA1 + 0x20]
    );
}

#[test]
fn the_same_database_writes_nothing_and_a_change_rewrites_the_header() {
    let mut r = radio();
    let mut img = Image::blank();
    iddb::write_n(&mut img, &records(500), 16, usize::MAX).unwrap();
    r.write_iddb(&img, &mut |_, _| {}).unwrap();
    r.port_mut().written_blocks.clear();
    assert_eq!(r.write_iddb(&img, &mut |_, _| {}).unwrap().blocks, 0);
    assert!(r.port_mut().written_blocks.is_empty());
    // one more record: the header's block (the count) twice, plus whatever the record touches
    let mut img2 = img.clone();
    iddb::write_n(&mut img2, &records(501), 16, usize::MAX).unwrap();
    r.write_iddb(&img2, &mut |_, _| {}).unwrap();
    let w = &r.port_mut().written_blocks;
    assert_eq!((w.first(), w.last()), (Some(&HEAD), Some(&HEAD)));
}

#[test]
fn an_erase_leaves_no_database_and_the_codeplug_alone() {
    let mut r = radio();
    let mut img = Image::blank();
    iddb::write_n(&mut img, &records(50), 16, usize::MAX).unwrap();
    r.write_iddb(&img, &mut |_, _| {}).unwrap();
    iddb::erase(&mut img);
    r.write_iddb(&img, &mut |_, _| {}).unwrap();
    let got = Image::from_bytes(r.port_mut().image());
    assert!(iddb::read_header(got.bytes()).is_none());
    assert!(Codeplug::decode(&got).iddb.is_none());
}

#[test]
fn the_radios_bytes_after_the_database_are_kept() {
    let mut r = radio();
    let mut img = Image::blank();
    iddb::write_n(&mut img, &records(100), 16, usize::MAX).unwrap();
    let top = iddb::end(img.bytes());
    r.port_mut().poke(top + 8, 0x5A);
    r.write_iddb(&img, &mut |_, _| {}).unwrap();
    assert_eq!(r.port_mut().image()[top + 8], 0x5A);
}

#[test]
fn a_database_past_the_radios_image_is_refused_before_anything_is_written() {
    let mut f = FakeRadio::new(Image::blank().bytes());
    f.serve_iddb = true;
    f.info_layout = Some((1, 0x10000, 0x20000, 0xB0000));
    let mut r = Radio::new(f);
    let mut img = Image::blank();
    iddb::write_n(&mut img, &records(20000), 16, usize::MAX).unwrap();
    assert!(matches!(
        r.write_iddb(&img, &mut |_, _| {}),
        Err(LinkError::IddbTooBig(..))
    ));
    assert!(r.port_mut().written_blocks.is_empty());
}

#[test]
fn the_radio_says_whether_it_reads_a_compact_database() {
    let mut r = radio();
    assert!(!r.reads_compact_iddb().unwrap());
    r.port_mut().features = "FM,DMR,IDDB_COMPACT,CPS_FAST".into();
    assert!(r.reads_compact_iddb().unwrap());
    assert!(r.features().unwrap().contains(&"DMR".to_string()));
}

#[test]
fn a_compact_database_writes_and_reads_back_through_the_link() {
    let mut r = radio();
    let mut img = Image::blank();
    iddb::write(&mut img, &records(3000), iddb::Options { format: b'C', ..Default::default() }).unwrap();
    r.write_iddb(&img, &mut |_, _| {}).unwrap();
    let got = Image::from_bytes(r.port_mut().image());
    assert_eq!(iddb::read(got.bytes()), iddb::read(img.bytes()));
    assert_eq!(iddb::read(got.bytes()).len(), 3000);
    // a refresh writes only the pages that changed, with the header's block first and last
    let mut next = img.clone();
    let mut recs = records(3000);
    recs[10].text = "Changed Name".into();
    iddb::write(&mut next, &recs, iddb::Options { format: b'C', ..Default::default() }).unwrap();
    r.port_mut().written_blocks.clear();
    let rep = r.write_iddb(&next, &mut |_, _| {}).unwrap();
    assert!(rep.blocks < 12, "only the changed pages and the header's block twice: {}", rep.blocks);
    let w = &r.port_mut().written_blocks;
    assert_eq!((w.first(), w.last()), (Some(&HEAD), Some(&HEAD)));
}
