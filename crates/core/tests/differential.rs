// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux

//! A random differential test against RadioFoundry's web editor. A script (`tools/webcompare/differential_gen.mjs`, see
//! docs/testing.md) applies random edits to a codeplug (a made-up one, `make-base.mjs`) through the web editor's own model and writes, for each case, the image the web
//! editor encodes, what a reader decodes from it (a canonical dump), its CSV, qdmr and CHIRP exports and its problems;
//! and, for pairs of cases, the first one's model written onto the second one's bytes. This test makes this port do
//! the same and compares everything. It runs only when `RFCPS_FUZZ_DIR` is set, and reports every mismatch.

use rfcps_core::csv::{export_table, ORDER};
use rfcps_core::modes::Params;
use rfcps_core::problems::{self, Level, Table};
use rfcps_core::records::{CallType, Mode};
use rfcps_core::systems::{EntryBody, SystemFields};
use rfcps_core::{Codeplug, Image};
use serde_json::{json, Value};

fn sig(r: &Option<rfcps_core::kv::SigRecord>) -> Value {
    match r {
        None => Value::Null,
        Some(r) => json!({"dmr": r.dmr, "fm": r.fm, "hz": r.hz, "ms": r.ms}),
    }
}

/// A system as the dump shows it (the web editor's `dump`).
fn system_json(s: &rfcps_core::systems::System) -> Value {
    let fields = match &s.fields {
        SystemFields::P25Trunk(f) => json!({"wacn": f.wacn, "sysid": f.sysid, "ccNac": f.cc_nac, "skipEnc": f.skip_enc,
            "planOnly": f.plan_only, "noReg": f.no_reg, "holdS": f.hold_s, "roamDbm": f.roam_dbm}),
        SystemFields::T3(f) => json!({"model": f.model, "net": f.net, "category": if f.category_b {"b"} else {"a"},
            "colorCode": f.color_code, "allCalls": f.all_calls, "emergencyAny": f.emergency_any, "followAll": f.follow_all, "learnSystem": f.learn_system,
            "privateCalls": f.private_calls, "registerTalkgroups": f.register_talkgroups, "acceptAls": f.accept_als,
            "acceptKill": f.accept_kill, "noRoaming": f.no_roaming, "acceptStun": f.accept_stun,
            "ipGateway": f.ip_gateway, "fallbackHold": f.fallback_hold, "planBaseHz": f.plan_base_hz,
            "planFirstLcn": f.plan_first_lcn, "planSep": f.plan_sep, "planTxLow": f.plan_tx_low,
            "planNoLearn": f.plan_no_learn, "planSplitHz": f.plan_split_hz, "leaveDbm": f.leave_dbm,
            "powerSave": f.power_save, "sites": f.sites}),
        SystemFields::Nxdn => json!({}),
        SystemFields::Aes { network } => json!({"network": network}),
        SystemFields::Unknown => Value::Null,
    };
    let entries: Vec<Value> = s
        .entries
        .iter()
        .map(|e| {
            let mut v = json!({"cls": e.cls, "group": e.group, "control": e.control, "flags": e.flags});
            let o = v.as_object_mut().unwrap();
            match &e.body {
                EntryBody::P25Control { iden, chan_no, rx_hz, tx_hz } => {
                    o.insert("chan".into(), json!((u16::from(*iden) << 12) | chan_no));
                    o.insert("rxHz".into(), json!(rx_hz));
                    o.insert("txHz".into(), json!(tx_hz));
                    o.insert("iden".into(), json!(iden));
                    o.insert("chanNo".into(), json!(chan_no));
                }
                EntryBody::P25Site { rfss, site } => {
                    o.insert("rfss".into(), json!(rfss));
                    o.insert("site".into(), json!(site));
                }
                EntryBody::P25Plan { base_hz, spacing_hz, tx_offset_hz, bw_hz, tdma, slots } => {
                    o.insert("baseHz".into(), json!(base_hz));
                    o.insert("spacingHz".into(), json!(spacing_hz));
                    o.insert("txOffsetHz".into(), json!(tx_offset_hz));
                    o.insert("bwHz".into(), json!(bw_hz));
                    o.insert("tdma".into(), json!(tdma));
                    o.insert("slots".into(), json!(slots));
                }
                EntryBody::T3Channel { lcn, rx_hz, tx_hz, cc } => {
                    o.insert("chan".into(), json!(lcn));
                    o.insert("rxHz".into(), json!(rx_hz));
                    o.insert("txHz".into(), json!(tx_hz));
                    o.insert("cc".into(), json!(cc));
                }
                EntryBody::Other => {}
            }
            v
        })
        .collect();
    json!({"number": s.number, "type": s.type_id, "name": s.name, "sharing": s.sharing, "ownId": s.own_id, "tx": s.tx,
        "needsKey": s.needs_key, "fields": fields, "entries": entries, "hasSecret": s.secret.is_some(),
        "trafficKeys": s.traffic_keys.iter().map(|k| json!({"id": k.id, "seed": k.seed, "flags": k.flags, "slot": k.slot})).collect::<Vec<_>>()})
}

