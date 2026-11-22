// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux
// A richer starting point: the starting image plus fixed RX lists, DTMF contacts, APRS configurations, systems and tags.
import { readFileSync, writeFileSync } from "node:fs";
const J = `${process.env.RFOUNDRY_DIR || process.env.HOME + "/rfoundry"}/site/static/js/codeplug`;
const { CodeplugImage } = await import(`${J}/image.js`);
const { decode, encode, newContact } = await import(`${J}/model.js`);
const { setKind, newSystem, encodeTag } = await import(`${J}/channelmodes.js`);
const { decodeArea, setValue, putArea } = await import(`${J}/kv.js`);
const { nameCheck } = await import(`${J}/modebytes.js`);
const m = decode(CodeplugImage.from(new Uint8Array(readFileSync(process.argv[2]))));
for (let i = 0; i < 4; i++) m.contacts.push(newContact({ name: `Prep TG ${i}`, id: 1000 + i, type: "group", timeslot: i % 3 }));
for (let i = 0; i < 5; i++) m.rxLists.push({ number: null, name: `Prep List ${i}`, contacts: m.contacts.slice(i, i + 4) });
for (let i = 0; i < 6; i++) m.dtmfContacts.push({ number: null, name: `Prep DTMF ${i}`, code: `12${i}A#*` });
for (let i = 0; i < 4; i++) m.aprs.push({ number: null, name: `Prep ${i}`, ssid: i, lat: 37.5 + i, lon: -122.5 - i, path1: "WIDE1-1", path2: i % 2 ? "WIDE2-2" : "", symTable: "/", symCode: ">", comment: `comment ${i}`, txHz: 144390000, baud300: i === 1, useFixed: i === 2, qsy: i === 3, silent: false, ambiguity: i });
const s1 = newSystem(m, 3, "Prep P25T"); Object.assign(s1.fields, { wacn: 0xBEE00, sysid: 0x3AB, ccNac: 0x293, holdS: 4 });
s1.entries.push({ cls: 1, group: 0, control: false, flags: 0, rfss: 1, site: 1 }, { cls: 0, group: 0, control: true, flags: 0, iden: 1, chanNo: 5, rxHz: 851012500, txHz: 806012500, chan: 0 }, { cls: 2, group: 1, control: false, flags: 0, baseHz: 851006250, spacingHz: 12500, txOffsetHz: -45000000, bwHz: 12500, tdma: false, slots: 2 });
m.systems.push(s1);
const s2 = newSystem(m, 1, "Prep T3"); Object.assign(s2.fields, { model: 1, net: 5, category: "b", colorCode: 3, allCalls: true, sites: [3, 7] });
s2.entries.push({ cls: 0, control: true, flags: 0, chan: 1, rxHz: 438100000, txHz: 443100000, cc: null }); s2.needsKey = true; s2.secret = { key: "00112233445566778899AABBCCDDEEFF", psn: 5 };
m.systems.push(s2);
const ch = (n) => m.channels.find((c) => c.number === n);
setKind(m, ch(3), 2); setKind(m, ch(5), 4); setKind(m, ch(7), 3); setKind(m, ch(9), 1);
m.zones.push({ name: "Prep Zone", channels: m.channels.slice(0, 20), slot: null });
const img = encode(m);
// Stale tags: tags at plain channels whose check matches no name (another tool renamed or replaced the channel), so an
// edit has to leave them alone; and one whose check is a name the channel had, which a rename has to remove.
const area = decodeArea(img.bytes);
let entries = area.entries;
const plain = m.channels.filter((c) => !c.kind && ![3, 5, 7, 9].includes(c.number));
plain.slice(0, 6).forEach((c, i) => {
	const check = i < 3 ? (nameCheck(c.name) + 0x1234 + i) & 0xFFFF : nameCheck(c.name);
	entries = setValue(entries, 0x0400 + c.number - 1, encodeTag({ kind: i % 2 ? 2 : 4, system: 0, check, flags: 0, fbChan: 0, fbCheck: 0, params: new Uint8Array(i % 2 ? 6 : 4) }));
});
putArea(img.bytes, entries);
writeFileSync(process.argv[3], Buffer.from(img.bytes));
console.log("prepared", m.channels.length, "channels", m.rxLists.length, "lists", m.aprs.length, "aprs", m.systems.length, "systems");
