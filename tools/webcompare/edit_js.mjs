// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux
// The web editor's encode of a fixed set of edits to an image (RFCPS_EDITED_JS).
import { readFileSync, writeFileSync } from "node:fs";
const J = `${process.env.RFOUNDRY_DIR || process.env.HOME + "/rfoundry"}/site/static/js/codeplug`;
const { CodeplugImage } = await import(`${J}/image.js`);
const { decode, encode } = await import(`${J}/model.js`);
const img = CodeplugImage.from(new Uint8Array(readFileSync(process.argv[2])));
const m = decode(img);
const ch = (n) => m.channels.find((c) => c.number === n);
ch(7).rxTone = "D023I";
ch(3).squelch = 3;
ch(9).name = "Test_Chan";
ch(9).power = 3;
m.channels.push({ ...m.channels[0], number: 40, name: "NewFM", rxHz: 146520000, txHz: 146520000, rxTone: "", txTone: "88.5", bw: 12.5, colorCode: 1 });
m.contacts.push({ number: 11, name: "Test", id: 4242, type: "group", timeslot: 2 });
m.zones[2].name = "Biz";
m.zones[2].channels.push(m.channels.find((c) => c.number === 40));
m.general.radioName = "TESTCALL";
m.general.bootLine1 = "Hello";
writeFileSync(process.argv[3], Buffer.from(encode(m).bytes));
console.log("wrote", process.argv[3]);
