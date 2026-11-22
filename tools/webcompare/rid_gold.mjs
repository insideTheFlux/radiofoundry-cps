// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux
// The web editor's RadioID user file source: parse, select, format, regions, contacts (RFCPS_RID_DIR).
import { writeFileSync, mkdirSync } from "node:fs";
const J = `${process.env.RFOUNDRY_DIR || process.env.HOME + "/rfoundry"}/site/static/js/codeplug`;
// The web editor's RadioID source: parseUsers, select, regionsOf, format on random user.csv text.
const rid = await import(`${J}/sources/radioid.js`);
const outp = await import(`${J}/sources/output.js`);
const { assignNumbers } = await import(`${J}/model.js`);
const out = process.argv[2]; mkdirSync(out, { recursive: true });
let s = 777; const R = () => { s = (Math.imul(s, 1664525) + 1013904223) >>> 0; return s / 4294967296; };
const rnd = (n) => Math.floor(R() * n);
const pick = (a) => a[rnd(a.length)];
const CALLS = ["k6abc", "DL1XYZ", "w1aw", "JA1AAA", "N0CALL", "f4abc", "VE3XYZ", "VK2ABC", "Q0TST", "ß1ß"];
const NAMES = ["Ann", "José María", "  Hans   Jürgen ", "Łukasz", "Élodie", "", "Mary-Jane", "日本"];
const LASTS = ["Smith", "Müller", "", "O'Brien", "Ørsted"];
const CITIES = ["San Jose", "", "München", "Saint-Étienne", "Toronto", "Sydney"];
const COUNTRIES = ["United States", "Canada", "Australia", "Germany", "Japan", "france", "United States", "", "Narnia"];
const STATES = { "United States": ["California", "Ohio", "New York", "texas", ""], Canada: ["Ontario", "Quebec", ""], Australia: ["Victoria", "Tasmania", ""] };
const rows = [];
const n = 400;
for (let i = 0; i < n; i++) {
	const country = pick(COUNTRIES);
	const state = pick(STATES[country] || ["", "Bayern"]);
	let id = R() < 0.05 ? pick(["0", "abc", "16777216", "-5", ""]) : String(1 + rnd(3000000));
	if (R() < 0.03 && rows.length) id = rows[rnd(rows.length)][0];
	rows.push([id, R() < 0.03 ? "" : pick(CALLS), pick(NAMES), pick(LASTS), pick(CITIES), state, country]);
}
const q = (f) => (/[",]/.test(f) ? `"${f.replace(/"/g, '""')}"` : f);
const variants = {
	std: "RADIO_ID,CALLSIGN,FIRST_NAME,LAST_NAME,CITY,STATE,COUNTRY\n" + rows.map((r) => r.map(q).join(",")).join("\n") + "\n",
	crlf_bom: "﻿" + "radio id,Call Sign,Name,Surname,City,Province,Country\r\n" + rows.map((r) => r.map(q).join(",")).join("\r\n") + "\r\n",
	noheader: rows.map((r) => r.map(q).join(",")).join("\n"),
	shuffled: "country,id,callsign,fname\n" + rows.map((r) => [r[6], r[0], r[1], r[2]].map(q).join(",")).join("\n"),
	badheader: "foo,bar\n" + rows.slice(0, 20).map((r) => r.map(q).join(",")).join("\n"),
	blanklines: "\n\n" + rows.slice(0, 30).map((r) => r.map(q).join(",")).join("\n\n") + "\n\n  \n",
};
const TEMPLATES = ["{call} {name}", "{call}, {city} {st}", "{call} {first} {last} ({cc})", "{id} {call}", "{call}-{country}", "x {call} y", "{foo} {call}", "{call} {foo}", "no tokens", "", "{city} / {state} / {name}", "[{call}]", "{call} {foo}!", "{call} {city}!", "<{state}>"];
const SELS = [
	{}, { regions: [{ country: "United States" }] }, { regions: [{ country: "united states", state: "Ohio" }, { country: "Canada" }] },
	{ prefixes: ["k", "W1"] }, { prefixes: [" ", ""] }, { idMin: 100000, idMax: 2000000 }, { regions: [{ country: "Narnia" }], prefixes: ["Q"] },
	{ regions: [{ country: "Australia", state: "" }] },
];
const cases = [];
for (const [name, text] of Object.entries(variants)) {
	const users = rid.parseUsers(text);
	const fmts = [];
	for (const t of TEMPLATES) for (const [upper, max] of [[false, 0], [true, 16], [false, 24], [true, 48]])
		fmts.push({ t, upper, max, out: users.slice(0, 40).map((u) => rid.format(u, t, { upper, max })) });
	const sels = SELS.map((sel) => ({ sel, ids: rid.select(users, sel).map((u) => u.id) }));
	const adds = [];
	for (const [t, upper, nExisting, take] of [["{call} {name}", false, 0, 50], ["{name} {last} Łukasz", true, 5, 100], ["{id}", false, 1020, 30], ["", false, 3, 20], ["{city} {state} {country} {name} {last}", false, 2, 60]]) {
		const m = { contacts: [] };
		for (let i = 0; i < nExisting; i++) m.contacts.push({ number: i % 2 ? i + 1 : null, name: `C${i}`, id: i % 3 ? users[i % users.length]?.id ?? 7 : users[i % users.length].id, type: i % 3 ? "private" : "group", timeslot: 0 });
		assignNumbers(m.contacts, 1024);
		const before = m.contacts.map((c) => ({ number: c.number, name: c.name, id: c.id, type: c.type, timeslot: c.timeslot }));
		const sel = users.slice(0, take);
		const r = outp.addContacts(m, sel, { template: t, upper });
		assignNumbers(m.contacts, 1024);
		adds.push({ t, upper, before, take, r, after: m.contacts.map((c) => ({ number: c.number, name: c.name, id: c.id, type: c.type, timeslot: c.timeslot })) });
	}
	cases.push({ name, text, users, fmts, sels, adds, regions: rid.regionsOf(users) });
}
writeFileSync(`${out}/rid.json`, JSON.stringify(cases));
console.log(cases.map((c) => `${c.name}:${c.users.length}`).join(" "));
