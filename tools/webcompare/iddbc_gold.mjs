// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux
// The web editor's DMR ID database in format C and the conversions (RFCPS_IDDBC_DIR).
import { writeFileSync, mkdirSync } from "node:fs";
const J = `${process.env.RFOUNDRY_DIR || process.env.HOME + "/rfoundry"}/site/static/js/codeplug`;
// The web editor's format C (and convert) on fixed and random record sets, as steps on one image.
const { CodeplugImage } = await import(`${J}/image.js`);
const iddb = await import(`${J}/iddb.js`);
const out = process.argv[2]; mkdirSync(out, { recursive: true });
let s = 424242; const R = () => { s = (Math.imul(s, 1664525) + 1013904223) >>> 0; return s / 4294967296; };
const rnd = (n) => Math.floor(R() * n);
const pick = (a) => a[rnd(a.length)];
const gen = (n, o = {}) => {
	const firsts = Array.from({ length: o.firsts ?? 300 }, (_, i) => ["Ann", "Bob", "Carl", "Dora", "Emil", "Fay", "Gus", "Hana"][i % 8] + (i >= 8 ? String(i) : ""));
	const cities = Array.from({ length: o.cities ?? 250 }, (_, i) => ["San Jose", "Berlin", "Tokyo", "Paris", "Rome", "Oslo", "Lima"][i % 7] + (i >= 7 ? ` ${i}` : ""));
	const sts = ["CA", "OH", "NY", "TX", "BY", "BE", ""];
	const recs = [];
	let id = 1 + rnd(1000);
	for (let i = 0; i < n; i++) {
		id += 1 + (R() < 0.02 ? rnd(200000) : rnd(40));
		if (id > 0xFFFFFF) break;
		const call = `${pick(["K", "W", "N", "DL", "JA", "F", "VK"])}${rnd(10)}${String.fromCharCode(65 + rnd(26))}${String.fromCharCode(65 + rnd(26))}${R() < 0.5 ? String.fromCharCode(65 + rnd(26)) : ""}`;
		recs.push({ id, text: `${call} ${pick(firsts)} ${pick(cities)} ${pick(sts)}`.trim() });
	}
	return recs;
};
const cases = [];
const L = (n) => 0x20000 + 0xB8000 + n;
const add = (name, steps) => cases.push({ name, steps });
const w = (format, records, o = {}) => ({ op: "write", format, records, end: o.end ?? null, fresh: !!o.fresh, textLen: o.textLen ?? 32 });
add("c_empty", [w("C", [])]);
add("c_one", [w("C", [{ id: 5, text: "K6ABC Ann" }])]);
add("c_small", [w("C", gen(200))]);
add("c_mid", [w("C", gen(5000))]);
add("c_big", [w("C", gen(40000, { firsts: 900, cities: 700 }))]);
const base = gen(3000);
const refreshed = base.filter(() => R() > 0.07).concat(gen(300).map((r) => ({ ...r, id: r.id + 3 }))).map((r) => (R() < 0.04 ? { ...r, text: r.text + " X" } : r));
add("c_refresh", [w("C", base), w("C", refreshed)]);
add("c_refresh_twice", [w("C", base), w("C", refreshed), w("C", base.slice(100))]);
add("c_growth_splits", [w("C", gen(1500)), w("C", gen(9000, { firsts: 600 }))]);
add("c_fresh_flag", [w("C", base), w("C", refreshed, { fresh: true })]);
add("c_limited", [w("C", gen(30000, { firsts: 800, cities: 600 }), { end: L(0x5000) })]);
add("c_no_room", [w("C", gen(9000), { end: 0x20000 + 0x30000 })]);
add("c_limited_refresh", [w("C", gen(4000)), w("C", gen(12000), { end: L(0x1000) })]);
const odd = [{ id: 1, text: "" }, { id: 2, text: "A" }, { id: 3, text: "A B" }, { id: 4, text: "A B C D E" }, { id: 5, text: "X".repeat(80) }, { id: 6, text: "José Müller Łódź 日本" },
	{ id: 0xFFFFFF, text: "LAST ID" }, { id: 100000, text: "big gap" }, { id: 7, text: "dup one" }, { id: 7, text: "dup two" }, { id: 0, text: "zero" }, { id: 0x1000000, text: "too big" },
	{ id: 20, text: "   spaced   out  words " }, { id: 21, text: "a.b c.d e.f" }];