fn dump(cp: &Codeplug) -> Value {
    let g = &cp.general;
    let k = &g.keys;
    let num = |n: u16| if n == 0 { Value::Null } else { json!(n) };
    let channels: Vec<Value> = cp
        .channels
        .iter()
        .map(|c| {
            let t = c.tagged.as_ref();
            let params = t.map(|t| match &t.params {
                Params::P25(p) => json!({"squelch": p.squelch, "rxNac": p.rx_nac, "txNac": p.tx_nac}),
                Params::Nxdn(p) => {
                    let mut v = json!({"rate9600": p.rate9600, "squelch": p.squelch, "rxRan": p.rx_ran, "txRan": p.tx_ran});
                    if let Some(id) = p.tx_key_id {
                        v["txKeyId"] = json!(id);
                    }
                    v
                }
                Params::P25Trunk(p) => json!({"followAll": p.follow_all}),
                Params::T3 => json!({}),
                Params::Aes(p) => json!({"id": p.key_id}),
                Params::Unknown => Value::Null,
            });
            json!({
                "number": c.number, "name": c.name, "rxHz": c.rx_hz, "txHz": c.tx_hz,
                "mode": if c.mode == Mode::Dmr { "dmr" } else { "fm" }, "power": c.power, "tot": c.tot,
                "rxTone": c.rx_tone, "txTone": c.tx_tone, "bw": c.bw, "rxOnly": c.rx_only, "allSkip": c.all_skip,
                "zoneSkip": c.zone_skip, "vox": c.vox, "squelch": c.squelch, "colorCode": c.color_code,
                "timeslot": c.timeslot, "contact": num(c.contact), "rxList": num(u16::from(c.rx_list)),
                "aprs": num(u16::from(c.aprs)), "dmrId": c.dmr_id, "noBeep": c.no_beep, "noEco": c.no_eco, "dmo": c.dmo,
                "roam": c.roam, "aliasTs1": c.alias_ts1, "aliasTs2": c.alias_ts2, "useLocation": c.use_location,
                "lat": c.lat, "lon": c.lon, "sfr": c.sfr, "sfrA": c.sfr_a, "sfrB": c.sfr_b, "toneBurst": c.tone_burst, "acceptBadCrc": c.accept_bad_crc,
                "kind": c.kind(),
                "tagTx": t.map(|t| t.tx), "baseOwn": t.map(|t| t.base_own), "noFallback": t.map(|t| t.no_fallback),
                "system": t.and_then(|t| (t.system != 0xFF).then(|| u16::from(t.system) + 1)),
                "params": params,
            })
        })
        .collect();
    let systems: Vec<Value> = cp.systems.iter().map(system_json).collect();
    json!({
        "general": {"radioName": g.radio_name, "dmrId": g.dmr_id, "groupHang": g.group_hang, "privateHang": g.private_hang,
            "bootLine1": g.boot_line1, "bootLine2": g.boot_line2, "arsServer": k.ars_server, "ipGateway": k.ip_gateway,
            "callEndTone": k.call_end_tone, "totWarnTone": k.tot_warn_tone, "batteryTone": k.battery_tone,
            "txDenyTone": k.tx_deny_tone, "sigBegin": sig(&k.sig[0]), "sigEnd": sig(&k.sig[1]), "sigIdle": sig(&k.sig[2])},
        "modeSettings": {"p25": cp.mode_settings.p25_unit_id, "nxdn": cp.mode_settings.nxdn_unit_id},
        "channels": channels,
        "zones": cp.zones.iter().map(|z| json!({"name": z.name, "channels": z.channels})).collect::<Vec<_>>(),
        "contacts": cp.contacts.iter().map(|c| json!({"number": c.number, "name": c.name, "id": c.id,
            "type": match c.call_type { CallType::Group => "group", CallType::Private => "private", CallType::All => "all" },
            "timeslot": c.timeslot})).collect::<Vec<_>>(),
        "rxLists": cp.rx_lists.iter().map(|l| json!({"number": l.number, "name": l.name, "contacts": l.contacts})).collect::<Vec<_>>(),
        "dtmf": cp.dtmf_contacts.iter().map(|d| json!({"number": d.number, "name": d.name, "code": d.code})).collect::<Vec<_>>(),
        "aprs": cp.aprs.iter().map(|a| json!({"number": a.number, "name": a.name, "ssid": a.ssid, "lat": a.lat, "lon": a.lon,
            "path1": a.path1, "path2": a.path2, "symTable": a.sym_table.to_string(), "symCode": a.sym_code.to_string(),
            "comment": a.comment, "txHz": a.tx_hz, "baud300": a.baud300, "useFixed": a.use_fixed, "qsy": a.qsy,
            "silent": a.silent, "ambiguity": a.ambiguity})).collect::<Vec<_>>(),
        "systems": systems,
    })
}

