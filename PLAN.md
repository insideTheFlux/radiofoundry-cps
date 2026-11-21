# Plan

## Goal
A native editor for RadioFoundry radios: Windows 11 (x86-64) `.exe` and Ubuntu/Linux binary. One codebase, all Rust,
no browser, no Python. First radio: DM-32UV.

## Next, in order
1. **Write test from Windows. Done (2026-11-20).** On Windows 11, with the build from GitHub Actions, over COM3 (CH340): a channel rename and the revert, one block each, read back and verified; the radio's screen showed the new name; a final read was byte for byte the same as the one before (1,179,648 bytes, 0 differ).
2. **qdmr YAML, open and save. Done (2026-11-20).** What the web editor does (`site/static/js/codeplug/yaml.js`, spec `docs/spec/qdmr-yaml.md`): open a qdmr codeplug, save one with a dated name, and list what qdmr's format can't hold (SFR settings, power levels qdmr has no name for). Ported only from the web editor's code and the RadioFoundry spec, never from qdmr's source (GPL); checked byte for byte against the web editor on real files, with the golden tests gated by environment variables so no private data is in the repo.

## Not in the first version
Flashing firmware (it can brick a radio), the DMR ID database download, Android and iOS, macOS.

## Layout
| Crate | Holds | Tests |
|---|---|---|
| `rfcps-core` | codeplug model, image encode and decode, import and export formats | host unit tests, round trips |
| `rfcps-link` | serial port, CPS protocol (and the fast block protocol later) | against a software radio |
| `rfcps` | egui GUI | by hand on the radio |

## Phases
| # | Phase | Result | Rough effort |
|---|---|---|---|
| 0 | Scaffold, toolchain, Windows type-check | window opens on Linux (done: compiles for Linux and Windows) | days |
| 1 | `rfcps-core`: the codeplug image (channels, contacts, zones, groups, settings) from `docs/spec/` | decode and encode a real radio image, round trip is exact | 3-5 weeks. **Decode and encode done** for channels, VFOs, zones, contacts, RX lists and general settings: they match the web editor on a real DM-32UV image, an unedited image round-trips byte for byte, and edits encode identically to the web editor's. APRS, DTMF, Tier III and the trunked P25 systems are not modeled yet: they come through unchanged. P25 and NXDN conventional channels (the tag, its parameter block and the kinds' radio-wide unit IDs; channel-modes.md) are modeled, matching the web editor's whole image byte for byte. Written to a real DM-32UV (2026-11-20): channel 31 made P25 (NAC 123, selective squelch) and then NXDN (4800 bit/s, RAN 5), both receive only, the radio's channel screen showing each; made plain FM again and the P25 and NXDN unit IDs set and cleared, each step read back and the radio byte-identical to before at the end. The radio has no screen for the squelch or the radio-wide unit IDs, so those were checked by the read-back only. The settings area (ARS server, IP gateway, the four tones, the three signaling tones) is modeled: it matches the web editor's `kv.js` byte for byte, and a write keeps the radio's own keys. Written to a real DM-32UV (2026-11-20): the call end tone key set Off, back to On and removed, one block each time, read back and verified, the radio's Sound menu following; nothing else on the radio changed. The ARS server, IP gateway and signaling tones are covered by the web-editor match and the software-radio tests, not yet tried on a radio (an ARS server would make the radio register on air) |
| 2 | `rfcps-link`: read and write over the programming cable | reads the radio's codeplug to a file and writes it back, byte for byte | 1-2 weeks. **Read done**: the debug link's frames and the fast codeplug protocol (INFO, SUMS, READ, LZ4, CRC-32), tested against a software radio and a real DM-32UV (identical to the Python tool's read, 0.7 s). **Write built**: BEGIN, BLOCK, DATA, END, COMMIT; only changed blocks; the radio's own bytes outside the regions kept; a refusal to write if the radio changed since it was read; a read-back verify. Tested on a software radio (8 tests) and a dry run on a real DM-32UV (an unedited codeplug plans 0 blocks, a renamed channel 1). **Written to a real DM-32UV (2026-11-20)**: channel 3 renamed and renamed back, one block each time, read back and verified; the radio's screen showed the new name at once, and afterward its codeplug was byte-identical to the backup in every region |
| 3 | GUI: tables for channels, contacts, zones; edit, undo, validation ("problems" list) | edit a channel and write it | 4-8 weeks. **Done** (a first version): port chooser, read from a radio, the web editor's palette in light and dark, a Channels table with a detail form, Zones, Contacts, RX Group Lists and General tabs, typed frequencies, undo and redo, a Problems list, Write to radio with a change list, an automatic backup and a read-back check. Needs polish from use on real screens |
| 4 | Import and export: CSV, CHIRP, qdmr YAML, the stock DM-32UV codeplug | open a stock backup | 2-4 weeks. **Done**: the stock DM-32UV import (DCS read as BCD; checked against the web editor on a real backup), CSV tables (export identical to the web editor's on a real image), CHIRP CSV (export identical too). **qdmr YAML done** (open and save; the export is identical to the web editor's, character for character, on a real image; APRS and DTMF are counted in the notes because the model doesn't hold them yet). CHIRP images (`.img`) are not planned: CHIRP does not support the DM-32UV, and CHIRP CSV import and export already carry channels between the two |
| 5 | Packaging: Windows `.exe` or installer, Linux AppImage and `.deb`, CI | a download that runs | 1-2 weeks. **Linux done**: a release build (8 MB, no libraries beyond libc), a `.tar.gz` and a `.deb`. **Windows**: the build script, the icon embedding and a GitHub Actions workflow are written; the `.exe` itself has to be built on a Windows computer or by the workflow. No AppImage, no signed installer |
Total for a usable DM-32UV editor: about 2-3 months part-time. Full parity with the web editor takes longer.

## Testing
- Host tests for every encode and decode, including the real DM-32UV words we know (CTCSS, DCS as BCD).
- A software radio (a stub speaking the protocol over a pty) so `rfcps-link` is tested without hardware. RadioFoundry's
  `tests/host/cpsSim` does this for the Python client.
- Hardware: Linux here, Windows 11 on the owner's laptop. Back up the codeplug before any write.

## Risks
- Windows: the FTDI and CH340 drivers, and COM port naming. Windows 11 usually installs them itself.
- Windows code signing: an unsigned `.exe` makes SmartScreen warn. A certificate costs money; fine for private use.
- A write replaces the radio's codeplug. Always read and keep a backup first, and verify after writing.
- The spec is the source of truth; where it is wrong or silent, check against a real radio (as with the DCS words).
