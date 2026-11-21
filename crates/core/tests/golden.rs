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

/// The stock DM-32UV import gives what the web editor's gives, except that DCS tones are read as BCD digits (the web
/// editor's importer reads them as a decimal number: a known bug, fixed in RadioFoundry PR #776).
/// `RFCPS_STOCK_DATA` is a stock radio's data flash, `RFCPS_STOCK_GOLDEN` the web editor's import of it (JSON).
#[test]
fn the_stock_import_matches_the_web_editor() {
    use rfcps_core::stock_dm32;
    let (Ok(data_path), Ok(gold_path)) = (
        std::env::var("RFCPS_STOCK_DATA"),
        std::env::var("RFCPS_STOCK_GOLDEN"),
    ) else {
        eprintln!("RFCPS_STOCK_DATA / RFCPS_STOCK_GOLDEN not set: skipped");
        return;
    };
    let data = std::fs::read(data_path).unwrap();
    let gold: Value = serde_json::from_slice(&std::fs::read(gold_path).unwrap()).unwrap();
    assert!(stock_dm32::detect(&data));
    let mut cp = Codeplug::decode(&Image::blank());
    let report = stock_dm32::import_stock(&mut cp, &data).unwrap();
    assert_eq!(
        report.channels as u64,
        gold["counts"]["channels"].as_u64().unwrap()
    );
    assert_eq!(
        report.contacts as u64,
        gold["counts"]["contacts"].as_u64().unwrap()
    );
    assert_eq!(
        report.rx_lists as u64,
        gold["counts"]["rxLists"].as_u64().unwrap()
    );
    assert_eq!(
        report.zones as u64,
        gold["counts"]["zones"].as_u64().unwrap()
    );
    assert_eq!(
        u64::from(cp.general.dmr_id),
        gold["general"]["dmrId"].as_u64().unwrap()
    );
    assert_eq!(
        cp.general.radio_name,
        gold["general"]["radioName"].as_str().unwrap()
    );
    let mut dcs = 0;
    for (c, j) in cp.channels.iter().zip(gold["channels"].as_array().unwrap()) {
        let at = format!("channel {} {}", c.number, c.name);
        assert_eq!(c.name, j["name"].as_str().unwrap(), "{at}");
        assert_eq!(c.rx_hz, j["rxHz"].as_u64().unwrap(), "{at}");
        assert_eq!(c.tx_hz, j["txHz"].as_u64().unwrap(), "{at}");
        assert_eq!(c.mode == Mode::Dmr, j["mode"] == "dmr", "{at}");
        assert_eq!(u64::from(c.power), j["power"].as_u64().unwrap(), "{at}");
        assert_eq!(c.bw, j["bw"].as_f64().unwrap(), "{at}");
        assert_eq!(
            u64::from(c.color_code),
            j["colorCode"].as_u64().unwrap(),
            "{at}"
        );
        assert_eq!(
            u64::from(c.timeslot),
            j["timeslot"].as_u64().unwrap(),
            "{at}"
        );
        assert_eq!(
            u64::from(c.contact),
            j["contact"].as_u64().unwrap_or(0),
            "{at}"
        );
        assert_eq!(
            u64::from(c.rx_list),
            j["rxList"].as_u64().unwrap_or(0),
            "{at}"
        );
        for (mine, theirs) in [(&c.rx_tone, &j["rxTone"]), (&c.tx_tone, &j["txTone"])] {
            let theirs = theirs.as_str().unwrap();
            if theirs.starts_with('D') {
                dcs += 1;
                assert!(
                    mine.starts_with('D') && mine != theirs,
                    "{at}: DCS read as BCD ({mine}), not decimal ({theirs})"
                );
            } else {
                assert_eq!(mine, theirs, "{at}");
            }
        }
    }
    for (z, j) in cp.zones.iter().zip(gold["zones"].as_array().unwrap()) {
        assert_eq!(z.name, j["name"].as_str().unwrap());
        let want: Vec<u64> = j["channels"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap())
            .collect();
        assert_eq!(
            z.channels.iter().map(|&n| u64::from(n)).collect::<Vec<_>>(),
            want
        );
    }
    for (c, j) in cp.contacts.iter().zip(gold["contacts"].as_array().unwrap()) {
        assert_eq!(
            (c.name.as_str(), u64::from(c.id)),
            (j["name"].as_str().unwrap(), j["id"].as_u64().unwrap())
        );
    }
    eprintln!(
        "stock import matches the web editor ({} channels, {} DCS tones read as BCD)",
        cp.channels.len(),
        dcs
    );
}

