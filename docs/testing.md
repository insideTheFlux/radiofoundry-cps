# Testing

`cargo test --workspace` runs the unit tests on synthetic images. No radio and no personal data are needed.

## GUI tests

`crates/app/src/gui_tests.rs` runs the real app headless in `egui_kittest` (through AccessKit): no window, no display,
no radio. The app is built with no serial ports, and no test opens a file dialog or presses the final write button.
They click buttons, tabs and menus by their labels and check the app's state: the welcome screen, a new codeplug, every
tab, the Import and Export menus, the confirm and notice dialogs, the theme buttons, the Problems button and the
write-failed banner.

The harness also fails a test when a widget has no accessible name, which is how the port chooser and the General tab's
fields were found. The Channels table still has unnamed fields (about a hundred on a full radio), so the real-image test
switches that check off.

```
cargo test -p rfcps gui                              # the synthetic tests
RFCPS_IMAGE=... cargo test -p rfcps gui              # also walks every tab of a real image (the file is only read)
```

The General tab's settings-area fields (ARS server, IP gateway, tones, signaling tones) are covered too: typing an ID,
picking a tone, switching a signaling tone on, and the warning for an area of a newer version. The link tests
(`crates/link/tests/write.rs`) run the area through the software radio: a key edit writes only that block, a key the
radio's own menu changed since the read is kept, a file without an area never erases the radio's keys, and an area of a
newer version is never overwritten.