for (let i = 0; i < 300; i++) odd.push({ id: 200 + i * 3, text: i % 3 ? "Repeat Name Here" : "Other Repeat X" });
add("c_odd_texts", [w("C", odd)]);
add("c_odd_refresh", [w("C", odd), w("C", odd.slice(3, 200))]);
add("n_to_c_to_n", [w("N", gen(3000), { textLen: 32 }), { op: "convert", format: "C", end: null }, { op: "convert", format: "N", end: null }]);
add("convert_same_and_none", [{ op: "convert", format: "C", end: null }, w("N", gen(50), { textLen: 16 }), { op: "convert", format: "N", end: null }]);
add("c_to_n_limited", [w("C", gen(20000, { firsts: 500 })), { op: "convert", format: "N", end: L(0x2000) }]);
add("c_erase_write", [w("C", gen(500)), { op: "erase" }, w("C", gen(400))]);
add("n_over_c", [w("C", gen(800)), w("N", gen(300), { textLen: 24 })]);
add("c_over_n", [w("N", gen(800), { textLen: 48 }), w("C", gen(300))]);
add("c_limited_odd_end", [w("C", gen(30000, { firsts: 800, cities: 600 }), { end: L(0x5000 + 1234) })]);
{
	const b2 = gen(2500);
	const lo = b2[1000].id, cluster = [];
	for (let i = 0; i < 407; i++) cluster.push({ id: lo + 1 + i * 0 + (i % 4000), text: `K${i % 10}ZZ Cluster Name ${i}` });
	const seen = new Set(b2.map((r) => r.id));
	const extra = cluster.filter((r) => !seen.has(r.id));
	add("c_page_splits", [w("C", b2), w("C", b2.concat(extra))]);
	add("c_page_splits_odd", [w("C", b2), w("C", b2.concat(extra.slice(0, 301)))]);
}
{
	const recs = [];
	let id = 100;
	for (let i = 0; i < 17300 * 3; i++) { id += 1 + rnd(3); recs.push({ id, text: `X${i % 9} N${String(i % 17300).padStart(5, "0")}` }); }
	add("c_dict_full", [w("C", recs)]);
}
const index = [];
for (const c of cases) {
	const img = CodeplugImage.blank();
	const results = [];
	let lastRecs = null, lastEnd = Infinity;
	for (const st of c.steps) {
		const end = st.end ?? Infinity;
		if (st.op === "write") {
			const r = iddb.write(img, st.records, { format: st.format, textLen: st.textLen, end, fresh: st.fresh });
			results.push({ count: r.count, top: r.top, dropped: r.dropped, format: r.format });
			if (st.format === "C") { lastRecs = st.records; lastEnd = end; }
		} else if (st.op === "convert") {
			const r = iddb.convert(img, st.format, { end });
			results.push(r ? { count: r.count, top: r.top, dropped: r.dropped, format: r.format } : null);
		} else { iddb.erase(img); results.push(null); }
	}
	const h = iddb.readHeader(img.bytes);
	const est = lastRecs ? iddb.estimateCompact(lastRecs, lastEnd) : null;
	const exp = { results, header: h && { format: h.format, count: h.count, textLen: h.textLen }, extent: iddb.extent(img.bytes), read: iddb.read(img.bytes),
		est: est && { bytes: est.bytes, fit: est.fit === Infinity ? null : est.fit }, estRecs: lastRecs, estEnd: lastEnd === Infinity ? null : lastEnd };
	writeFileSync(`${out}/${c.name}.json`, JSON.stringify({ steps: c.steps, expect: exp }));
	writeFileSync(`${out}/${c.name}.bin`, img.bytes);
	index.push(c.name);
}
writeFileSync(`${out}/index.json`, JSON.stringify(index));
console.log(index.length, "cases");
