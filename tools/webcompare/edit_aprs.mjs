// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux
// The web editor's whole images for DTMF contact and APRS configuration edits (RFCPS_APRS_DIR).
import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
const J = `${process.env.RFOUNDRY_DIR || process.env.HOME + "/rfoundry"}/site/static/js/codeplug`;
const { CodeplugImage } = await import(`${J}/image.js`);
const { decode, encode, remove } = await import(`${J}/model.js`);
const out = process.argv[3];
mkdirSync(out, { recursive: true });
const load = (b) => decode(CodeplugImage.from(b));
const save = (name, m) => { const img = encode(m); writeFileSync(`${out}/${name}`, Buffer.from(img.bytes)); return img.bytes; };
const ch = (m, n) => m.channels.find((c) => c.number === n);
const APRS_BASE = 0x1588;

// 1: two DTMF contacts (one lower case), two APRS configurations, channel 3 using the first
let m = load(new Uint8Array(readFileSync(process.argv[2])));
m.dtmfContacts.push({ number: null, name: "Home Gate", code: "1234A#*" });
m.dtmfContacts.push({ number: null, name: "Lowercase", code: "ab12" });
m.aprs.push({ number: null, name: "Mobile", ssid: 9, lat: 37.7749, lon: -122.4194, path1: "WIDE1-1", path2: "WIDE2-2", symTable: "/", symCode: ">",
	comment: "RadioFoundry test", txHz: 144390000, baud300: false, useFixed: true, qsy: true, silent: false, ambiguity: 2 });
m.aprs.push({ number: null, name: "Fixed", ssid: 0, lat: -33.8688, lon: 151.2093, path1: "", path2: "", symTable: "\\", symCode: "-",
	comment: "", txHz: 0, baud300: true, useFixed: false, qsy: false, silent: true, ambiguity: 0 });
ch(m, 3).aprs = m.aprs[0];
const b1 = save("aprs1.bin", m);

// 2: from that: a code changed, one DTMF contact removed, the first APRS edited, the second deleted
m = load(Uint8Array.from(b1));
m.dtmfContacts[0].code = "9999";
remove(m, "dtmfContacts", [m.dtmfContacts[1]]);
Object.assign(m.aprs[0], { comment: "Edited comment", lat: 38.5, ssid: 7 });
remove(m, "aprs", [m.aprs[1]]);
save("aprs2.bin", m);

// 3: an APRS record in the older layout (magic 0x4152, ambiguity one bit lower): read as it is, upgraded when it is rewritten
const old = Uint8Array.from(b1);
old[APRS_BASE + 0x3E] = 0x52;
old[APRS_BASE + 0x3F] = 0x41;
old[APRS_BASE + 0x3D] = 0x01 | (3 << 4);
writeFileSync(`${out}/aprs_old.bin`, Buffer.from(old));
m = load(Uint8Array.from(old));
console.log("old layout reads ambiguity", m.aprs[0].ambiguity, "baud300", m.aprs[0].baud300);
save("aprs3_unchanged.bin", m);
m.aprs[0].comment = "Upgraded";
save("aprs3_edited.bin", m);
console.log("done");
