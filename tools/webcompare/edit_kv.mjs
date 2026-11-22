// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux
// The web editor's settings-area sectors for fixed key edits (RFCPS_KV_DIR).
import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
const J = `${process.env.RFOUNDRY_DIR || process.env.HOME + "/rfoundry"}/site/static/js/codeplug`;
const { CodeplugImage } = await import(`${J}/image.js`);
const { decodeArea, putKeys, areaForWrite, KV_ADDR, KV_SIZE, getKey, KEYS } = await import(`${J}/kv.js`);
const raw = new Uint8Array(readFileSync(process.argv[2]));
const out = process.argv[3];
mkdirSync(out, { recursive: true });
const sec = (b) => Buffer.from(b.subarray(KV_ADDR, KV_ADDR + KV_SIZE));
const rec = { dmr: 2, fm: 1, hz: [1000, 0, 0, 0, 0, 0, 3400, 0, 0, 0], ms: [200, 0, 0, 0, 0, 0, 900, 0, 0, 0] };
console.log("start state:", decodeArea(raw).state);

// 1: a set of edits on the image as read
const a = Uint8Array.from(raw);
console.log("put1", putKeys(a, { arsServer: 3100001, ipGateway: 4242, callEndTone: false, batteryTone: true, sigBegin: rec }));
writeFileSync(`${out}/kv1.bin`, sec(a));
// 2: a second edit on that: remove the gateway, set another tone
const b = Uint8Array.from(a);
console.log("put2", putKeys(b, { ipGateway: null, totWarnTone: true }));
writeFileSync(`${out}/kv2.bin`, sec(b));
// 3: areaForWrite: the radio's menu changed key 2 and the tone since the file was read; the edit set the ars server
const base = Uint8Array.from(a);
const radio = Uint8Array.from(a);
putKeys(radio, { ipGateway: 9, txDenyTone: true });
const file = Uint8Array.from(a);
putKeys(file, { arsServer: 77 });
areaForWrite(file, radio, base);
writeFileSync(`${out}/kv3.bin`, sec(file));
writeFileSync(`${out}/radio3.bin`, sec(radio));
writeFileSync(`${out}/base3.bin`, sec(base));
writeFileSync(`${out}/file3_before.bin`, sec(((x) => { putKeys(x, { arsServer: 77 }); return x; })(Uint8Array.from(a))));
console.log("done", KEYS.length);
