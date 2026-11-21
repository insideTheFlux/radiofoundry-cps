// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux

//! A radio whose INFO reply describes another image layout than the editor's is refused before anything is read or
//! written: the editor would take the wrong blocks.

use rfcps_core::Image;
use rfcps_link::fake::FakeRadio;
use rfcps_link::{LinkError, Radio};

const OK: (u8, u32, u32, u32) = (1, 0x10000, 0x20000, 0x120000);

fn radio(layout: (u8, u32, u32, u32)) -> Radio<FakeRadio> {
    let img = Image::blank();
    let mut fake = FakeRadio::new(img.bytes());
    fake.info_layout = Some(layout);
    Radio::new(fake)
}

#[test]
fn the_editors_layout_is_accepted() {
    let mut r = radio(OK);
    assert!(r.read_image(&mut |_, _| {}).is_ok());
}

#[test]
fn another_layout_is_refused_by_read_plan_and_write() {
    let cases = [
        ("version", (2, 0x10000, 0x20000, 0x120000)),
        ("eeprom size", (1, 0x20000, 0x20000, 0x120000)),
        ("flash start", (1, 0x10000, 0x40000, 0x120000)),
        ("image end", (1, 0x10000, 0x20000, 0x80000)),
    ];
    for (what, layout) in cases {
        let img = Image::blank();
        let mut r = radio(layout);
        assert!(
            matches!(r.read_image(&mut |_, _| {}), Err(LinkError::Layout(_))),
            "read, {what}"
        );
        assert!(
            matches!(r.plan_write(&img), Err(LinkError::Layout(_))),
            "plan, {what}"
        );
        assert!(
            matches!(r.verify(&img), Err(LinkError::Layout(_))),
            "verify, {what}"
        );
        let res = r.write_image(&img, &img, &mut |_, _| {});
        assert!(matches!(res, Err(LinkError::Layout(_))), "write, {what}");
        assert!(
            r.into_inner().written_blocks.is_empty(),
            "nothing written, {what}"
        );
    }
}

#[test]
fn the_refusal_says_what_differs() {
    let mut r = radio((1, 0x10000, 0x40000, 0x120000));
    let e = r.read_image(&mut |_, _| {}).unwrap_err().to_string();
    assert!(e.contains("flash image starts at 0x40000"), "{e}");
}
