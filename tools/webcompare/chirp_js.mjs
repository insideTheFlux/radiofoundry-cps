// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux
// The web editor's CHIRP CSV export of an image (RFCPS_CHIRP_CSV).
import { readFileSync, writeFileSync } from "node:fs";
const J = `${process.env.RFOUNDRY_DIR || process.env.HOME + "/rfoundry"}/site/static/js/codeplug`;
const { CodeplugImage } = await import(`${J}/image.js`);
const { decode } = await import(`${J}/model.js`);
const { exportChirp } = await import(`${J}/chirp.js`);
const m = decode(CodeplugImage.from(new Uint8Array(readFileSync(process.argv[2]))));
const r = exportChirp(m);
writeFileSync(process.argv[3], r.text);
console.log(JSON.stringify(r.notes));
