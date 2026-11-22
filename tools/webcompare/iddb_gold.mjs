// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux
// The web editor's DMR ID database in format N (RFCPS_IDDB_DIR).
import { writeFileSync, mkdirSync } from "node:fs";
const J = `${process.env.RFOUNDRY_DIR || process.env.HOME + "/rfoundry"}/site/static/js/codeplug`;
// The web editor's format N database for fixed and random record sets: records, parameters, the bytes it writes, what it reads back.
const { CodeplugImage } = await import(`${J}/image.js`);
const iddb = await import(`${J}/iddb.js`);
const out = process.argv[2]; mkdirSync(out, { recursive: true });
let s = 12345; const R = () => { s = (Math.imul(s, 1664525) + 1013904223) >>> 0; return s / 4294967296; };
const rnd = (n) => Math.floor(R() * n);
const NAMES = ["K6ABC Ann Smith San Jose CA", "José Müller Łódź", "DL1XYZ Hans-Jürgen München", "  spaced   out  ", "Æsir ßtraße Ørsted", "日本語 JA1AAA Tokyo", "N0CALL.1", "a", "", "!!!", "Ünïcödé Çafé Naïve", "F4ABC Élodie Saint-Étienne France", "X".repeat(80)];
const text = () => NAMES[rnd(NAMES.length)] + (R() < 0.3 ? ` ${rnd(1000)}` : "");
const cases = [];
const add = (name, count, textLen, end, opts = {}) => {
	const recs = [];
	for (let i = 0; i < count; i++) recs.push({ id: opts.seq ? i + 1 : opts.wide ? 1 + rnd(0xFFFFFF) : 1 + rnd(5000000), text: text() });
	if (opts.bad) recs.push({ id: 0, text: "zero" }, { id: 0x1000000, text: "big" }, { id: 7, text: "dup1" }, { id: 7, text: "dup2" });
	cases.push({ name, recs, textLen, end });
};
add("empty", 0, 32, null);
add("one", 1, 16, null);
add("small16", 50, 16, null, { bad: true });
add("small24", 50, 24, null, { bad: true });
add("small32", 500, 32, null, { bad: true, wide: true });
add("small48", 500, 48, null, { bad: true });
add("area2_16", 17500, 16, null, { seq: true });
add("area2_32", 10000, 32, null, { wide: true });
add("limited_end", 10000, 16, 0x20000 + 0xB8000 + 15 * 20, {});
add("limited_none", 20000, 16, 0x20000 + 0xB8000, {});
add("end_in_area1", 100, 32, 0x20000 + 0x30000 + 100, {});
for (const c of cases) {
	const img = CodeplugImage.blank();
	const res = iddb.write(img, c.recs, { format: "N", textLen: c.textLen, end: c.end ?? Infinity });
	const hdr = iddb.readHeader(img.bytes);
	const info = { count: res.count, top: res.top, dropped: res.dropped, rec: res.rec, header: hdr, read: iddb.read(img.bytes), extent: iddb.extent(img.bytes) };
	writeFileSync(`${out}/${c.name}.json`, JSON.stringify({ recs: c.recs, textLen: c.textLen, end: c.end, expect: info }));
	writeFileSync(`${out}/${c.name}.bin`, Buffer.from(img.bytes.subarray(iddb.AREA1, res.top)));
}
writeFileSync(`${out}/index.json`, JSON.stringify(cases.map((c) => c.name)));
console.log("cases", cases.length);
