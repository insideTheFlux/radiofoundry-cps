# Testing

`cargo test --workspace` runs the unit tests on synthetic images. No radio and no personal data are needed.

## GUI tests

`crates/app/src/gui_tests.rs` runs the real app headless in `egui_kittest` (through AccessKit): no window, no display,
no radio. The app is built with no serial ports, and no test opens a file dialog or presses the final write button.
They click buttons, tabs and menus by their labels and check the app's state: the welcome screen, a new codeplug, every
tab, the Import and Export menus, the confirm and notice dialogs, the theme buttons, the Problems button and the
write-failed banner.

The harness also fails a test when a widget has no accessible name, which is how the port chooser and the General tab's
fields were found. Every tab now passes it, including the Channels, Contacts, DTMF and Systems tables (each cell is named
"column, row", for example "RX MHz, channel 5"), and the walk-through of a full image runs with the check on.

The program publishes its controls to screen readers through AccessKit (AT-SPI on Linux, UI Automation on Windows).
`tools/a11y_check.py` reads that from a running copy on Linux and reports how many controls have no name (needs
accessibility switched on for the session; see the script). It was run against a 39-channel image: 0 unnamed controls,
after 112 before the cells were named; the window itself is named "RadioFoundry CPS".

Listened to by hand, with Narrator on Windows 11 (2026-11-22, v0.4.2, using the keyboard): the window is announced; the
Channels table reads each cell's name and value ("Name, channel 1", "RX MHz, channel 1"), says when text is deleted and
echoes typing; drop-downs are announced when reached with Tab; check boxes toggle and the new state is spoken; the menus,
the dialogs, the tabs (Channels, Contacts, Zones and the others) and adding and deleting a channel all work from the
keyboard. The keyboard is the supported way: a drop-down clicked with the mouse is not announced, because a click does not
move the keyboard focus, and Narrator's own scan-mode navigation did not read drop-downs either (not investigated).
On Windows, one early test had the keyboard focus leave a check box after it toggled (Narrator announced "RadioFoundry CPS"
and Space did nothing until the user Tabbed back). It did not happen again: later, repeated Space presses kept the focus
in the editor and in a plain egui test window (three kinds of check box, the window named or not), and the cause was not
found.

Orca 50.3 on Ubuntu GNOME (2026-11-22), by hand: drop-downs, menus and dialogs work, and check boxes work better than on
Windows: pressing Space again and again to check and uncheck keeps the focus on the check box.

```
cargo test -p rfcps gui                              # the synthetic tests
RFCPS_IMAGE=... cargo test -p rfcps gui              # also walks every tab of a real image (the file is only read)
```

The General tab's settings-area fields (ARS server, IP gateway, tones, signaling tones) are covered too: typing an ID,
picking a tone, switching a signaling tone on, and the warning for an area of a newer version. The link tests
(`crates/link/tests/write.rs`) run the area through the software radio: a key edit writes only that block, a key the
radio's own menu changed since the read is kept, a file without an area never erases the radio's keys, and an area of a
newer version is never overwritten.