/// The differences between two dumps, as short strings (numbers compared to 1e-9).
fn diff(a: &Value, b: &Value, path: &str, out: &mut Vec<String>) {
    if out.len() >= 6 {
        return;
    }
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            if (x.as_f64().unwrap() - y.as_f64().unwrap()).abs() > 1e-9 {
                out.push(format!("{path}: rust {x} web {y}"));
            }
        }
        (Value::Array(x), Value::Array(y)) => {
            if x.len() != y.len() {
                out.push(format!("{path}: rust has {} items, web {}", x.len(), y.len()));
            }
            for (i, (p, q)) in x.iter().zip(y).enumerate() {
                diff(p, q, &format!("{path}[{i}]"), out);
            }
        }
        (Value::Object(x), Value::Object(y)) => {
            for (k, v) in x {
                match y.get(k) {
                    Some(w) => diff(v, w, &format!("{path}.{k}"), out),
                    None if v.is_null() => {}
                    None => out.push(format!("{path}.{k}: rust {v} web (absent)")),
                }
            }
            for (k, w) in y {
                if !x.contains_key(k) && !w.is_null() {
                    out.push(format!("{path}.{k}: rust (absent) web {w}"));
                }
            }
        }
        _ if a == b => {}
        _ => out.push(format!("{path}: rust {a} web {b}")),
    }
}

fn first_diff_line(a: &str, b: &str) -> String {
    let (x, y): (Vec<&str>, Vec<&str>) = (a.lines().collect(), b.lines().collect());
    let at = x.iter().zip(&y).position(|(p, q)| p != q).unwrap_or(x.len().min(y.len()));
    format!("line {}: rust {:?} web {:?}", at + 1, x.get(at), y.get(at))
}

fn first_diff_byte(a: &[u8], b: &[u8]) -> String {
    let d: Vec<usize> = (0..a.len().min(b.len())).filter(|&i| a[i] != b[i]).collect();
    format!("{} bytes differ, first at {:#x}", d.len(), d.first().copied().unwrap_or(0))
}

