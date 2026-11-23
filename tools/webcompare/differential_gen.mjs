// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux
// Random edits through the web editor's own model; writes images, exports, problems and a canonical dump per case.
import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
const J = `${process.env.RFOUNDRY_DIR || process.env.HOME + "/rfoundry"}/site/static/js/codeplug`;
const { CodeplugImage } = await import(`${J}/image.js`);
const { decode, encode, remove, newChannel, newContact, validate } = await import(`${J}/model.js`);
const { setKind, newSystem, newHamSystem } = await import(`${J}/channelmodes.js`);
const { exportTable, TABLES, ORDER } = await import(`${J}/csv.js`);
const { exportQdmr, importQdmr } = await import(`${J}/qdmr.js`);
const { exportChirp } = await import(`${J}/chirp.js`);

const [basePath, outDir, nCases, seed0] = [process.argv[2], process.argv[3], Number(process.argv[4] || 80), Number(process.argv[5] || 1)];
mkdirSync(outDir, { recursive: true });
const base = new Uint8Array(readFileSync(basePath));

function rng(seed) { let a = seed >>> 0; return () => { a |= 0; a = (a + 0x6D2B79F5) | 0; let t = Math.imul(a ^ (a >>> 15), 1 | a); t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t; return ((t ^ (t >>> 14)) >>> 0) / 4294967296; }; }
let R;
const rnd = (n) => Math.floor(R() * n);
const pick = (a) => a[rnd(a.length)];
const chance = (p) => R() < p;
const ALPHA = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789 _-./*#";
const word = (max, min = 1) => { const n = min + rnd(max - min + 1); let s = ""; for (let i = 0; i < n; i++) s += ALPHA[rnd(ALPHA.length)]; return s.trim() || "X"; };
const TONES = ["", "67.0", "88.5", "100.0", "123.0", "146.2", "250.3", "D023N", "D023I", "D754I", "D411N"];
const FREQS = [146520000, 146940000, 147060000, 162400000, 162550000, 118000000, 121500000, 88100000, 446000000, 446006250, 433500000, 462562500, 151820000, 70450000, 145500000, 144390000, 439562500];
const ch = (m) => (m.channels.length ? pick(m.channels) : null);
const num = (x) => (x && typeof x === "object" ? x.number ?? null : typeof x === "number" ? x : null);

const OPS = {
	rename: (m) => { const c = ch(m); if (c) c.name = chance(0.05) ? "" : chance(0.05) ? "Sixteen chars!!!" : word(16); },
	freq: (m) => { const c = ch(m); if (c) { c.rxHz = pick(FREQS); c.txHz = chance(0.7) ? c.rxHz : pick(FREQS); } },
	tones: (m) => { const c = ch(m); if (c) { c.rxTone = pick(TONES); c.txTone = pick(TONES); } },
	power: (m) => { const c = ch(m); if (c) c.power = pick([0, 1, 3, 6, 9, 10]); },
	flags: (m) => { const c = ch(m); if (c) for (const k of ["rxOnly", "allSkip", "zoneSkip", "vox", "toneBurst", "noBeep", "noEco", "dmo", "roam", "acceptBadCrc"]) if (chance(0.3)) c[k] = !c[k]; },
	nums: (m) => { const c = ch(m); if (c) { c.bw = pick([12.5, 25]); c.squelch = rnd(22); c.colorCode = rnd(16); c.timeslot = 1 + rnd(2); c.tot = 15 * rnd(255); c.aliasTs1 = rnd(4); c.aliasTs2 = rnd(4); } },
	dmrid: (m) => { const c = ch(m); if (c) c.dmrId = chance(0.5) ? 0 : 1 + rnd(16777215); },
	mode: (m) => { const c = ch(m); if (c && !c.kind) c.mode = pick(["fm", "dmr"]); },
	loc: (m) => { const c = ch(m); if (c) { c.useLocation = chance(0.5); c.lat = Math.round((R() * 180 - 90) * 1e4) / 1e4; c.lon = Math.round((R() * 360 - 180) * 1e4) / 1e4; c.sfr = chance(0.2); c.sfrA = rnd(256); c.sfrB = rnd(256); } },
	refs: (m) => { const c = ch(m); if (!c) return; c.contact = m.contacts.length && chance(0.7) ? pick(m.contacts) : null; c.rxList = m.rxLists.length && chance(0.5) ? pick(m.rxLists) : null; c.aprs = m.aprs.length && chance(0.4) ? pick(m.aprs) : null; },
	addChannel: (m) => { const c = newChannel({ name: word(16), rxHz: pick(FREQS), txHz: pick(FREQS), mode: pick(["fm", "dmr"]), number: chance(0.5) ? 100 + rnd(300) : null }); if (!m.channels.some((x) => x.number === c.number)) m.channels.push(c); },
	delChannel: (m) => { const c = ch(m); if (c) remove(m, "channels", [c]); },
	zoneAdd: (m) => { const members = []; for (let i = 0; i < rnd(12); i++) { const c = ch(m); if (c && !members.includes(c)) members.push(c); } m.zones.push({ name: word(16), channels: members, slot: null }); },
	zoneEdit: (m) => { if (!m.zones.length) return; const z = pick(m.zones); if (chance(0.3)) z.name = word(16); if (chance(0.5) && z.channels.length) z.channels.splice(rnd(z.channels.length), 1); const c = ch(m); if (c && !z.channels.includes(c)) z.channels.push(c); },
	zoneDel: (m) => { if (m.zones.length) m.zones.splice(rnd(m.zones.length), 1); },
	contactAdd: (m) => { m.contacts.push(newContact({ name: word(16), id: 1 + rnd(16777215), type: pick(["group", "private", "all"]), timeslot: rnd(3) })); },
	contactEdit: (m) => { if (!m.contacts.length) return; const c = pick(m.contacts); if (chance(0.5)) c.name = word(16); if (chance(0.5)) c.id = 1 + rnd(16777215); if (chance(0.3)) c.type = pick(["group", "private", "all"]); if (chance(0.3)) c.timeslot = rnd(3); },
	contactDel: (m) => { if (m.contacts.length) remove(m, "contacts", [pick(m.contacts)]); },
	listAdd: (m) => { const members = []; for (let i = 0; i < rnd(8); i++) if (m.contacts.length) { const c = pick(m.contacts); if (!members.includes(c)) members.push(c); } m.rxLists.push({ number: null, name: word(16), contacts: members }); },
	listEdit: (m) => { if (!m.rxLists.length) return; const l = pick(m.rxLists); if (chance(0.2)) l.name = word(16); if (chance(0.5) && l.contacts.length) l.contacts.splice(rnd(l.contacts.length), 1); if (m.contacts.length) { const c = pick(m.contacts); if (!l.contacts.includes(c)) l.contacts.push(c); } if (chance(0.3)) l.contacts.reverse(); },
	manyChannels: (m) => { for (let i = 0; i < 45 + rnd(30); i++) { const c = newChannel({ name: word(16), rxHz: pick(FREQS), txHz: pick(FREQS), mode: "fm", number: 130 + rnd(500) }); if (!m.channels.some((x) => x.number === c.number)) m.channels.push(c); } },
	bigZone: (m) => { const members = []; const want = 70 + rnd(25); for (const c of m.channels) { if (members.length >= want) break; members.push(c); } m.zones.push({ name: word(16), channels: members, slot: null }); },
	zoneShuffle: (m) => { if (!m.zones.length) return; const z = pick(m.zones); for (let i = z.channels.length - 1; i > 0; i--) { const j = rnd(i + 1); [z.channels[i], z.channels[j]] = [z.channels[j], z.channels[i]]; } },
	bigList: (m) => { const members = []; for (const c of m.contacts) { if (members.length >= 30 + rnd(8)) break; members.push(c); } m.rxLists.push({ number: null, name: word(16), contacts: members }); },
	listDel: (m) => { if (m.rxLists.length) remove(m, "rxLists", [pick(m.rxLists)]); },
	dtmfAdd: (m) => { let code = ""; const D = "0123456789ABCD*#abcd"; for (let i = 0; i < 1 + rnd(16); i++) code += D[rnd(D.length)]; m.dtmfContacts.push({ number: null, name: word(16), code }); },
	dtmfEdit: (m) => { if (m.dtmfContacts.length) { const d = pick(m.dtmfContacts); if (chance(0.5)) d.name = word(16); else d.code = "9" + rnd(1000); } },
	dtmfDel: (m) => { if (m.dtmfContacts.length) remove(m, "dtmfContacts", [pick(m.dtmfContacts)]); },
	aprsAdd: (m) => { const path = () => (chance(0.5) ? "" : pick(["WIDE1-1", "WIDE2-2", "RELAY", "TRACE3-3", "A1B2C3-15"])); m.aprs.push({ number: null, name: word(8), ssid: rnd(16), lat: Math.round((R() * 180 - 90) * 1e4) / 1e4, lon: Math.round((R() * 360 - 180) * 1e4) / 1e4, path1: path(), path2: path(), symTable: pick(["/", "\\"]), symCode: pick([">", "-", "k", "#", "_"]), comment: chance(0.5) ? "" : word(23), txHz: chance(0.5) ? 0 : pick([144390000, 145175000]), baud300: chance(0.3), useFixed: chance(0.4), qsy: chance(0.3), silent: chance(0.3), ambiguity: rnd(5) }); },
	aprsEdit: (m) => { if (m.aprs.length) { const a = pick(m.aprs); if (chance(0.5)) a.comment = word(23); if (chance(0.5)) a.lat = Math.round((R() * 180 - 90) * 1e4) / 1e4; if (chance(0.3)) a.ssid = rnd(16); } },
	aprsDel: (m) => { if (m.aprs.length) remove(m, "aprs", [pick(m.aprs)]); },
	general: (m) => { const g = m.general; if (chance(0.5)) g.radioName = word(8); if (chance(0.5)) g.dmrId = chance(0.2) ? 0 : 1 + rnd(16776000); if (chance(0.4)) { g.groupHang = rnd(256); g.privateHang = rnd(256); } if (chance(0.4)) { g.bootLine1 = word(16); g.bootLine2 = word(16); } },
	keys: (m) => { const g = m.general; if (chance(0.5)) g.arsServer = chance(0.3) ? null : rnd(16776415); if (chance(0.5)) g.ipGateway = chance(0.3) ? null : rnd(16776415); for (const k of ["callEndTone", "totWarnTone", "batteryTone", "txDenyTone"]) if (chance(0.4)) g[k] = pick([null, true, false]); if (chance(0.4)) { const hz = () => 10 * rnd(341); const ms = () => 10 * rnd(91); g.sigBegin = chance(0.2) ? null : { dmr: rnd(5), fm: rnd(5), hz: Array.from({ length: 10 }, hz), ms: Array.from({ length: 10 }, ms) }; } },
	unitIds: (m) => { m.modeSettings[2] = { unitId: chance(0.3) ? 0 : 1 + rnd(0xFFFFFE) }; m.modeSettings[4] = { unitId: chance(0.3) ? 0 : 1 + rnd(65519) }; },
	kind: (m) => { const c = ch(m); if (!c) return; const k = pick([0, 2, 4, 2, 4]); if (k) { setKind(m, c, k); if (k === 2) Object.assign(c.params, { squelch: rnd(2), rxNac: pick([0x293, 0x123, 0xF7E, 0x001, 0xFFF]), txNac: pick([0x293, 0x123, 0x456, 0xF7E, 0x293]) }); if (k === 4) { const rx = pick([0, 0, 1, 63, rnd(64)]); Object.assign(c.params, { rate9600: chance(0.5), squelch: rnd(2), rxRan: rx, txRan: pick([rx, rx || 1, 0, 1, 63, rnd(64)]) }); } if (chance(0.3)) c.tagTx = !c.tagTx; } else setKind(m, c, 0, pick(["fm", "dmr"])); },
	sysAdd: (m) => {
		const t = pick([1, 3]); const s = newSystem(m, t, word(16));
		if (t === 3) Object.assign(s.fields, { wacn: chance(0.3) ? null : rnd(0xFFFFF), sysid: chance(0.3) ? null : rnd(0xFFF), ccNac: chance(0.3) ? null : rnd(0xFFF), skipEnc: chance(0.5), planOnly: chance(0.5), noReg: chance(0.3), holdS: rnd(256), roamDbm: rnd(256) - 128 });
		else Object.assign(s.fields, { model: rnd(4), net: rnd(4), category: pick(["a", "b"]), colorCode: rnd(16), allCalls: chance(0.5), privateCalls: chance(0.5), acceptAls: chance(0.3), planBaseHz: chance(0.5) ? 0 : 440000000 + 12500 * rnd(100), planFirstLcn: rnd(4095), planSep: rnd(8), planSplitHz: chance(0.5) ? 0 : 5000000, leaveDbm: rnd(256) - 128, powerSave: rnd(8), fallbackHold: rnd(256), sites: Array.from({ length: rnd(5) }, () => rnd(4)) });
		s.tx = chance(0.5); s.ownId = chance(0.5) ? 0 : 1 + rnd(16776415);
		const nEntries = 1 + rnd(4);
		for (let i = 0; i < nEntries; i++) {
			if (t === 1) s.entries.push({ cls: 0, control: i === 0, flags: 0, chan: 1 + rnd(4094), rxHz: 430000000 + 12500 * rnd(500), txHz: 430000000 + 12500 * rnd(500), cc: chance(0.5) ? null : rnd(16) });
			else { const k = pick([0, 1, 2]); if (k === 0) s.entries.push({ cls: 0, group: rnd(3), control: true, flags: 0, iden: rnd(16), chanNo: rnd(4096), rxHz: 851000000 + 12500 * rnd(500), txHz: chance(0.5) ? 0 : 806000000 + 12500 * rnd(500), chan: 0 }); if (k === 1) s.entries.push({ cls: 1, group: rnd(5), control: false, flags: 0, rfss: chance(0.3) ? null : rnd(255), site: chance(0.3) ? null : rnd(255) }); if (k === 2) s.entries.push({ cls: 2, group: rnd(16), control: false, flags: 0, baseHz: 851006250, spacingHz: 12500, txOffsetHz: chance(0.5) ? -45000000 : 0, bwHz: 12500, tdma: chance(0.3), slots: 1 + rnd(4) }); }
		}
		if (t === 1 && chance(0.5)) { s.needsKey = true; s.secret = { key: Array.from({ length: 16 }, () => rnd(256).toString(16).padStart(2, "0")).join("").toUpperCase(), psn: chance(0.5) ? 0 : rnd(0xFFFFFF) }; }
		m.systems.push(s);
		if (chance(0.7) && m.channels.length) { const c = ch(m); setKind(m, c, t === 1 ? 1 : 3); }
	},
	nxdnSys: (m) => {
		const s = newSystem(m, 4, word(16));
		s.tx = chance(0.7); s.ownId = chance(0.5) ? 0 : chance(0.9) ? 1 + rnd(65519) : 70000;
		const used = new Set();
		for (let i = rnd(4); i > 0; i--) { const id = rnd(64); if (!used.has(id)) { used.add(id); s.trafficKeys.push({ id, seed: 1 + rnd(32767), flags: chance(0.2) ? rnd(256) : 0 }); } }
		m.systems.push(s);
		if (chance(0.7) && m.channels.length) { const c = ch(m); setKind(m, c, 4); c.system = s; if (s.trafficKeys.length && chance(0.6)) c.params.txKeyId = pick(s.trafficKeys).id; else if (chance(0.15)) c.params.txKeyId = rnd(64); }
	},
	keyEdit: (m) => {
		const sys = m.systems.filter((s) => s.type === 4); if (!sys.length) return; const s = pick(sys);
		if (chance(0.4) && s.trafficKeys.length) s.trafficKeys.splice(rnd(s.trafficKeys.length), 1);
		else if (chance(0.5)) { const id = rnd(64); if (!s.trafficKeys.some((k) => k.id === id)) s.trafficKeys.push({ id, seed: 1 + rnd(32767), flags: 0 }); }
		else if (s.trafficKeys.length) pick(s.trafficKeys).seed = 1 + rnd(32767);
	},
	nxdnTx: (m) => { const c = m.channels.find((x) => x.kind === 4 && chance(0.5)); if (!c) return; if (chance(0.3)) delete c.params.txKeyId; else if (c.system && typeof c.system === "object" && c.system.trafficKeys.length) c.params.txKeyId = pick(c.system.trafficKeys).id; },
	sysEdit: (m) => { if (!m.systems.length) return; const s = pick(m.systems); if (chance(0.5)) s.name = word(16); if (chance(0.5)) s.tx = !s.tx; if (s.entries.length && chance(0.5)) s.entries.splice(rnd(s.entries.length), 1); },
	sysDel: (m) => { if (m.systems.length) remove(m, "systems", [pick(m.systems)]); },
};
const WEIGHT = { nxdnSys: 2, keyEdit: 2, nxdnTx: 2, listEdit: 3, manyChannels: 1, bigZone: 2, zoneShuffle: 2, kind: 5, sysAdd: 3, sysEdit: 2, rename: 3, delChannel: 2, zoneEdit: 2, unitIds: 2 };
const OPNAMES = Object.keys(OPS).flatMap((k) => Array(WEIGHT[k] || 1).fill(k));

const cleanProblems = (ps) => ps.map((p) => ({ level: p.level, table: p.table, number: p.item && typeof p.item === "object" ? p.item.number ?? null : null }));

function sysDump(s) {
	return ({ number: s.number, type: s.type, name: s.name, sharing: s.sharing, ownId: s.ownId, tx: s.tx, needsKey: s.needsKey, fields: s.fields, entries: s.entries.map(({ raw, ...e }) => (s.type === 3 && e.cls === 0 && e.chan === undefined ? { ...e, chan: ((e.iden & 15) << 12) | (e.chanNo & 0xFFF) } : e)), hasSecret: !!s.secret, trafficKeys: (s.trafficKeys ?? []).map((k) => ({ id: k.id, seed: k.seed, flags: k.flags, slot: k.slot ?? null })) });
}

function dump(m) {
	const n = (x) => num(x);
	return {
		general: { radioName: m.general.radioName, dmrId: m.general.dmrId, groupHang: m.general.groupHang, privateHang: m.general.privateHang, bootLine1: m.general.bootLine1, bootLine2: m.general.bootLine2,
			arsServer: m.general.arsServer ?? null, ipGateway: m.general.ipGateway ?? null, callEndTone: m.general.callEndTone ?? null, totWarnTone: m.general.totWarnTone ?? null, batteryTone: m.general.batteryTone ?? null, txDenyTone: m.general.txDenyTone ?? null,
			sigBegin: m.general.sigBegin ?? null, sigEnd: m.general.sigEnd ?? null, sigIdle: m.general.sigIdle ?? null },
		modeSettings: { p25: m.modeSettings?.[2]?.unitId ?? 0, nxdn: m.modeSettings?.[4]?.unitId ?? 0 },
		channels: m.channels.map((c) => ({ number: c.number, name: c.name, rxHz: c.rxHz, txHz: c.txHz, mode: c.mode, power: c.power, tot: c.tot, rxTone: c.rxTone, txTone: c.txTone, bw: c.bw, rxOnly: c.rxOnly, allSkip: c.allSkip, zoneSkip: c.zoneSkip, vox: c.vox, squelch: c.squelch, colorCode: c.colorCode, timeslot: c.timeslot, contact: n(c.contact), rxList: n(c.rxList), aprs: n(c.aprs), dmrId: c.dmrId, noBeep: c.noBeep, noEco: c.noEco, dmo: c.dmo, roam: c.roam, aliasTs1: c.aliasTs1, aliasTs2: c.aliasTs2, useLocation: c.useLocation, lat: c.lat, lon: c.lon, sfr: c.sfr, sfrA: c.sfrA, sfrB: c.sfrB, toneBurst: c.toneBurst, acceptBadCrc: c.acceptBadCrc,
			kind: c.kind || 0, tagTx: c.kind ? !!c.tagTx : null, baseOwn: c.kind ? !!c.baseOwn : null, noFallback: c.kind ? !!c.noFallback : null, system: c.kind ? n(c.system) : null, params: c.kind ? c.params : null })),
		zones: m.zones.map((z) => ({ name: z.name, channels: z.channels.map(n) })),
		contacts: m.contacts.map((c) => ({ number: c.number, name: c.name, id: c.id, type: c.type, timeslot: c.timeslot })),
		rxLists: m.rxLists.map((l) => ({ number: l.number, name: l.name, contacts: l.contacts.map(n) })),
		dtmf: m.dtmfContacts.map((d) => ({ number: d.number, name: d.name, code: d.code })),
		aprs: m.aprs.map((a) => ({ number: a.number, name: a.name, ssid: a.ssid, lat: a.lat, lon: a.lon, path1: a.path1, path2: a.path2, symTable: a.symTable, symCode: a.symCode, comment: a.comment, txHz: a.txHz, baud300: a.baud300, useFixed: a.useFixed, qsy: a.qsy, silent: a.silent, ambiguity: a.ambiguity })),
		systems: m.systems.map(sysDump),
	};
}

const files = [];
for (let i = 0; i < nCases; i++) {
	R = rng(seed0 * 100003 + i);
	let m = decode(CodeplugImage.from(Uint8Array.from(base)));
	const nOps = 3 + rnd(14); const log = [];
	for (let k = 0; k < nOps; k++) { const name = pick(OPNAMES); try { OPS[name](m); log.push(name); } catch (e) { log.push(name + "!"); } }
	let bytes;
	try { bytes = encode(m).bytes; } catch (e) { console.log("case", i, "encode failed:", e.message, log.join(",")); continue; }
	const dir = `${outDir}/c${i}`; mkdirSync(dir, { recursive: true });
	writeFileSync(`${dir}/img.bin`, Buffer.from(bytes));
	try { const again = decode(CodeplugImage.from(Uint8Array.from(bytes))); writeFileSync(`${dir}/reenc.bin`, Buffer.from(encode(again).bytes)); } catch (e) { writeFileSync(`${dir}/reenc.bin`, Buffer.from(bytes)); }
	// what a reader decodes, and its exports and problems
	const m2 = decode(CodeplugImage.from(Uint8Array.from(bytes)));
	writeFileSync(`${dir}/dump.json`, JSON.stringify(dump(m2)));
	for (const t of ORDER) writeFileSync(`${dir}/${TABLES[t].file}`, exportTable(m2, t));
	writeFileSync(`${dir}/qdmr.yaml`, exportQdmr(m2).text);
	// the file imported into a fresh read of the image, then what that decodes to and exports as
	try { const m3 = decode(CodeplugImage.from(Uint8Array.from(bytes))); importQdmr(m3, exportQdmr(m2).text); writeFileSync(`${dir}/qdmr_rt_dump.json`, JSON.stringify(dump(m3))); writeFileSync(`${dir}/qdmr_rt.yaml`, exportQdmr(m3).text); } catch (e) { writeFileSync(`${dir}/qdmr_rt.yaml`, "ERR " + e.message); }
	try { writeFileSync(`${dir}/chirp.csv`, exportChirp(m2).text); } catch (e) { writeFileSync(`${dir}/chirp.csv`, "ERR " + e.message); }
	writeFileSync(`${dir}/problems.json`, JSON.stringify(cleanProblems(validate(m2))));
	writeFileSync(`${dir}/ops.txt`, log.join(","));
	files.push(i);
}
// pairs: the model of image i written onto the bytes of image j, by the web editor
const pairs = [];
for (const i of files) {
	const j = files[(files.indexOf(i) * 7 + 3) % files.length];
	if (i === j) continue;
	const a = decode(CodeplugImage.from(new Uint8Array(readFileSync(`${outDir}/c${i}/img.bin`))));
	a.base = CodeplugImage.from(new Uint8Array(readFileSync(`${outDir}/c${j}/img.bin`)));
	try { writeFileSync(`${outDir}/c${i}/onto_${j}.bin`, Buffer.from(encode(a).bytes)); pairs.push([i, j]); } catch (e) { console.log("pair", i, j, "failed:", e.message); }
}
writeFileSync(`${outDir}/index.json`, JSON.stringify({ cases: files, pairs }));
console.log("cases", files.length, "pairs", pairs.length);

// the recommended ham systems (modes' `ham`), for the Rust side to build the same ones
const hams = [];
for (const [type, name, rxHz, txHz, site] of [[1, "", 438100000, 0, 0], [1, "My Site", 438100000, 443100000, 17], [1, "", 440000000, 0, 1022], [3, "", 851012500, 0, 0], [3, "Pacific", 851012500, 806012500, 0], [3, "X", 853000000, 808000000, 0]]) {
	const m = decode(CodeplugImage.from(Uint8Array.from(base)));
	const sys = newHamSystem(m, type, { name, rxHz, txHz, site });
	hams.push({ args: { type, name, rxHz, txHz, site }, system: sysDump(sys) });
}
writeFileSync(`${outDir}/ham.json`, JSON.stringify(hams));
