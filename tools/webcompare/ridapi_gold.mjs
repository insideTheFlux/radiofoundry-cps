// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux
// The web editor's reading of RadioID API replies (RFCPS_RIDAPI_DIR).
import { writeFileSync, mkdirSync } from "node:fs";
const J = `${process.env.RFOUNDRY_DIR || process.env.HOME + "/rfoundry"}/site/static/js/codeplug`;
// The web editor's fromApi on random API replies.
const rid = await import(`${J}/sources/radioid.js`);
const out = process.argv[2]; mkdirSync(out, { recursive: true });
let s = 99; const R = () => { s = (Math.imul(s, 1664525) + 1013904223) >>> 0; return s / 4294967296; };
const rnd = (n) => Math.floor(R() * n);
const pick = (a) => a[rnd(a.length)];
const idv = () => pick([() => 1 + rnd(3000000), () => String(1 + rnd(3000000)), () => 0, () => "", () => null, () => 16777216, () => 16777215, () => "abc", () => " 42 ", () => true, () => undefined, () => -3, () => "0x10"])();
const strv = (list) => pick([() => pick(list), () => null, () => undefined, () => "", () => 0, () => 7, () => false, () => true, () => "  padded  "])();
const rows = (n) => Array.from({ length: n }, () => {
	const r = {};
	const a = R();
	if (a < 0.7) r.id = idv(); else if (a < 0.85) r.radio_id = idv(); else { r.id = null; r.radio_id = idv(); }
	r.callsign = strv(["k6abc", "W1AW", "dl1xyz", "  ", "n0call"]);
	r.fname = strv(["Ann", "José", " Hans "]); r.surname = strv(["Smith", "Ørsted"]); r.city = strv(["Berlin", "San Jose"]);
	r.state = strv(["Ohio", "Bayern"]); r.country = strv(["United States", "Germany"]);
	for (const k of Object.keys(r)) if (r[k] === undefined) delete r[k];
	return r;
});
const cases = [];
for (const n of [0, 1, 5, 40, 200]) { const results = JSON.parse(JSON.stringify(rows(n))); cases.push({ results, users: rid.fromApi(results) }); }
cases.push({ results: null, users: rid.fromApi(null) });
writeFileSync(`${out}/api.json`, JSON.stringify(cases));
console.log(cases.map((c) => c.users.length).join(" "));
