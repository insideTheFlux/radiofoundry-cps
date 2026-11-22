<!-- SPDX-License-Identifier: MIT -->
<!-- Copyright (c) 2026 insideTheFlux -->

# Comparing with the web editor

This editor is meant to match RadioFoundry's web Codeplug Editor (`site/static/js/codeplug/*.js`). The golden tests in
`crates/core/tests/golden.rs` (and one in `crates/app/src/radioid_api.rs`) check that: each one compares the Rust code's
result with the web editor's own JavaScript result for the same input. The scripts here make the web editor's side.

## Run it

```
tools/webcompare/run-all.sh [PATH_TO_RADIOFOUNDRY_CHECKOUT]
```

It needs Node 18 or newer, `cargo`, and a RadioFoundry checkout (the path is also read from `RFOUNDRY_DIR`, default
`~/rfoundry`). Linux, macOS or WSL. It takes about a minute and prints how many tests were compared and whether they all
passed. `WEBCOMPARE_OUT=DIR` keeps the generated files in `DIR` (otherwise a temporary folder, shown at the start).

No radio, no radio image and no personal data are used. The starting image is made up by `make-base.mjs`, through the web
editor's own model: 39 channels, 7 zones, 10 contacts and 2 RX lists.

## What each script makes

| Script | Test variable | What |
|---|---|---|
| `make-base.mjs` | `RFCPS_IMAGE` | the made-up starting image |
| `golden.mjs` | `RFCPS_GOLDEN` | the web editor's decode of the image, as JSON |
| `edit_js.mjs` | `RFCPS_EDITED_JS` | its encode of a fixed set of edits |
| `csv6_js.mjs` | `RFCPS_CSV_DIR`, `csv/` of `RFCPS_APRS_DIR` | its CSV export of the six tables |
| `chirp_js.mjs` | `RFCPS_CHIRP_CSV` | its CHIRP CSV export |
| `qdmr_js.mjs` | `RFCPS_QDMR_DIR` | its qdmr YAML export, and an export imported again |
| `edit_kv.mjs` | `RFCPS_KV_DIR` | the settings-area sectors for fixed key edits, with the radio-merge case |
| `edit_modes.mjs` | `RFCPS_MODES_DIR` | whole images for P25 and NXDN channel edits |
| `edit_systems.mjs` | `RFCPS_SYS_DIR` | whole images for P25 trunked system edits |
| `edit_t3.mjs` | `RFCPS_T3_DIR` | whole images for DMR Tier III system edits |
| `edit_aprs.mjs` | `RFCPS_APRS_DIR` | whole images for DTMF contact and APRS configuration edits |
| `qdmr_aprs_js.mjs` | `qdmr/` of `RFCPS_APRS_DIR` | qdmr export and import with DTMF and APRS (`fixtures/rich_aprs.yaml` is the file imported) |
| `iddb_gold.mjs` | `RFCPS_IDDB_DIR` | the DMR ID database in format N |
| `iddbc_gold.mjs` | `RFCPS_IDDBC_DIR` | format C, and the conversions between the formats |
| `rid_gold.mjs` | `RFCPS_RID_DIR` | the RadioID user file source: parsing, selecting, name templates, contacts |
| `ridapi_gold.mjs` | `RFCPS_RIDAPI_DIR` | the reading of RadioID API replies |
| `differential_prep.mjs`, `differential_gen.mjs` | `RFCPS_FUZZ_DIR` | random edits (see below) |
| `genregions.mjs` | | regenerates `crates/core/src/regions.rs` from the web editor's `regions.js` (not a test) |

A test passes only when the Rust output equals the web editor's byte for byte (images) or character for character (text).
A test that is not given its variable passes without checking anything and says so, so a plain `cargo test` needs none of
this.

## Random edits (the differential test)

`differential_prep.mjs` adds fixed RX lists, DTMF contacts, APRS configurations, P25 and Tier III systems and channel tags
to the starting image. `differential_gen.mjs` then applies random edits (3 to 16 per case: renames, frequencies, tones,
flags, adds and deletes of every table, systems, modes) through the web editor's own model and writes, for each case, the
image it encodes, a canonical dump of what it decodes, its CSV, qdmr and CHIRP exports and its problems; for pairs of cases
it also writes the first one's model onto the second one's bytes. `crates/core/tests/differential.rs` makes the Rust code do
the same and compares everything, reporting every mismatch with the edits that led to it. The CPS-only weather-channel
warning is left out of the problem comparison.

`run-all.sh` runs 3 seeds of 100 cases (about 1,200 comparisons each). More: `DIFF_SEEDS=14 DIFF_CASES=100 run-all.sh`.
`crates/link/tests/lz4_spec.rs` is an ordinary test (no web editor needed): the streams the editor sends are valid LZ4
blocks that obey the format's end rules and the protocol's size limit.

## What is not covered here

- **The stock DM-32UV import** (`stock_js.mjs`, `RFCPS_STOCK_DATA` and `RFCPS_STOCK_GOLDEN`) needs a backup made from a
  stock DM-32UV, which cannot be made up. It is skipped by `run-all.sh`; run it with your own backup if you have one.
- **Tests on a real radio** are separate: see `docs/testing.md`.

## Checking that the tests can fail

Plant a bug in the Rust (for example change a byte `write_n` puts in the ID database header, or the 25.0 kHz bandwidth in
`decode_channel`) and run `run-all.sh`: it reports the failing tests, the differential test lists its mismatches, and the
script exits with an error. Put the bug back and it passes again.
