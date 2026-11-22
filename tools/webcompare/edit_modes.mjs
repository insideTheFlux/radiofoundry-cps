// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux
// The web editor's whole images for fixed P25 and NXDN channel edits (RFCPS_MODES_DIR).
import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
const J = `${process.env.RFOUNDRY_DIR || process.env.HOME + "/rfoundry"}/site/static/js/codeplug`;
const { CodeplugImage } = await import(`${J}/image.js`);
const { decode, encode } = await import(`${J}/model.js`);
const { setKind } = await import(`${J}/channelmodes.js`);
const out = process.argv[3];
mkdirSync(out, { recursive: true });
const load = (b) => decode(CodeplugImage.from(b));
const save = (name, m) => { const img = encode(m); writeFileSync(`${out}/${name}`, Buffer.from(img.bytes)); return img.bytes; };
const ch = (m, n) => m.channels.find((c) => c.number === n);

// 1: two channels made P25 and NXDN, and both unit IDs set
let m = load(new Uint8Array(readFileSync(process.argv[2])));
const c3 = ch(m, 3), c5 = ch(m, 5);
console.log("ch3", c3.name, c3.mode, c3.rxOnly, "ch5", c5.name);
setKind(m, c3, 2);
Object.assign(c3.params, { squelch: 1, rxNac: 0x123, txNac: 0x456 });
setKind(m, c5, 4);
Object.assign(c5.params, { rate9600: false, squelch: 1, rxRan: 5, txRan: 9 });
m.modeSettings[2] = { unitId: 3100001 };
m.modeSettings[4] = { unitId: 4242 };
const b1 = save("modes1.bin", m);

// 2: from that image: the tagged channel 3 renamed, tagged channel 5 deleted, P25 unit ID removed
m = load(Uint8Array.from(b1));
ch(m, 3).name = "Renamed_P25";
m.channels = m.channels.filter((c) => c.number !== 5);
m.modeSettings[2] = { unitId: 0 };
const b2 = save("modes2.bin", m);

// 3: from image 1: channel 3 made a plain FM channel again, channel 5's TX RAN edited
m = load(Uint8Array.from(b1));
setKind(m, ch(m, 3), 0, "fm");
ch(m, 5).params.txRan = 12;
save("modes3.bin", m);
console.log("done");
