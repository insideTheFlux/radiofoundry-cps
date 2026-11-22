# Adding a radio

This editor is tested on one radio, the DM-32UV. Whether it works on another RadioFoundry radio is not known: it has not
been tried. This page says what the code assumes and what to check, so a second radio can be tried safely.

## What the code assumes

The fast codeplug protocol (`docs/spec/cps-fast.md` in the RadioFoundry repository) works in *image offsets*: the
EEPROM image starts at 0, the flash image at 0x20000 ("on every radio so far"), and the radio adds its own base
address, so a host never needs it. The image layout is the open-firmware CPS layout (`open-firmware-cps.md` 7), shared
with qdmr's MK22 and UV380 families. So, on paper, a RadioFoundry radio that speaks the fast protocol with the same INFO
reply looks the same to this editor.

The editor checks that before it reads or writes (`Info::check_layout` in `crates/link/src/fast.rs`). It refuses a radio
unless its INFO reply says: protocol version 1, 4 KB blocks, the flash image at 0x20000, an EEPROM image of 0x10000
bytes, and an image that reaches at least the end of the editor's regions. A radio that answers differently is refused
with a message that says what differs. Nothing is read or written.

## What may differ between radios

| Where | What | Notes |
|---|---|---|
| The INFO reply | image end, staging area, data per write | read from the radio each time; the ID database takes its room from the image end |
| The VERSION features | P25, NXDN, DMR trunking, `IDDB_COMPACT` | the editor reads `IDDB_COMPACT` to choose the database format; it does not yet hide a mode a radio does not run |
| `crates/core/src/image.rs`, `records.rs` | the regions a read takes and every record's address | the open-firmware layout, so expected to be shared; not checked on another radio |
| `crates/core/src/stock_dm32.rs` | the import of the stock DM-32UV backup | each radio family has its own stock format; only this one is imported |
| `crates/link/src/radio.rs` | the USB chips offered as likely radios (CH340, CH9102) | a cable with another chip is still listed under "show all ports" |
| `crates/core/src/freqs.rs`, `problems.rs` | band limits and checks | written for the DM-32UV's bands |

## How to try a radio

1. Use a radio whose codeplug you can restore, and make a backup first. A different firmware version can change what is
   safe to write.
2. Read only: `rfcps --read PORT out.bin`. If the editor refuses the radio, the message says which INFO value differs.
   Send that message in an issue; it is the most useful result.
3. Open the image in the editor and look at it: the counts, names and zones should match what the radio shows.
4. Compare with the web editor's decode of the same image, as `docs/testing.md` and `tools/webcompare/` describe
   (`RFCPS_IMAGE` and `RFCPS_GOLDEN`).
5. Write only after steps 2 to 4 look right, one small change at a time, with the backup beside you. The editor writes only
   the blocks that differ, saves a backup of the radio's codeplug first and reads the radio back; the self-undoing tests in
   `crates/link/examples/` (`edit_test`, `iddb_test`) show how to try a write and restore it.

## Tested radios

| Radio | Read | Write | Notes |
|---|---|---|---|
| DM-32UV | yes | yes | Windows 11 and Linux, over its USB cable (CH340) |

Report a result for another radio in an issue: the model and firmware version, what you tried and what the editor said.