Channel modes (P25 and NXDN conventional) are covered the same way: the GUI tests switch a channel's mode, edit its NAC,
RAN, rate and squelch, flip RX Only (the tag's transmit flag), go back to FM, set the unit IDs and show a channel of a
mode this editor doesn't edit; the link tests write a tagged channel (its record, then its tag), follow a rename, drop the
tag with its channel and keep the radio's tags when a file without an area is written; the golden test compares whole
images with the web editor's.

The Systems tab is covered by GUI tests (add a system, edit its identity and transmit side, add a site, a control
channel and a plan row, delete it with its channels going plain and receive only; a channel put on a system takes its
control channel's frequencies), problems tests (a full or newer settings area, a site twice, no control channel, a
secret needed but missing) and a link test that writes a system and a channel on it to the software radio. DMR Tier III systems are covered the same way (a new one transmits, as in the web editor; every field, the channel
table, and the key: set from 32 hex digits, shown only when asked, removed; a Tier III channel on a system), with problems
tests for the network model's limits and a link test where deleting a system also takes its secret and its own records.
DTMF contacts and APRS configurations have their own tabs and an APRS choice on a channel; the GUI tests add, edit
and delete them, and a channel picks a configuration, which a delete clears. CSV (`dtmf-contacts.csv`, `aprs.csv`, and
the channel table's APRS column naming the configuration) and qdmr (`positioning` APRS systems, `gnss` fixed position,
DTMF contacts after the digital ones) both match the web editor, and a link test writes both tables to the software radio.
The ID database tab is covered by GUI tests (build a database from a user file, replace it after a question, remove it, undo, add the users as contacts once, convert a format N database, download the user file from a server on this computer, fetch regions from an API server on this computer, and a choice kept in a file for the next run, with no API key in it) and by tests of the document's history; the link tests write the database to the software radio with the header block first erased and last written, and the download is tested against a small local server (the API fetch is tested the same way: pages followed and users merged by ID, the key only in its header, a selection too big refused after one request, a failing server tried twice, a stop; ignored tests fetch the real file and a small country: `cargo test -p rfcps radioid_net -- --ignored`). On a real radio, `crates/link/examples/iddb_probe.rs` asks about the database (read only) and `iddb_test.rs` is a write test that undoes itself: it saves the codeplug and every database block it will touch, writes a database in format C then N from a real `user.csv`, checks each by read-back and the radio's own lookup, and restores the saved blocks. Run it only with a backup you have looked at.
Save image
asks before it carries a secret key (a GUI test; the file dialog itself is not driven).

Not covered: native file dialogs, the serial write itself (the software-radio tests in `rfcps-link` and the hand test
on a radio cover it), and pixel snapshots (`egui_kittest`'s `snapshot` feature, not enabled).

## Against the web editor on a real image (local only)

The decoder and encoder are ported from RadioFoundry's web editor. Three more tests compare them with it on a real
radio image. They run only when these variables are set, and otherwise pass without checking anything:

| Variable | What |
|---|---|
| `RFCPS_IMAGE` | a codeplug image read from a radio (`tools/cps_fast.py read`, `--regions radio`) |
| `RFCPS_GOLDEN` | the web editor's decode of it, as JSON (`golden.mjs`, a small Node script over `model.js decode()`) |
| `RFCPS_EDITED_JS` | the web editor's encode of a fixed set of edits to the same image (`edit_js.mjs`) |
| `RFCPS_KV_DIR` | the web editor's settings-area sectors for fixed key edits, with the radio-merge case (`edit_kv.mjs`, a small Node script over `kv.js` `putKeys` and `areaForWrite`) |
| `RFCPS_MODES_DIR` | the web editor's whole images for fixed channel-mode edits (`edit_modes.mjs`: `setKind` to P25 and NXDN with parameters and unit IDs, a rename, a delete, back to plain; `model.js` `encode()`) |
| `RFCPS_SYS_DIR` | the web editor's whole images for fixed system edits (`edit_systems.mjs`: `newSystem` for a P25 trunked system with a site, a control channel and a plan row, a channel put on it, a rename and a second control channel, and the system deleted; `model.js` `encode()` and `remove()`) |
| `RFCPS_T3_DIR` | the web editor's whole images for fixed Tier III edits (`edit_t3.mjs`: a system with every field set, two channels and a key with a PSN, a channel on it, the system's own `0x0800` and `0x0900` records put in, edits, and the system deleted) |
| `RFCPS_APRS_DIR` | the web editor's whole images for DTMF contact and APRS configuration edits (`edit_aprs.mjs`: created, edited, deleted, and an APRS record in the older layout), its CSV export of an image that holds them (`csv/`), and its qdmr export and import (`qdmr/`: `qdmr_aprs_js.mjs`, with a `rich.yaml` of awkward APRS systems) |
| `RFCPS_IDDB_DIR` | the web editor's DMR ID database in format N for fixed and random record sets (`iddb_gold.mjs`: records, text length and room in, the bytes written, header, count, read-back and extent out; `iddb.js`) |
| `RFCPS_IDDBC_DIR` | the web editor's DMR ID database in format C and the conversions (`iddbc_gold.mjs`: steps on one image, namely writes in either format fresh or against what is there, converts and an erase, with room limits and page splits; whole image, each step's answer, read-back, extent and size estimate out) |
| `RFCPS_RID_DIR` | the web editor's RadioID source on random `user.csv` text (`rid_gold.mjs`: `parseUsers` with header variants and bad rows, `select`, `regionsOf`, `format` over many templates, `addContacts`; `sources/radioid.js`) |
| `RFCPS_RIDAPI_DIR` | the web editor's `fromApi` on random API replies, with ids and texts of every JSON type (`ridapi_gold.mjs`; `sources/radioid.js`) |
| `RFCPS_IDDB_DIR` | the web editor's DMR ID database in format N for fixed and random record sets (`iddb_gold.mjs`: records, text length and room in, the bytes written, header, count, read-back and extent out; `iddb.js`) |
| `RFCPS_RID_DIR` | the web editor's RadioID source on random `user.csv` text (`rid_gold.mjs`: `parseUsers` with header variants and bad rows, `select`, `regionsOf`, `format` over many templates; `sources/radioid.js`) |

```
RFCPS_IMAGE=... RFCPS_GOLDEN=... RFCPS_EDITED_JS=... cargo test -p rfcps-core --test golden -- --nocapture
```

What they check: the same records and fields; the image decoded and encoded again, unedited, is byte for byte the same;
and the edits (rename, tone, squelch, power, a new channel, a new contact, a renamed zone, radio name, boot text) give
the same image as the web editor's encoder.

A real image holds a callsign, a DMR ID and channel names. Keep these files out of the repository and never write an
edited test image to a radio.
