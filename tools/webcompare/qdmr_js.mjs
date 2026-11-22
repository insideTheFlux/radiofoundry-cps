// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux
// The web editor's qdmr YAML export of an image, and of an export imported again (RFCPS_QDMR_DIR).
import { readFileSync, writeFileSync } from "node:fs";
const J = `${process.env.RFOUNDRY_DIR || process.env.HOME + "/rfoundry"}/site/static/js/codeplug`;
const { CodeplugImage } = await import(`${J}/image.js`);
const { decode } = await import(`${J}/model.js`);
const { exportQdmr, importQdmr } = await import(`${J}/qdmr.js`);
const img = () => CodeplugImage.from(new Uint8Array(readFileSync(process.argv[2])));
const m = decode(img());
const e = exportQdmr(m);
writeFileSync(process.argv[3] + "/qdmr_js.yaml", e.text);
console.log("export notes", JSON.stringify(e.notes), "aprs", m.aprs.length, "dtmf", m.dtmfContacts.length);
// import the JS export into a fresh decode, then export again
const m2 = decode(img());
const r = importQdmr(m2, e.text);
console.log("import", JSON.stringify(r));
writeFileSync(process.argv[3] + "/qdmr_js_roundtrip.yaml", exportQdmr(m2).text);
