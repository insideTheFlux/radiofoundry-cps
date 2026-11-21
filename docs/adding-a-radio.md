# Adding a radio

This editor reads and writes one radio today, the DM-32UV. The layout of its codeplug is in the code as constants and
record offsets, and the link refuses a radio whose layout differs. This page lists what a second radio needs. It
describes the code as it is; nothing here is built for other radios yet.

## What is the same for every radio

- The window, the tables, undo, the Problems list and the import and export formats work on the model (`Codeplug`),
  not on bytes.
- The link speaks RadioFoundry's fast codeplug protocol (`docs/spec/cps-fast.md` in the RadioFoundry repository): the
  same commands on every radio that has it. A radio without it is out of reach until it gains it.
- The write is the same safe sequence: backup, changed blocks only, read-back.

## What is specific to the DM-32UV

| Where | What it fixes |
|---|---|
| `crates/core/src/image.rs` | the image size, where the EEPROM and flash images start, the regions a read takes (`REGIONS`), the settings area |
| `crates/core/src/records.rs` | every record's address and size: channels, zones, contacts, lists, DTMF, APRS, the general block, the VFOs |
| `crates/core/src/iddb.rs` | where the DMR ID database areas are |
| `crates/core/src/stock_dm32.rs` | the import of the stock DM-32UV backup |
| `crates/link/src/fast.rs` | `check_layout`: the INFO reply must match the layout above, or nothing is read or written |
| `crates/link/src/write.rs` | the VFO A record, which the radio rewrites by itself |
| `crates/link/src/radio.rs` | the USB IDs offered as likely radios (CH340, CH9102) |
| `crates/core/src/freqs.rs`, `problems.rs` | band limits and the checks that depend on what the radio can do |

## Steps

1. Read the radio's codeplug layout in RadioFoundry's `docs/spec/` and the web editor's `records.js` for that radio.
2. Make the layout data (addresses, sizes, limits) a value the model and link take, instead of constants.
   This is the main piece of work.
3. Read an image from the radio with `rfcps --read PORT out.bin`, decode it, and compare with the web editor's decode
   (`RFCPS_IMAGE` and `RFCPS_GOLDEN` in `docs/testing.md`).
4. Compare edits and whole images the same way, table by table, before any write.
5. Write only with a backup, and only after the software-radio tests pass for the new layout.

## Tested radios

| Radio | Read | Write | Notes |
|---|---|---|---|
| DM-32UV | yes | yes | Windows 11 and Linux, over its USB cable (CH340) |

Report a result for another radio in an issue, with what you tried and the backup you made first.
