#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Copyright (c) 2026 insideTheFlux
#
# Builds the web editor's side of every golden test from a RadioFoundry checkout, then runs the Rust tests against it.
# No radio, no radio image and no personal data are needed: the starting image is made up (make-base.mjs).
#
#   tools/webcompare/run-all.sh [PATH_TO_RADIOFOUNDRY_CHECKOUT]
#
# The checkout is also read from RFOUNDRY_DIR (default ~/rfoundry). Needs Node 18 or newer and cargo.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "$here/../.." && pwd)"
export RFOUNDRY_DIR="${1:-${RFOUNDRY_DIR:-$HOME/rfoundry}}"
if [ ! -f "$RFOUNDRY_DIR/site/static/js/codeplug/model.js" ]; then
  echo "No RadioFoundry checkout at $RFOUNDRY_DIR (set RFOUNDRY_DIR or pass the path)." >&2
  exit 2
fi
command -v node >/dev/null || { echo "node is needed" >&2; exit 2; }

out="${WEBCOMPARE_OUT:-$(mktemp -d)}"
mkdir -p "$out"
echo "web editor from: $RFOUNDRY_DIR"
echo "fixtures in:     $out"
cd "$here"

echo "-- starting image"
node make-base.mjs "$out/base.bin"

echo "-- decode, encode, exports"
node golden.mjs "$out/base.bin" "$out/golden.json" >/dev/null
node edit_js.mjs "$out/base.bin" "$out/edited.bin" >/dev/null
node csv6_js.mjs "$out/base.bin" "$out/csv" >/dev/null
node chirp_js.mjs "$out/base.bin" "$out/chirp.csv" >/dev/null
mkdir -p "$out/qdmr"
node qdmr_js.mjs "$out/base.bin" "$out/qdmr" >/dev/null

echo "-- settings area, modes, systems"
node edit_kv.mjs "$out/base.bin" "$out/kv" >/dev/null
node edit_modes.mjs "$out/base.bin" "$out/modes" >/dev/null
node edit_systems.mjs "$out/base.bin" "$out/sys" >/dev/null
node edit_t3.mjs "$out/base.bin" "$out/t3" >/dev/null

echo "-- DTMF and APRS (tables, CSV, qdmr)"
node edit_aprs.mjs "$out/base.bin" "$out/aprs" >/dev/null
node csv6_js.mjs "$out/aprs/aprs1.bin" "$out/aprs/csv" >/dev/null
mkdir -p "$out/aprs/qdmr"
cp fixtures/rich_aprs.yaml "$out/aprs/qdmr/rich.yaml"
node qdmr_aprs_js.mjs "$out/aprs/aprs1.bin" "$out/base.bin" "$out/aprs/qdmr" >/dev/null

echo "-- ID database and RadioID"
node iddb_gold.mjs "$out/iddb" >/dev/null
node iddbc_gold.mjs "$out/iddbc" >/dev/null
node rid_gold.mjs "$out/rid" >/dev/null
node ridapi_gold.mjs "$out/ridapi" >/dev/null

echo "-- random edits (differential): ${DIFF_SEEDS:-3} seeds of ${DIFF_CASES:-100} cases"
node differential_prep.mjs "$out/base.bin" "$out/prepared.bin" >/dev/null
for seed in $(seq 1 "${DIFF_SEEDS:-3}"); do
  node differential_gen.mjs "$out/prepared.bin" "$out/fuzz$seed" "${DIFF_CASES:-100}" "$seed" >/dev/null
done

echo "-- the Rust tests"
cd "$repo"
RFCPS_IMAGE="$out/base.bin" \
RFCPS_GOLDEN="$out/golden.json" \
RFCPS_EDITED_JS="$out/edited.bin" \
RFCPS_CSV_DIR="$out/csv" \
RFCPS_CHIRP_CSV="$out/chirp.csv" \
RFCPS_QDMR_DIR="$out/qdmr" \
RFCPS_KV_DIR="$out/kv" \
RFCPS_MODES_DIR="$out/modes" \
RFCPS_SYS_DIR="$out/sys" \
RFCPS_T3_DIR="$out/t3" \
RFCPS_APRS_DIR="$out/aprs" \
RFCPS_IDDB_DIR="$out/iddb" \
RFCPS_IDDBC_DIR="$out/iddbc" \
RFCPS_RID_DIR="$out/rid" \
  cargo test -p rfcps-core --test golden -- --nocapture --test-threads=1 2>&1 | tee "$out/golden.log" | grep -E "^test |test result|panicked|skipped" || true
RFCPS_RIDAPI_DIR="$out/ridapi" \
  cargo test -p rfcps radioid_api::tests::from_api_matches_the_web_editor -- --nocapture 2>&1 | tee "$out/app.log" | grep -E "^test |test result|panicked|skipped" || true

rm -f "$out/differential.log"
for seed in $(seq 1 "${DIFF_SEEDS:-3}"); do
  RFCPS_FUZZ_DIR="$out/fuzz$seed" \
    cargo test -p rfcps-core --test differential -- --nocapture 2>&1 | tee -a "$out/differential.log" | grep -E "^test |test result|mismatch|panicked" || true
done
cargo test -p rfcps-link --test lz4_spec 2>&1 | tee "$out/lz4.log" | grep -E "^test |test result" || true

passed=$(cat "$out/golden.log" "$out/app.log" "$out/differential.log" "$out/lz4.log" | sed -n 's/^test result: ok\. \([0-9]*\) passed.*/\1/p' | awk '{n+=$1} END {print n+0}')
if grep -q "FAILED\|panicked" "$out/golden.log" "$out/app.log" "$out/differential.log" "$out/lz4.log" || ! grep -q "test result: ok" "$out/golden.log" "$out/app.log" "$out/differential.log" "$out/lz4.log"; then
  echo "SOME TESTS FAILED: see the logs in $out (golden.log, app.log, differential.log, lz4.log)" >&2
  exit 1
fi
skipped=$(cat "$out/golden.log" "$out/app.log" "$out/differential.log" "$out/lz4.log" | grep -c "not set: skipped" || true)
echo "$((passed - skipped)) tests passed (golden tests against the web editor, ${DIFF_SEEDS:-3} x ${DIFF_CASES:-100} random-edit cases, and the LZ4 stream rules); ${skipped} skipped: the stock-backup import needs a stock DM-32UV backup of your own"