/// The CSV export of a real image is, byte for byte, the web editor's. `RFCPS_CSV_DIR` holds the web editor's files
/// (`channels.csv`, `contacts.csv`, `rx-group-lists.csv`, `zones.csv`) for `RFCPS_IMAGE`.
#[test]
fn the_csv_export_matches_the_web_editor() {
    use rfcps_core::csv::{export_table, TableKind};
    let (Ok(img_path), Ok(dir)) = (std::env::var("RFCPS_IMAGE"), std::env::var("RFCPS_CSV_DIR"))
    else {
        eprintln!("RFCPS_IMAGE / RFCPS_CSV_DIR not set: skipped");
        return;
    };
    let cp = Codeplug::decode(&Image::from_bytes(&std::fs::read(img_path).unwrap()));
    for t in [
        TableKind::Contacts,
        TableKind::RxLists,
        TableKind::Channels,
        TableKind::Zones,
    ] {
        let want = std::fs::read_to_string(format!("{dir}/{}", t.file())).unwrap();
        assert_eq!(export_table(&cp, t), want, "{}", t.file());
    }
    eprintln!("csv export identical to the web editor's");
}

/// The CHIRP export of a real image is, byte for byte, the web editor's (`RFCPS_CHIRP_CSV`).
#[test]
fn the_chirp_export_matches_the_web_editor() {
    let (Ok(img_path), Ok(csv)) = (
        std::env::var("RFCPS_IMAGE"),
        std::env::var("RFCPS_CHIRP_CSV"),
    ) else {
        eprintln!("RFCPS_IMAGE / RFCPS_CHIRP_CSV not set: skipped");
        return;
    };
    let cp = Codeplug::decode(&Image::from_bytes(&std::fs::read(img_path).unwrap()));
    let (text, dmr) = rfcps_core::chirp::export_chirp(&cp);
    assert_eq!(text, std::fs::read_to_string(csv).unwrap());
    eprintln!("chirp export identical to the web editor's ({dmr} DMR channel(s) left out)");
}

#[test]
fn the_qdmr_yaml_matches_the_web_editor() {
    let (Ok(img_path), Ok(dir)) = (std::env::var("RFCPS_IMAGE"), std::env::var("RFCPS_QDMR_DIR"))
    else {
        eprintln!("RFCPS_IMAGE / RFCPS_QDMR_DIR not set: skipped");
        return;
    };
    let image = Image::from_bytes(&std::fs::read(img_path).unwrap());
    let read = |n: &str| std::fs::read_to_string(format!("{dir}/{n}")).unwrap();
    let cp = Codeplug::decode(&image);

    // The export is the web editor's, character for character
    let (text, _) = rfcps_core::qdmr::export_qdmr(&cp);
    let gold = read("qdmr_js.yaml");
    if text != gold {
        let (a, b): (Vec<&str>, Vec<&str>) = (text.lines().collect(), gold.lines().collect());
        let at = a.iter().zip(&b).position(|(x, y)| x != y).unwrap_or(a.len().min(b.len()));
        panic!("export differs at line {}:\nrust: {:?}\nweb:  {:?}", at + 1, a.get(at), b.get(at));
    }

    // Importing the web editor's file and exporting again gives the web editor's result
    let mut again = Codeplug::decode(&image);
    let report = rfcps_core::qdmr::import_qdmr(&mut again, &gold).unwrap();
    assert_eq!(report.channels, cp.channels.len());
    let (round, _) = rfcps_core::qdmr::export_qdmr(&again);
    assert!(round == read("qdmr_js_roundtrip.yaml"), "the round trip differs from the web editor's");
    assert_eq!(again.channels.len(), cp.channels.len());
    assert_eq!(again.zones, cp.zones);
}

