// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux
// The web editor's CSV export of the six tables of an image (RFCPS_CSV_DIR, and csv/ of RFCPS_APRS_DIR).
import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
const J = `${process.env.RFOUNDRY_DIR || process.env.HOME + "/rfoundry"}/site/static/js/codeplug`;
const { CodeplugImage } = await import(`${J}/image.js`);
const { decode } = await import(`${J}/model.js`);
const { exportTable, TABLES, ORDER } = await import(`${J}/csv.js`);
const m = decode(CodeplugImage.from(new Uint8Array(readFileSync(process.argv[2]))));
mkdirSync(process.argv[3], { recursive: true });
for (const t of ORDER) writeFileSync(`${process.argv[3]}/${TABLES[t].file}`, exportTable(m, t));
console.log("tables:", ORDER.join(","));
