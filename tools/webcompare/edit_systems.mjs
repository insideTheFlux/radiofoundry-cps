// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux
// The web editor's whole images for fixed P25 trunked system edits (RFCPS_SYS_DIR).
import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
const J = `${process.env.RFOUNDRY_DIR || process.env.HOME + "/rfoundry"}/site/static/js/codeplug`;
const { CodeplugImage } = await import(`${J}/image.js`);
const { decode, encode, remove } = await import(`${J}/model.js`);
const { setKind, newSystem } = await import(`${J}/channelmodes.js`);
const out = process.argv[3];
mkdirSync(out, { recursive: true });
const load = (b) => decode(CodeplugImage.from(b));
const save = (name, m) => { const img = encode(m); writeFileSync(`${out}/${name}`, Buffer.from(img.bytes)); return img.bytes; };
const ch = (m, n) => m.channels.find((c) => c.number === n);

// 1: a P25 trunked system with a site, a control channel and a plan row; channel 6 put on it
let m = load(new Uint8Array(readFileSync(process.argv[2])));
const s = newSystem(m, 3, "TestSys P25T");
Object.assign(s.fields, { wacn: 0xBEE00, sysid: 0x3AB, ccNac: 0x293, skipEnc: true, planOnly: false, noReg: false, holdS: 5, roamDbm: -90 });
s.entries.push({ cls: 1, group: 0, control: false, flags: 0, rfss: 1, site: 1 });
s.entries.push({ cls: 0, group: 0, control: true, flags: 0, iden: 1, chanNo: 5, rxHz: 851012500, txHz: 806012500, chan: 0 });
s.entries.push({ cls: 2, group: 1, control: false, flags: 0, baseHz: 851006250, spacingHz: 12500, txOffsetHz: -45000000, bwHz: 12500, tdma: false, slots: 2 });
m.systems.push(s);
const c6 = ch(m, 6);
console.log("ch6", c6.name, c6.mode, c6.rxHz);
setKind(m, c6, 3);
c6.params.followAll = true;
save("sys1.bin", m);
const b1 = encode(m).bytes;

// 2: from image 1: the system renamed and made transmit-capable, a second control channel, follow-all off
m = load(Uint8Array.from(b1));
const t = m.systems[0];
t.name = "Renamed Sys";
t.tx = true;
t.fields.holdS = 9;
t.entries.push({ cls: 0, group: 0, control: true, flags: 0, iden: 1, chanNo: 9, rxHz: 851312500, txHz: 0, chan: 0 });
ch(m, 6).params.followAll = false;
save("sys2.bin", m);

// 3: from image 1: the system deleted: its channel becomes a plain receive-only DMR channel
m = load(Uint8Array.from(b1));
remove(m, "systems", [m.systems[0]]);
save("sys3.bin", m);
console.log("done", ch(m, 6).kind, ch(m, 6).rxOnly);