DMR AES-256 channels and systems have their own tests (`crates/core/tests/aes.rs`): the bytes of a system, a key and a channel's tag equal what the web editor's `mode_aes.js` wrote for the same edits (the expected hex was made with `encode()` of a model with one AES system, one key and one channel), a decoded image re-encodes to the same bytes, keys come and go with their system and share the traffic-key family with NXDN's, a key record of another algorithm is kept, the keys count as secrets and are left out of an image saved without them, and the problems (no key, a key its system lacks, no system, another type of system, encryption on an amateur frequency) are listed; the GUI tests add an AES system and its keys (shown only when asked, Generate, unique PI key IDs) and make a channel AES with a system and a key. The differential test generates AES systems, keys and channels too (`aesSys`, `aesKeyEdit`, `aesPick`), and compares their qdmr export and import (the web editor writes an AES system's network and a channel's PI key ID, never a key).

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
The ID database tab is covered by GUI tests (build a database from a user file, replace it after a question, remove it, undo, add the users as contacts once, convert a format N database, a database past the radio's image end flagged and cut to fit in one undo step, a header the editor can't parse shown and removed, download the user file from a server on this computer, fetch regions from an API server on this computer, a choice kept in a file for the next run with no API key in it, the Contacts tab's shortcut, and a list over 20,000 users counted off the screen thread with the buttons waiting for it) and by tests of the document's history; the link tests write the database to the software radio with the header block first erased and last written, and the download is tested against a small local server (the API fetch is tested the same way: pages followed and users merged by ID, the key only in its header, a selection too big refused after one request, a failing server tried twice, a stop; ignored tests fetch the real file and a small country: `cargo test -p rfcps radioid_net -- --ignored`). On a real radio, `crates/link/examples/iddb_probe.rs` asks about the database (read only), and `iddb_check.rs` (read only) asks the radio's own lookup for a sample of users with a call sign prefix and compares its answers with the text the editor builds, which proves the firmware reads what was written and `iddb_test.rs` is a write test that undoes itself: it saves the codeplug and every database block it will touch, writes a database in format C then N from a real `user.csv`, checks each by read-back and the radio's own lookup, and restores the saved blocks. Run it only with a backup you have looked at.
Save image
asks before it carries a secret key (a GUI test; the file dialog itself is not driven).

`crates/link/examples/sigtone_test.rs` is a write test of the three signaling tone records (steps `show`, `apply`, `defaults`, `remove`; the tones sound on the speaker only, nothing goes on air). On a DM-32UV (2026-11-22) the radio's Settings > Signaling tones menu showed the written values, and after `defaults` and `remove` the whole codeplug, settings area included, was identical to before.

`crates/link/examples/p25_test.rs` follows a live P25 trunked system, receive only: `apply SITE:MHZ[,MHZ]...` writes a listen-only system with the sites given (a site number and its control channels in MHz) and a channel with its transmit flag off, `zone` puts the channel in a zone so it can be selected on the radio, `watch SECONDS` polls the radio's p25_trunk view (debuglink.md view 37; `Radio::view`), and `restore` writes the saved codeplug back. `rssi_scan.rs` (read only) prints the radio's signal strength per control channel the follower is on. On a DM-32UV (2026-11-22) the system, a channel and a zone were written and read back, the transmit side read 0 throughout, and the follower hunted ten control channels at five VHF sites near the tester for minutes, indoors and outdoors, decoding no block at all. That is inconclusive, not a pass: there was no P25 Phase 1 signal the radio could hear (the other systems nearby are at 700 and 800 MHz, outside its bands). The setup, the follower's reporting and the restore worked. Repeat it with a site the radio can hear.

`crates/link/examples/edit_test.rs` is a write test of the table edits on a real radio, in three steps the owner watches: `apply` saves the codeplug and adds a receive-only channel, a zone, a group and a private contact, an RX group list and a boot text line in one write; `delete` takes them away; `restore` writes the saved codeplug back. `edit_test fake:IMAGE.bin DIR rehearse` runs all three on a software radio first. On a DM-32UV (2026-11-21) the channel, zone, contacts and group list showed on the radio (a group list only shows in a channel's RX group list picker), and after the restore the codeplug was identical to the backup.

Not covered: native file dialogs, the serial write itself (the software-radio tests in `rfcps-link` and the hand test
on a radio cover it), and pixel snapshots (`egui_kittest`'s `snapshot` feature, not enabled).

## Against the web editor (`tools/webcompare/`)

The decoder, encoder and every export are ported from RadioFoundry's web editor. The tests that compare them with it run
only when their variables are set, and otherwise pass without checking anything. `tools/webcompare/run-all.sh` makes the
web editor's side of every one of them from a RadioFoundry checkout and runs them (see `tools/webcompare/README.md`). It
needs no radio and no radio image: the starting image is made up. The table below names each variable; the script that
makes its input is named there and in the README.

### Random edits (differential test)

`tools/webcompare/differential_gen.mjs` applies random edits to an image through the web editor's own model and writes, per
case, the image it encodes, a canonical dump of what it decodes, its CSV, qdmr and CHIRP exports and its problems; for pairs
of cases it also writes the first one's model onto the second one's bytes. `crates/core/tests/differential.rs` makes the CPS
do the same and compares everything. It runs only when `RFCPS_FUZZ_DIR` is set (`run-all.sh` does: 3 seeds of 100 cases, more
with `DIFF_SEEDS` and `DIFF_CASES`). It reports every mismatch with the edits that led to it. The CPS-only weather-channel
warning is left out of the problem comparison. Each case also imports the web editor's qdmr file into a fresh read of the image and compares what that decodes to and exports as, and a re-encode (the web editor's own: a channel in a listen band is written RX only with TX on RX), and `ham.json` checks the recommended ham systems. The edits include NXDN systems, scrambler keys and transmit keys, and AES systems, keys and channels. The tests compare with the web editor that is checked out, so check out its current `main` first (`dd67ecd6`, 2026-11-24, is the last it was run against: 0 mismatches in 3 x 100 cases, and in 6 more seeds of 200 cases, about a quarter of them with AES edits). Use several seeds and the prepared base (`differential_prep.mjs`), since a
plain image has no systems or tags. `crates/link/tests/lz4_spec.rs` checks the streams the editor sends are valid LZ4 blocks
that obey the format's end rules and the 4128-byte limit; it needs no web editor.

To compare on a real image of your own as well, read one from a radio (`rfcps --read PORT out.bin`) and set `RFCPS_IMAGE`
to it, with `RFCPS_GOLDEN` and the others made from the same file by the same scripts.

| Variable | What |
|---|---|
| `RFCPS_IMAGE` | the starting image: made up by `make-base.mjs`, or one read from a radio (`rfcps --read PORT out.bin`) |
| `RFCPS_GOLDEN` | the web editor's decode of it, as JSON (`golden.mjs`, a small Node script over `model.js decode()`) |
| `RFCPS_EDITED_JS` | the web editor's encode of a fixed set of edits to the same image (`edit_js.mjs`) |
| `RFCPS_CSV_DIR` | the web editor's CSV export of the six tables of the image (`csv6_js.mjs`) |
| `RFCPS_CHIRP_CSV` | its CHIRP CSV export of the image (`chirp_js.mjs`) |
| `RFCPS_QDMR_DIR` | its qdmr YAML export of the image, and of an export imported again (`qdmr_js.mjs`) |
| `RFCPS_FUZZ_DIR` | its random edits (`differential_gen.mjs`): per case the image, a re-encode, a canonical dump, the CSV, qdmr and CHIRP exports, the problems, and the qdmr file imported again; plus `ham.json`, the recommended ham systems built by `newHamSystem` |
| `RFCPS_STOCK_DATA`, `RFCPS_STOCK_GOLDEN` | a stock DM-32UV backup of your own and the web editor's import of it (`stock_js.mjs`); not made by `run-all.sh` |
| `RFCPS_KV_DIR` | the web editor's settings-area sectors for fixed key edits, with the radio-merge case (`edit_kv.mjs`, a small Node script over `kv.js` `putKeys` and `areaForWrite`) |
| `RFCPS_MODES_DIR` | the web editor's whole images for fixed channel-mode edits (`edit_modes.mjs`: `setKind` to P25 and NXDN with parameters and unit IDs, a rename, a delete, back to plain; `model.js` `encode()`) |
| `RFCPS_SYS_DIR` | the web editor's whole images for fixed system edits (`edit_systems.mjs`: `newSystem` for a P25 trunked system with a site, a control channel and a plan row, a channel put on it, a rename and a second control channel, and the system deleted; `model.js` `encode()` and `remove()`) |
| `RFCPS_T3_DIR` | the web editor's whole images for fixed Tier III edits (`edit_t3.mjs`: a system with every field set, two channels and a key with a PSN, a channel on it, the system's own `0x0800` and `0x0900` records put in, edits, and the system deleted) |
| `RFCPS_APRS_DIR` | the web editor's whole images for DTMF contact and APRS configuration edits (`edit_aprs.mjs`: created, edited, deleted, and an APRS record in the older layout), its CSV export of an image that holds them (`csv/`), and its qdmr export and import (`qdmr/`: `qdmr_aprs_js.mjs`, with a `rich.yaml` of awkward APRS systems) |
| `RFCPS_IDDB_DIR` | the web editor's DMR ID database in format N for fixed and random record sets (`iddb_gold.mjs`: records, text length and room in, the bytes written, header, count, read-back and extent out; `iddb.js`) |
| `RFCPS_IDDBC_DIR` | the web editor's DMR ID database in format C and the conversions (`iddbc_gold.mjs`: steps on one image, namely writes in either format fresh or against what is there, converts and an erase, with room limits and page splits; whole image, each step's answer, read-back, extent and size estimate out) |
| `RFCPS_RID_DIR` | the web editor's RadioID source on random `user.csv` text (`rid_gold.mjs`: `parseUsers` with header variants and bad rows, `select`, `regionsOf`, `format` over many templates, `addContacts`; `sources/radioid.js`) |
| `RFCPS_RIDAPI_DIR` | the web editor's `fromApi` on random API replies, with ids and texts of every JSON type (`ridapi_gold.mjs`; `sources/radioid.js`) |

`tools/webcompare/run-all.sh` sets all of these and runs the tests. By hand, for example:

```
RFCPS_IMAGE=... RFCPS_GOLDEN=... RFCPS_EDITED_JS=... cargo test -p rfcps-core --test golden -- --nocapture
```

What they check: the same records and fields; the image decoded and encoded again, unedited, is byte for byte the same;
and the edits (rename, tone, squelch, power, a new channel, a new contact, a renamed zone, radio name, boot text) give
the same image as the web editor's encoder.

A real image holds a callsign, a DMR ID and channel names. Keep these files out of the repository and never write an
edited test image to a radio.