fn table_name(t: Table) -> &'static str {
    match t {
        Table::General => "general",
        Table::Channels => "channels",
        Table::Zones => "zones",
        Table::Contacts => "contacts",
        Table::RxLists => "rxLists",
        Table::Systems => "systems",
        Table::Dtmf => "dtmfContacts",
        Table::Aprs => "aprs",
    }
}

#[test]
fn random_edits_agree_with_the_web_editor() {
    let Ok(dir) = std::env::var("RFCPS_FUZZ_DIR") else {
        eprintln!("RFCPS_FUZZ_DIR not set: skipped");
        return;
    };
    let index: Value = serde_json::from_str(&std::fs::read_to_string(format!("{dir}/index.json")).unwrap()).unwrap();
    let cases: Vec<u64> = index["cases"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap()).collect();
    let mut fails: Vec<String> = Vec::new();
    let mut checks = 0usize;
    let mut fail = |case: String, what: &str, detail: String| fails.push(format!("{case}: {what}: {detail}"));

    for &i in &cases {
        let case = format!("c{i}");
        let read = |n: &str| std::fs::read(format!("{dir}/{case}/{n}")).unwrap();
        let text = |n: &str| String::from_utf8(read(n)).unwrap();
        let ops = text("ops.txt");
        let img = Image::from_bytes(&read("img.bin"));
        let cp = Codeplug::decode(&img);

        // 1. decoding and writing back does what the web editor's does (it changes nothing, except that a channel
        // in a listen band is read as RX only with TX on RX, so the record is written so)
        checks += 1;
        let back = cp.encode(&img);
        let want_back = std::fs::read(format!("{dir}/{case}/reenc.bin")).unwrap_or_else(|_| img.bytes().to_vec());
        if back.bytes() != want_back.as_slice() {
            fail(case.clone(), "re-encode", format!("{} [{ops}]", first_diff_byte(back.bytes(), &want_back)));
        }
        // 2. what a reader decodes
        checks += 1;
        let (mut d, want) = (dump(&cp), serde_json::from_str::<Value>(&text("dump.json")).unwrap());
        let mut out = Vec::new();
        diff(&d, &want, "", &mut out);
        if !out.is_empty() {
            fail(case.clone(), "decode", format!("{} [{ops}]", out.join("; ")));
        }
        d = Value::Null;
        let _ = d;
        // 3. the exports
        for t in ORDER {
            checks += 1;
            let (got, w) = (export_table(&cp, t), text(t.file()));
            if got != w {
                fail(case.clone(), t.file(), first_diff_line(&got, &w));
            }
        }
        checks += 1;
        let (got, w) = (rfcps_core::qdmr::export_qdmr(&cp).0, text("qdmr.yaml"));
        if got != w {
            fail(case.clone(), "qdmr export", first_diff_line(&got, &w));
        }
        // the qdmr file imported into a fresh read of the image: what it decodes to and exports as
        let rt = text("qdmr_rt.yaml");
        if !rt.starts_with("ERR ") {
            checks += 1;
            let mut cp3 = Codeplug::decode(&img);
            match rfcps_core::qdmr::import_qdmr(&mut cp3, &text("qdmr.yaml")) {
                Err(e) => fail(case.clone(), "qdmr import", e),
                Ok(_) => {
                    let (mut d3, want3) = (dump(&cp3), serde_json::from_str::<Value>(&text("qdmr_rt_dump.json")).unwrap());
                    let mut out = Vec::new();
                    diff(&d3, &want3, "", &mut out);
                    if !out.is_empty() {
                        fail(case.clone(), "qdmr import decode", out.join("; "));
                    }
                    d3 = Value::Null;
                    let _ = d3;
                    let got = rfcps_core::qdmr::export_qdmr(&cp3).0;
                    if got != rt {
                        fail(case.clone(), "qdmr import export", first_diff_line(&got, &rt));
                    }
                }
            }
        }
        checks += 1;
        let (got, w) = (rfcps_core::chirp::export_chirp(&cp).0, text("chirp.csv"));
        if got != w && !w.starts_with("ERR ") {
            fail(case.clone(), "chirp export", first_diff_line(&got, &w));
        }
        // 4. the problems: the same level and table, and the same record where both say which
        checks += 1;
        let web: Vec<Value> = serde_json::from_str(&text("problems.json")).unwrap();
        // The weather-channel warning is a CPS-only addition: the web editor has no such rule.
        let mut a: Vec<(String, String)> = problems::validate(&cp, None)
            .iter()
            .filter(|p| !p.message.starts_with("A weather channel"))
            .map(|p| (if p.level == Level::Error { "error" } else { "warning" }.to_string(), table_name(p.table).to_string()))
            .collect();
        let mut b: Vec<(String, String)> = web
            .iter()
            .map(|p| (p["level"].as_str().unwrap().to_string(), p["table"].as_str().unwrap().to_string()))
            .collect();
        a.sort();
        b.sort();
        if a != b {
            let count = |v: &[(String, String)]| {
                let mut m = std::collections::BTreeMap::new();
                for x in v {
                    *m.entry(x.clone()).or_insert(0) += 1;
                }
                m
            };
            let (ca, cb) = (count(&a), count(&b));
            let diffs: Vec<String> = ca
                .keys()
                .chain(cb.keys())
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .filter(|k| ca.get(*k) != cb.get(*k))
                .map(|k| format!("{} {} rust {} web {}", k.0, k.1, ca.get(k).unwrap_or(&0), cb.get(k).unwrap_or(&0)))
                .collect();
            fail(case.clone(), "problems", format!("{} [{ops}]", diffs.join("; ")));
        }
    }
    // 5. a model written onto another image's bytes
    for p in index["pairs"].as_array().unwrap() {
        let (i, j) = (p[0].as_u64().unwrap(), p[1].as_u64().unwrap());
        let a = Codeplug::decode(&Image::from_bytes(&std::fs::read(format!("{dir}/c{i}/img.bin")).unwrap()));
        let onto = Image::from_bytes(&std::fs::read(format!("{dir}/c{j}/img.bin")).unwrap());
        let want = std::fs::read(format!("{dir}/c{i}/onto_{j}.bin")).unwrap();
        checks += 1;
        let got = a.encode(&onto);
        if got.bytes() != Image::from_bytes(&want).bytes() {
            fail(format!("c{i} onto c{j}"), "encode onto another image", first_diff_byte(got.bytes(), &want));
        }
    }
    eprintln!("{} cases, {} checks, {} mismatches", cases.len(), checks, fails.len());
    for f in fails.iter().take(40) {
        eprintln!("  {f}");
    }
    assert!(fails.is_empty(), "{} mismatches against the web editor", fails.len());
}

