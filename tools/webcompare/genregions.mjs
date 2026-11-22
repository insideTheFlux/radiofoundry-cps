// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux
// Regenerates crates/core/src/regions.rs from the web editor's regions.js.
const J = `${process.env.RFOUNDRY_DIR || process.env.HOME + "/rfoundry"}/site/static/js/codeplug`;
const { COUNTRIES, ABBREV } = await import(`${J}/sources/regions.js`);
const q = (s) => JSON.stringify(s);
let o = `// SPDX-License-Identifier: MIT
// Copyright (c) 2026 The RadioFoundry Authors
// Copyright (c) 2026 insideTheFlux
//
// RadioID.net's country and state names (no user data), ISO 3166 country codes, and the postal abbreviations of
// US states, Canadian provinces and Australian states. Generated from RadioFoundry's
// \`site/static/js/codeplug/sources/regions.js\`; do not edit by hand.

/// (name, ISO code, states)
pub const COUNTRIES: &[(&str, &str, &[&str])] = &[\n`;
for (const c of COUNTRIES) o += `    (${q(c.name)}, ${q(c.cc)}, &[${c.states.map(q).join(", ")}]),\n`;
o += `];\n\n/// (country, [(state, abbreviation)])\npub const ABBREV: &[(&str, &[(&str, &str)])] = &[\n`;
for (const [c, m] of Object.entries(ABBREV)) o += `    (${q(c)}, &[${Object.entries(m).map(([s, a]) => `(${q(s)}, ${q(a)})`).join(", ")}]),\n`;
o += `];\n`;
console.log(o);
