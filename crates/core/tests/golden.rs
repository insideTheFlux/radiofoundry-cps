// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux

//! Compares the Rust decoder with RadioFoundry's web editor on a real radio image. Runs only when
//! `RFCPS_IMAGE` (the image) and `RFCPS_GOLDEN` (the web editor's decode of it, as JSON) are set; otherwise it
//! passes without checking anything, so it never needs a radio or personal data in this repository.

use rfcps_core::{records::Mode, Codeplug, Image};
use serde_json::Value;

#[test]
fn matches_the_web_editor_on_a_real_image() {
    let (Ok(img_path), Ok(gold_path)) =
        (std::env::var("RFCPS_IMAGE"), std::env::var("RFCPS_GOLDEN"))
    else {
        eprintln!("RFCPS_IMAGE / RFCPS_GOLDEN not set: skipped");
        return;
    };
    let image = Image::from_bytes(&std::fs::read(img_path).unwrap());
    let gold: Value = serde_json::from_slice(&std::fs::read(gold_path).unwrap()).unwrap();
    let cp = Codeplug::decode(&image);

    let g = &gold["general"];
    assert_eq!(cp.general.radio_name, g["radioName"].as_str().unwrap());
    assert_eq!(u64::from(cp.general.dmr_id), g["dmrId"].as_u64().unwrap());
    assert_eq!(
        u64::from(cp.general.group_hang),
        g["groupHang"].as_u64().unwrap()
    );
    assert_eq!(
        u64::from(cp.general.private_hang),
        g["privateHang"].as_u64().unwrap()
    );
    assert_eq!(cp.general.boot_line1, g["bootLine1"].as_str().unwrap());
    assert_eq!(cp.general.boot_line2, g["bootLine2"].as_str().unwrap());

    let chs = gold["channels"].as_array().unwrap();
    assert_eq!(cp.channels.len(), chs.len(), "channel count");
    for (c, j) in cp.channels.iter().zip(chs) {
        let at = format!("channel {} {}", c.number, c.name);
        assert_eq!(u64::from(c.number), j["number"].as_u64().unwrap(), "{at}");
        assert_eq!(c.name, j["name"].as_str().unwrap(), "{at}");
        assert_eq!(c.rx_hz, j["rxHz"].as_u64().unwrap(), "{at} rx");
        assert_eq!(c.tx_hz, j["txHz"].as_u64().unwrap(), "{at} tx");
        assert_eq!(c.mode == Mode::Dmr, j["mode"] == "dmr", "{at} mode");
        assert_eq!(
            u64::from(c.power),
            j["power"].as_u64().unwrap(),
            "{at} power"
        );
        assert_eq!(u64::from(c.tot), j["tot"].as_u64().unwrap(), "{at} tot");
        assert_eq!(c.rx_tone, j["rxTone"].as_str().unwrap(), "{at} rx tone");
        assert_eq!(c.tx_tone, j["txTone"].as_str().unwrap(), "{at} tx tone");
        assert_eq!(c.bw, j["bw"].as_f64().unwrap(), "{at} bw");
        assert_eq!(c.rx_only, j["rxOnly"].as_bool().unwrap(), "{at} rx only");
        assert_eq!(
            u64::from(c.squelch),
            j["squelch"].as_u64().unwrap(),
            "{at} squelch"
        );
        assert_eq!(
            u64::from(c.color_code),
            j["colorCode"].as_u64().unwrap(),
            "{at} cc"
        );
        assert_eq!(
            u64::from(c.timeslot),
            j["timeslot"].as_u64().unwrap(),
            "{at} ts"
        );
        assert_eq!(
            u64::from(c.dmr_id),
            j["dmrId"].as_u64().unwrap(),
            "{at} dmr id"
        );
        let contact = j["contact"].as_u64().unwrap_or(0);
        assert_eq!(u64::from(c.contact), contact, "{at} contact");
        assert_eq!(
            u64::from(c.rx_list),
            j["rxList"].as_u64().unwrap_or(0),
            "{at} rx list"
        );
    }

    let zs = gold["zones"].as_array().unwrap();
    assert_eq!(cp.zones.len(), zs.len(), "zone count");
    for (z, j) in cp.zones.iter().zip(zs) {
        assert_eq!(z.slot as u64, j["slot"].as_u64().unwrap());
        assert_eq!(z.name, j["name"].as_str().unwrap());
        let want: Vec<u64> = j["channels"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap())
            .collect();
        let got: Vec<u64> = z.channels.iter().map(|&n| u64::from(n)).collect();
        assert_eq!(got, want, "zone {}", z.name);
    }

    let cs = gold["contacts"].as_array().unwrap();
    assert_eq!(cp.contacts.len(), cs.len(), "contact count");
    for (c, j) in cp.contacts.iter().zip(cs) {
        assert_eq!(u64::from(c.number), j["number"].as_u64().unwrap());
        assert_eq!(c.name, j["name"].as_str().unwrap());
        assert_eq!(u64::from(c.id), j["id"].as_u64().unwrap());
        assert_eq!(u64::from(c.timeslot), j["timeslot"].as_u64().unwrap());
    }

    assert_eq!(
        cp.rx_lists.len(),
        gold["rxLists"].as_array().unwrap().len(),
        "rx list count"
    );
    eprintln!(
        "matched: {} channels, {} zones, {} contacts, {} rx lists",
        cp.channels.len(),
        cp.zones.len(),
        cp.contacts.len(),
        cp.rx_lists.len()
    );
}