#[test]
fn the_recommended_ham_systems_match_the_web_editor() {
    let Ok(dir) = std::env::var("RFCPS_FUZZ_DIR") else {
        eprintln!("RFCPS_FUZZ_DIR not set: skipped");
        return;
    };
    let Ok(text) = std::fs::read_to_string(format!("{dir}/ham.json")) else {
        eprintln!("no ham.json: skipped");
        return;
    };
    let cases: Vec<Value> = serde_json::from_str(&text).unwrap();
    assert!(!cases.is_empty());
    for c in cases {
        let a = &c["args"];
        let got = rfcps_core::systems::new_ham_system(
            a["type"].as_u64().unwrap() as u8,
            a["name"].as_str().unwrap(),
            a["rxHz"].as_u64().unwrap() as u32,
            a["txHz"].as_u64().unwrap() as u32,
            a["site"].as_u64().unwrap() as u16,
        )
        .expect("a ham system");
        let mut got = system_json(&got);
        let mut want = c["system"].clone();
        for k in ["number", "hasSecret", "trafficKeys"] {
            got.as_object_mut().unwrap().remove(k);
            want.as_object_mut().unwrap().remove(k);
        }
        let mut out = Vec::new();
        diff(&got, &want, "", &mut out);
        assert!(out.is_empty(), "{a}: {out:?}");
    }
}