/// The settings area (codeplug-settings.md): the same edits with the web editor's `kv.js` and with this port give the
/// same sector, byte for byte. `RFCPS_KV_DIR` holds the web editor's sectors (see `docs/testing.md`).
#[test]
fn the_settings_area_matches_the_web_editor() {
    use rfcps_core::kv::{self, Keys, SigRecord, KV_ADDR, KV_SIZE};
    let (Ok(img_path), Ok(dir)) = (std::env::var("RFCPS_IMAGE"), std::env::var("RFCPS_KV_DIR")) else {
        eprintln!("RFCPS_IMAGE / RFCPS_KV_DIR not set: skipped");
        return;
    };
    let read = |n: &str| std::fs::read(format!("{dir}/{n}")).unwrap();
    let sector = |b: &[u8]| b[KV_ADDR..KV_ADDR + KV_SIZE].to_vec();
    let raw = Image::from_bytes(&std::fs::read(img_path).unwrap()).bytes().to_vec();

    let mut sig = SigRecord { dmr: 2, fm: 1, ..SigRecord::default() };
    sig.hz[0] = 1000;
    sig.ms[0] = 200;
    sig.hz[6] = 3400;
    sig.ms[6] = 900;

    // 1: a set of edits on the image as read
    let mut a = raw.clone();
    let keys = Keys {
        ars_server: Some(3_100_001),
        ip_gateway: Some(4242),
        call_end_tone: Some(false),
        battery_tone: Some(true),
        sig: [Some(sig), None, None],
        ..Keys::default()
    };
    assert!(kv::put_keys(&mut a, &keys));
    assert_eq!(sector(&a), read("kv1.bin"), "first edit");

    // 2: a second edit on that: remove the gateway, set another tone
    let mut b = a.clone();
    let mut keys2 = kv::decode_keys(&b);
    keys2.ip_gateway = None;
    keys2.tot_warn_tone = Some(true);
    assert!(kv::put_keys(&mut b, &keys2));
    assert_eq!(sector(&b), read("kv2.bin"), "second edit");

    // 3: the radio's menu changed key 2 and a tone since the file was read; the edit set the ARS server
    let mut file = a.clone();
    let mut k3 = kv::decode_keys(&file);
    k3.ars_server = Some(77);
    assert!(kv::put_keys(&mut file, &k3));
    assert_eq!(sector(&file), read("file3_before.bin"));
    let radio = read("radio3.bin");
    kv::area_for_write(&mut file, &radio, &sector(&a)).unwrap();
    assert_eq!(sector(&file), read("kv3.bin"), "area for a write");
}

