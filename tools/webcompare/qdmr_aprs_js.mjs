// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux
// The web editor's qdmr export and import for an image that holds DTMF and APRS (qdmr/ of RFCPS_APRS_DIR).
import { readFileSync, writeFileSync } from "node:fs";
const J = `${process.env.RFOUNDRY_DIR || process.env.HOME + "/rfoundry"}/site/static/js/codeplug`;
const { CodeplugImage } = await import(`${J}/image.js`);
const { decode } = await import(`${J}/model.js`);
const { exportQdmr, importQdmr } = await import(`${J}/qdmr.js`);
const dir = process.argv[4];
const img = (p) => CodeplugImage.from(new Uint8Array(readFileSync(p)));
// 1: the export of an image that holds APRS and DTMF, and the round trip through the import
const m = decode(img(process.argv[2]));
const e = exportQdmr(m);
writeFileSync(`${dir}/export.yaml`, e.text);
writeFileSync(`${dir}/export_notes.json`, JSON.stringify(e.notes));
const m2 = decode(img(process.argv[2]));
importQdmr(m2, e.text);
writeFileSync(`${dir}/export_roundtrip.yaml`, exportQdmr(m2).text);
// 2: importing a rich file into the real image's codeplug
const m3 = decode(img(process.argv[3]));
const r = importQdmr(m3, readFileSync(`${dir}/rich.yaml`, "utf8"));
writeFileSync(`${dir}/rich_import.json`, JSON.stringify({ notes: r.notes, counts: r.counts }));
const f = exportQdmr(m3);
writeFileSync(`${dir}/rich_export.yaml`, f.text);
writeFileSync(`${dir}/rich_export_notes.json`, JSON.stringify(f.notes));
const sum = m3.aprs.map((a) => ({ n: a.number, name: a.name, ssid: a.ssid, lat: a.lat, lon: a.lon, p1: a.path1, p2: a.path2, st: a.symTable, sc: a.symCode, comment: a.comment, tx: a.txHz, b3: a.baud300, fx: a.useFixed, q: a.qsy, s: a.silent, amb: a.ambiguity }));
writeFileSync(`${dir}/rich_aprs.json`, JSON.stringify({ aprs: sum, dtmf: m3.dtmfContacts.map((d) => ({ n: d.number, name: d.name, code: d.code })), chAprs: m3.channels.map((c) => c.aprs && c.aprs.number) }));
console.log("import:", JSON.stringify(r)); console.log("export notes:", JSON.stringify(f.notes)); console.log("aprs", m3.aprs.length, "dtmf", m3.dtmfContacts.length);
