// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux
// The web editor's whole images for fixed DMR Tier III system edits (RFCPS_T3_DIR).
import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
const J = `${process.env.RFOUNDRY_DIR || process.env.HOME + "/rfoundry"}/site/static/js/codeplug`;
const { CodeplugImage } = await import(`${J}/image.js`);
const { decode, encode, remove } = await import(`${J}/model.js`);
const { setKind, newSystem } = await import(`${J}/channelmodes.js`);
const { decodeArea, dedupe, setValue, putArea } = await import(`${J}/kv.js`);
const out = process.argv[3];
mkdirSync(out, { recursive: true });
const load = (b) => decode(CodeplugImage.from(b));
const save = (name, m) => { const img = encode(m); writeFileSync(`${out}/${name}`, Buffer.from(img.bytes)); return img.bytes; };
const ch = (m, n) => m.channels.find((c) => c.number === n);

// 1: a Tier III system with every field set, two channels and a key; channel 6 put on it
let m = load(new Uint8Array(readFileSync(process.argv[2])));
const s = newSystem(m, 1, "TestT3");
Object.assign(s.fields, {
	model: 1, net: 5, category: "b", colorCode: 3, allCalls: true, privateCalls: true, registerTalkgroups: true, noRoaming: true,
	fallbackHold: 7, planBaseHz: 440000000, planFirstLcn: 2, planSep: 3, planTxLow: true, planNoLearn: false, planSplitHz: 5000000,
	leaveDbm: -85, powerSave: 2, sites: [3, 7],
});
s.needsKey = true;
s.entries.push({ cls: 0, control: true, flags: 0, chan: 1, rxHz: 438100000, txHz: 443100000, cc: null });
s.entries.push({ cls: 0, control: false, flags: 0, chan: 2, rxHz: 438125000, txHz: 443125000, cc: 5 });
s.secret = { key: "00112233445566778899AABBCCDDEEFF", psn: 0x010203 };
m.systems.push(s);
const c6 = ch(m, 6);
setKind(m, c6, 1);
console.log("tx default of the system:", s.tx, "channel tag tx:", c6.tagTx);
const b1 = save("t3_1.bin", m);

// The system's own records (the short data poll and the supplementary data), put in as another tool would
const withOwned = Uint8Array.from(b1);
let entries = dedupe(decodeArea(withOwned).entries);
entries = setValue(entries, 0x0800, Uint8Array.of(1, 2, 3));
entries = setValue(entries, 0x0900, Uint8Array.of(4, 5, 6, 7));
putArea(withOwned, entries);
writeFileSync(`${out}/t3_1b.bin`, Buffer.from(withOwned));

// 2: from that: renamed, listen only, more options, a third channel, the PSN dropped, no fallback on the channel
m = load(Uint8Array.from(withOwned));
const t = m.systems[0];
t.name = "RenamedT3";
t.tx = false;
t.fields.acceptKill = true;
t.fields.sites = [3];
t.fields.leaveDbm = -70;
t.entries.push({ cls: 0, control: false, flags: 0, chan: 3, rxHz: 438150000, txHz: 443150000, cc: null });
t.secret = { key: "00112233445566778899AABBCCDDEEFF", psn: 0 };
ch(m, 6).noFallback = true;
save("t3_2.bin", m);

// 3: from that: the system deleted: its channel is plain and receive only, its secret and own records go
m = load(Uint8Array.from(withOwned));
remove(m, "systems", [m.systems[0]]);
save("t3_3.bin", m);
console.log("done");
