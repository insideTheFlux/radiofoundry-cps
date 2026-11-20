# Plan

## Goal
A native editor for RadioFoundry radios: Windows 11 (x86-64) `.exe` and Ubuntu/Linux binary. One codebase, all Rust,
no browser, no Python. First radio: DM-32UV.

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
| 1 | `rfcps-core`: the codeplug image (channels, contacts, zones, groups, settings) from `docs/spec/` | decode and encode a real radio image, round trip is exact | 3-5 weeks |
| 2 | `rfcps-link`: read and write over the programming cable | reads the radio's codeplug to a file and writes it back, byte for byte | 1-2 weeks |
| 3 | GUI: tables for channels, contacts, zones; edit, undo, validation ("problems" list) | edit a channel and write it | 4-8 weeks |
| 4 | Import and export: CSV, CHIRP, qdmr YAML, the stock DM-32UV codeplug | open a stock backup | 2-4 weeks |
| 5 | Packaging: Windows `.exe` or installer, Linux AppImage and `.deb`, CI | a download that runs | 1-2 weeks |

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
