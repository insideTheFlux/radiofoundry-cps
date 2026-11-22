// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux
// The web editor's decode of an image, as JSON (RFCPS_GOLDEN).
import { readFileSync, writeFileSync } from "node:fs";
const J = `${process.env.RFOUNDRY_DIR || process.env.HOME + "/rfoundry"}/site/static/js/codeplug`;
const { CodeplugImage } = await import(`${J}/image.js`);
const { decode } = await import(`${J}/model.js`);
const img = CodeplugImage.from(new Uint8Array(readFileSync(process.argv[2])));
const m = decode(img);
const ref = (x) => (x && typeof x === "object" ? x.number ?? null : x ?? null);
const out = {
  general: m.general,
  channels: m.channels.map((c) => ({ number: c.number, name: c.name, rxHz: c.rxHz, txHz: c.txHz, mode: c.mode, power: c.power,
    tot: c.tot, rxTone: c.rxTone, txTone: c.txTone, bw: c.bw, rxOnly: c.rxOnly, squelch: c.squelch, colorCode: c.colorCode,
    timeslot: c.timeslot, contact: ref(c.contact), rxList: ref(c.rxList), dmrId: c.dmrId })),
  zones: m.zones.map((z) => ({ slot: z.slot, name: z.name, channels: z.channels.map(ref) })),
  contacts: m.contacts.map((c) => ({ number: c.number, name: c.name, id: c.id, type: c.type, timeslot: c.timeslot })),
  rxLists: m.rxLists.map((l) => ({ number: l.number, name: l.name, contacts: l.contacts.map(ref) })),
};
writeFileSync(process.argv[3], JSON.stringify(out, null, 1));
console.log("channels", out.channels.length, "zones", out.zones.length, "contacts", out.contacts.length, "rxLists", out.rxLists.length);
console.log("general", JSON.stringify(out.general).slice(0, 200));
