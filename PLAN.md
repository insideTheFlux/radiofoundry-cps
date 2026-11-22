# Plan

## Goal

A native editor for RadioFoundry radios: a Windows 11 (x86-64) `.exe` and a Linux program, all Rust, no browser. It stays
at parity with RadioFoundry's web Codeplug Editor (`site/static/js/codeplug/` in the RadioFoundry repository) and follows
the specs in its `docs/spec/`. First radio: the DM-32UV.

## Status

| Area | State |
|---|---|
| Codeplug image: channels, VFOs, zones, contacts, RX group lists, general settings, the settings area | done, matches the web editor |
| DTMF contacts, APRS configurations | done, matches the web editor |
| P25 and NXDN conventional channels, P25 trunked systems, DMR Tier III systems | done, matches the web editor |
| Read and write over the programming cable (the fast codeplug protocol) | done: only changed blocks, a backup first, a refusal if the radio changed, a read-back check |
| Import and export: stock DM-32UV backup, qdmr YAML, CHIRP CSV, CSV tables | done, matches the web editor |
| DMR ID database, formats N and C, built from a RadioID `user.csv` (opened, downloaded or fetched by region) | done, matches the web editor; a compact database goes to a radio that reads only N as N |
| Packaging: Windows `.zip`, Linux `.tar.gz` and `.deb`, built by GitHub Actions on a `v*` tag | done |

Checked on a real DM-32UV: reading and writing, the table edits, the settings area, signaling tones, P25 and NXDN
channels, P25 trunked and Tier III systems (listen only), DTMF and APRS, and the ID database in both formats.
Following a live P25 system was tried and found no signal (inconclusive). Not tried: the ARS server, the IP gateway, and a
real Tier III key (these transmit or need a live network).

Accessibility: the program publishes its controls to screen readers (AccessKit: AT-SPI on Linux, UI Automation on Windows),
every table cell has a name ("RX MHz, channel 5"), and the tests check it. Verified through AT-SPI on Linux only; not yet
heard in Orca or Narrator, and the window frame has no name.

Not edited, as in the web editor: targets, target lists and traffic keys (kept byte for byte), DTMF signaling.
Not planned: flashing firmware (it can brick a radio), Android and iOS, macOS.

## Layout

| Crate | Holds | Tests |
|---|---|---|
| `rfcps-core` | the codeplug as data: image encode and decode, import and export formats. No I/O | unit tests, golden and differential tests against the web editor |
| `rfcps-link` | the serial link: frames, the fast codeplug protocol, writing | against a software radio |
| `rfcps` | the egui window | headless GUI tests |

## Testing

See `docs/testing.md`. Host tests need no radio. `tools/webcompare/run-all.sh` checks parity with the web editor from a
RadioFoundry checkout, with no radio image. Back up the radio's codeplug before any write.

## Risks

- A write replaces the radio's codeplug: always keep a backup and verify after writing.
- Windows: the FTDI and CH340 drivers and COM port naming; an unsigned `.exe` makes SmartScreen warn once.
- The spec is the source of truth. Where it is wrong or silent, check on a real radio.
