// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux
// The web editor's import of a stock DM-32UV backup (RFCPS_STOCK_GOLDEN); needs a backup of your own.
import { readFileSync, writeFileSync } from "node:fs";
const J = `${process.env.RFOUNDRY_DIR || process.env.HOME + "/rfoundry"}/site/static/js/codeplug`;
const { empty } = await import(`${J}/model.js`);
const { importStock, detectStock } = await import(`${J}/stock_baofeng.js`);
const bytes = new Uint8Array(readFileSync(process.argv[2]));
const m = empty();
const r = importStock(m, bytes);
const ref = (x) => (x && typeof x === "object" ? x.number ?? null : x ?? null);
const out = {
  detected: detectStock(bytes), counts: r.counts, notes: r.notes,
  general: { dmrId: m.general.dmrId, radioName: m.general.radioName, bootLine1: m.general.bootLine1, bootLine2: m.general.bootLine2 },
  channels: m.channels.map((c) => ({ number: c.number, name: c.name, rxHz: c.rxHz, txHz: c.txHz, mode: c.mode, power: c.power, rxTone: c.rxTone, txTone: c.txTone, bw: c.bw, colorCode: c.colorCode, timeslot: c.timeslot, contact: ref(c.contact), rxList: ref(c.rxList) })),
  zones: m.zones.map((z) => ({ name: z.name, channels: z.channels.map(ref) })),
  contacts: m.contacts.map((c) => ({ number: c.number, name: c.name, id: c.id, type: c.type })),
  rxLists: m.rxLists.map((l) => ({ number: l.number, name: l.name, contacts: l.contacts.map(ref) })),
};
writeFileSync(process.argv[3], JSON.stringify(out, null, 1));
console.log(out.detected, JSON.stringify(out.counts), JSON.stringify(out.notes));