/// Channel modes (channel-modes.md, p25-codeplug.md, nxdn-codeplug.md): the same edits made with the web editor's
/// `channelmodes.js` and with this port give the same image, byte for byte. `RFCPS_MODES_DIR` holds the web editor's
/// images (`edit_modes.mjs`, see `docs/testing.md`).
#[test]
fn channel_modes_encode_like_the_web_editor() {
    use rfcps_core::modes::{self, NxdnConv, P25Conv, Params};
    use rfcps_core::records::Mode;
    let (Ok(img_path), Ok(dir)) = (std::env::var("RFCPS_IMAGE"), std::env::var("RFCPS_MODES_DIR")) else {
        eprintln!("RFCPS_IMAGE / RFCPS_MODES_DIR not set: skipped");
        return;
    };
    let want = |n: &str| Image::from_bytes(&std::fs::read(format!("{dir}/{n}")).unwrap());
    let base = Image::from_bytes(&std::fs::read(img_path).unwrap());
    let pos = |cp: &Codeplug, n: u16| cp.channels.iter().position(|c| c.number == n).unwrap();
    let same = |got: &Image, w: &Image, what: &str| {
        let diff: Vec<usize> = (0..got.bytes().len()).filter(|&i| got.bytes()[i] != w.bytes()[i]).collect();
        assert!(diff.is_empty(), "{what}: {} bytes differ, first at {:#x}", diff.len(), diff[0]);
    };

    // 1: two channels made P25 and NXDN, and both unit IDs set
    let mut cp = Codeplug::decode(&base);
    let (i3, i5) = (pos(&cp, 3), pos(&cp, 5));
    modes::set_kind(&mut cp.channels[i3], modes::KIND_P25, Mode::Dmr, &[]);
    cp.channels[i3].tagged.as_mut().unwrap().params = Params::P25(P25Conv { squelch: 1, rx_nac: 0x123, tx_nac: 0x456 });
    modes::set_kind(&mut cp.channels[i5], modes::KIND_NXDN, Mode::Dmr, &[]);
    cp.channels[i5].tagged.as_mut().unwrap().params =
        Params::Nxdn(NxdnConv { rate9600: false, squelch: 1, rx_ran: 5, tx_ran: 9 });
    cp.mode_settings.p25_unit_id = 3_100_001;
    cp.mode_settings.nxdn_unit_id = 4242;
    let one = cp.encode(&base);
    same(&one, &want("modes1.bin"), "first edit");

    // The decoded image reads back as the same modes.
    let back = Codeplug::decode(&one);
    assert_eq!(back.channel(3).unwrap().kind(), modes::KIND_P25);
    assert_eq!(back.channel(5).unwrap().kind(), modes::KIND_NXDN);
    assert_eq!(back.mode_settings, cp.mode_settings);
    assert_eq!(back.encode(&one).bytes(), one.bytes(), "re-encoding the decoded image changes nothing");

    // 2: the tagged channel 3 renamed, tagged channel 5 deleted, the P25 unit ID removed
    let mut cp = Codeplug::decode(&one);
    let i3 = pos(&cp, 3);
    cp.channels[i3].name = "Renamed_P25".into();
    cp.channels.retain(|c| c.number != 5);
    cp.mode_settings.p25_unit_id = 0;
    same(&cp.encode(&one), &want("modes2.bin"), "rename and delete");

    // 3: channel 3 made a plain FM channel again, channel 5's TX RAN edited
    let mut cp = Codeplug::decode(&one);
    let (i3, i5) = (pos(&cp, 3), pos(&cp, 5));
    modes::set_kind(&mut cp.channels[i3], 0, Mode::Fm, &[]);
    if let Some(Params::Nxdn(p)) = cp.channels[i5].tagged.as_mut().map(|t| &mut t.params) {
        p.tx_ran = 12;
    }
    same(&cp.encode(&one), &want("modes3.bin"), "plain again, TX RAN edited");
}

