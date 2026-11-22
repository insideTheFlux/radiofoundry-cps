// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux
// A made-up codeplug image, built through the web editor's own model, for the golden tests to start from: 39 channels
// (FM and DMR, tones, power levels, flags, contact and RX list references), 7 zones, 10 contacts, 2 RX lists, and a radio
// name, DMR ID and boot text. No radio and no personal data are involved. Usage: node make-base.mjs OUT.bin
import { writeFileSync } from "node:fs";
const J = `${process.env.RFOUNDRY_DIR || process.env.HOME + "/rfoundry"}/site/static/js/codeplug`;
const { empty, encode, newChannel, newContact } = await import(`${J}/model.js`);

let s = 20260101;
const R = () => { s = (Math.imul(s, 1664525) + 1013904223) >>> 0; return s / 4294967296; };
const rnd = (n) => Math.floor(R() * n);
const pick = (a) => a[rnd(a.length)];

const m = empty();
m.general.radioName = "TESTRAD";
m.general.dmrId = 1234567;
m.general.groupHang = 3;
m.general.privateHang = 5;
m.general.bootLine1 = "Hello";
m.general.bootLine2 = "World";

const names = ["Talkgroup 1", "Talkgroup 9", "Local 91", "Worldwide", "Regional", "Tac 310", "Tac 311", "Net 8"];
m.contacts = [];
names.forEach((name, i) => m.contacts.push(newContact({ number: i + 1, name, id: [1, 9, 91, 4000, 3100, 310, 311, 8][i], type: "group", timeslot: i % 3 })));
m.contacts.push(newContact({ number: 9, name: "Private Call", id: 1234568, type: "private", timeslot: 0 }));
m.contacts.push(newContact({ number: 10, name: "All Call", id: 16777215, type: "all", timeslot: 0 }));

m.rxLists = [
  { number: 1, name: "Local TGs", contacts: m.contacts.slice(0, 3) },
  { number: 2, name: "Wide TGs", contacts: m.contacts.slice(2, 6) },
];

const SIMPLEX = [146520000, 146550000, 147420000, 446000000, 446025000, 433500000, 462562500, 151820000];
const TONES = ["", "67.0", "88.5", "100.0", "123.0", "D023N", "D754I"];
m.channels = [];
for (let n = 1; n <= 39; n++) {
  const dmr = n % 3 === 0;
  const f = pick(SIMPLEX);
  const rpt = !dmr && n % 4 === 1;
  m.channels.push(newChannel({
    number: n, name: dmr ? `DMR ${n}` : rpt ? `RPT ${n}` : `FM ${n}`,
    rxHz: rpt ? f + 600000 : f, txHz: f, mode: dmr ? "dmr" : "fm",
    power: pick([0, 1, 3, 6, 9]), tot: 15 * rnd(20), bw: dmr ? 12.5 : pick([12.5, 25]),
    rxTone: dmr ? "" : pick(TONES), txTone: dmr ? "" : pick(TONES),
    rxOnly: n % 13 === 0, allSkip: n % 11 === 0, zoneSkip: n % 7 === 0, vox: n % 9 === 0, squelch: rnd(5),
    colorCode: dmr ? 1 + rnd(3) : 1, timeslot: 1 + (n % 2),
    contact: dmr ? m.contacts[n % 8] : null, rxList: dmr ? m.rxLists[n % 2] : null,
  }));
}

m.zones = [];
for (let z = 0; z < 7; z++) {
  const members = m.channels.filter((c) => (c.number + z) % 7 < 3).slice(0, 12);
  m.zones.push({ slot: z, name: `Zone ${z + 1}`, channels: members });
}

writeFileSync(process.argv[2], Buffer.from(encode(m).bytes));
console.log("base image:", m.channels.length, "channels,", m.zones.length, "zones,", m.contacts.length, "contacts,", m.rxLists.length, "RX lists");