/// Decoding a real image and encoding it again, unedited, gives the same bytes: nothing is lost or invented.
#[test]
fn a_real_image_round_trips_byte_for_byte() {
    let Ok(img_path) = std::env::var("RFCPS_IMAGE") else {
        eprintln!("RFCPS_IMAGE not set: skipped");
        return;
    };
    let image = Image::from_bytes(&std::fs::read(img_path).unwrap());
    let out = Codeplug::decode(&image).encode(&image);
    let diff: Vec<usize> = image
        .bytes()
        .iter()
        .zip(out.bytes())
        .enumerate()
        .filter(|(_, (a, b))| a != b)
        .map(|(i, _)| i)
        .collect();
    assert!(
        diff.is_empty(),
        "{} bytes differ, first at {:#x}",
        diff.len(),
        diff[0]
    );
    assert_eq!(image.len(), out.len());
}

/// The same edits made with the web editor's encoder and with this one give the same image, byte for byte.
/// `RFCPS_EDITED_JS` is the web editor's result (see `docs/testing.md`).
#[test]
fn edits_encode_like_the_web_editor() {
    use rfcps_core::records::{CallType, Contact};
    let (Ok(img_path), Ok(js_path)) = (
        std::env::var("RFCPS_IMAGE"),
        std::env::var("RFCPS_EDITED_JS"),
    ) else {
        eprintln!("RFCPS_IMAGE / RFCPS_EDITED_JS not set: skipped");
        return;
    };
    let image = Image::from_bytes(&std::fs::read(img_path).unwrap());
    let want = Image::from_bytes(&std::fs::read(js_path).unwrap());
    let mut cp = Codeplug::decode(&image);
    let ch = |cp: &mut Codeplug, n: u16| cp.channels.iter().position(|c| c.number == n).unwrap();
    let i = ch(&mut cp, 7);
    cp.channels[i].rx_tone = "D023I".into();
    let i = ch(&mut cp, 3);
    cp.channels[i].squelch = 3;
    let i = ch(&mut cp, 9);
    cp.channels[i].name = "Test_Chan".into();
    cp.channels[i].power = 3;
    let mut new = cp.channels[0].clone();
    new.number = 40;
    new.name = "NewFM".into();
    new.rx_hz = 146_520_000;
    new.tx_hz = 146_520_000;
    new.rx_tone = String::new();
    new.tx_tone = "88.5".into();
    new.bw = 12.5;
    new.color_code = 1;
    cp.channels.push(new);
    cp.contacts.push(Contact {
        number: 11,
        name: "Test".into(),
        id: 4242,
        call_type: CallType::Group,
        timeslot: 2,
    });
    cp.zones[2].name = "Biz".into();
    cp.zones[2].channels.push(40);
    cp.general.radio_name = "TESTCALL".into();
    cp.general.boot_line1 = "Hello".into();
    let got = cp.encode(&image);
    let diff: Vec<usize> = got
        .bytes()
        .iter()
        .zip(want.bytes())
        .enumerate()
        .filter(|(_, (a, b))| a != b)
        .map(|(i, _)| i)
        .collect();
    assert!(
        diff.is_empty(),
        "{} bytes differ from the web editor's, first at {:#x}",
        diff.len(),
        diff[0]
    );
    eprintln!("edits encode identically to the web editor");
}