/// Systems (channel-modes.md 7, p25-codeplug.md 3): a P25 trunked system with a site, a control channel and a plan row,
/// a channel on it, edits, and the system deleted, as whole images against the web editor's. `RFCPS_SYS_DIR` holds
/// the web editor's images (`edit_systems.mjs`, see `docs/testing.md`).
#[test]
fn systems_encode_like_the_web_editor() {
    use rfcps_core::modes::{self, P25TrunkChan, Params};
    use rfcps_core::records::Mode;
    use rfcps_core::systems::{self, EntryBody, P25Trunk, SysEntry, System, SystemFields};
    let (Ok(img_path), Ok(dir)) = (std::env::var("RFCPS_IMAGE"), std::env::var("RFCPS_SYS_DIR")) else {
        eprintln!("RFCPS_IMAGE / RFCPS_SYS_DIR not set: skipped");
        return;
    };
    let want = |n: &str| Image::from_bytes(&std::fs::read(format!("{dir}/{n}")).unwrap());
    let base = Image::from_bytes(&std::fs::read(img_path).unwrap());
    let same = |got: &Image, w: &Image, what: &str| {
        let diff: Vec<usize> = (0..got.bytes().len()).filter(|&i| got.bytes()[i] != w.bytes()[i]).collect();
        assert!(diff.is_empty(), "{what}: {} bytes differ, first at {:#x}", diff.len(), diff[0]);
    };
    let entry = |cls, group, control, body| SysEntry { cls, group, control, flags: 0, body, raw: Vec::new() };
    let pos = |cp: &Codeplug, n: u16| cp.channels.iter().position(|c| c.number == n).unwrap();

    // 1: a system and channel 6 on it
    let mut cp = Codeplug::decode(&base);
    let mut s = System::new(systems::SYSTEM_P25_TRUNK, "TestSys P25T");
    s.number = 1;
    s.fields = SystemFields::P25Trunk(P25Trunk {
        wacn: Some(0xBEE00),
        sysid: Some(0x3AB),
        cc_nac: Some(0x293),
        skip_enc: true,
        plan_only: false,
        no_reg: false,
        hold_s: 5,
        roam_dbm: -90,
    });
    s.entries.push(entry(1, 0, false, EntryBody::P25Site { rfss: Some(1), site: Some(1) }));
    s.entries.push(entry(0, 0, true, EntryBody::P25Control { iden: 1, chan_no: 5, rx_hz: 851_012_500, tx_hz: 806_012_500 }));
    s.entries.push(entry(
        2,
        1,
        false,
        EntryBody::P25Plan { base_hz: 851_006_250, spacing_hz: 12_500, tx_offset_hz: -45_000_000, bw_hz: 12_500, tdma: false, slots: 2 },
    ));
    cp.systems.push(s);
    let i6 = pos(&cp, 6);
    let systems = cp.systems.clone();
    modes::set_kind(&mut cp.channels[i6], modes::KIND_P25_TRUNK, Mode::Dmr, &systems);
    cp.channels[i6].tagged.as_mut().unwrap().params = Params::P25Trunk(P25TrunkChan { follow_all: true });
    let one = cp.encode(&base);
    same(&one, &want("sys1.bin"), "a system and a channel on it");

    // It reads back as it was written, and writing the decoded codeplug again changes nothing.
    let back = Codeplug::decode(&one);
    assert_eq!(back.systems.len(), 1);
    assert_eq!(back.systems[0].name, "TestSys P25T");
    assert_eq!(back.systems[0].entries.len(), 3);
    assert_eq!(back.channel(6).unwrap().kind(), modes::KIND_P25_TRUNK);
    assert_eq!(back.encode(&one).bytes(), one.bytes(), "re-encoding the decoded image changes nothing");

    // 2: renamed, transmit side on, a second control channel, follow-all off
    let mut cp = Codeplug::decode(&one);
    cp.systems[0].name = "Renamed Sys".into();
    cp.systems[0].tx = true;
    if let SystemFields::P25Trunk(f) = &mut cp.systems[0].fields {
        f.hold_s = 9;
    }
    cp.systems[0].entries.push(entry(0, 0, true, EntryBody::P25Control { iden: 1, chan_no: 9, rx_hz: 851_312_500, tx_hz: 0 }));
    let i6 = pos(&cp, 6);
    cp.channels[i6].tagged.as_mut().unwrap().params = Params::P25Trunk(P25TrunkChan { follow_all: false });
    same(&cp.encode(&one), &want("sys2.bin"), "edits");

    // 3: the system deleted: its channel is a plain receive-only DMR channel
    let mut cp = Codeplug::decode(&one);
    cp.remove_system(1);
    assert_eq!(cp.channel(6).unwrap().kind(), 0);
    same(&cp.encode(&one), &want("sys3.bin"), "the system deleted");
}
