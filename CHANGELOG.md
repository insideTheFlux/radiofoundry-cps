# Changes

## 0.5.4

### Fixed

- **AES keys** (Systems tab): 0.5.3 said the key box shows all 64 hex digits, but at the default window size it cut off
  the last four. Its width now comes from the font, so all 64 show, and the full key was always the one stored and
  written; only the display was short.

## 0.5.3

Fixes found by running the 0.5.2 package on a hidden display and looking at every system and tab.

### Fixed

- **AES keys** (Systems tab): the key box showed about 7 of the 64 hex digits. It was widened, but still cut off the last four at the default
  window size; fixed in 0.5.4.
- **Tone lists** (Channels tab): the "…" button beside a tone field fell into the next column of the channel form, so the
  RX and TX rows did not line up, and in the channel table it was cut off to a sliver. It sits beside its field in the
  form; the table has no button (pick a tone from the list in the form, or type it).
- **P25 trunked systems**: the Sites, Control channels and Channel plan tables had their headings one column off from
  their boxes, and the frequency boxes were cut to the width of their headings (a frequency read "851."). Headings and
  boxes line up, and the boxes show their whole value.
- **Tier III systems**: the Channels table had the same off-by-one headings.
- The note under a system's "Transmit side" said only a new Tier III system can transmit; it now says Tier III, NXDN
  or AES (the web editor starts all three able to transmit).

### Not checked

- Windows. Window sizes below the default (a hidden display without a window manager does not lay a window out again
  when it is resized, so this could not be looked at).

## 0.5.2

Brings the editor level with the web Codeplug Editor as of RadioFoundry `dd67ecd6` (2026-11-24).

### New

- **DMR AES-256 (RadioFoundry #758, #1046).** Channels can use the new AES mode, and the Systems tab has a new DMR
  AES-256 system type.
  - An AES system has a Network ID and a table of AES-256 keys, each with a PI key ID (0 to 255) and 64 hex digits. The
    keys are hidden until you tick **Show AES keys**. A new key starts random, from the operating system's random source,
    and **Generate** replaces one.
  - An AES channel keeps its DMR fields and picks a system and a PI key ID from that system's keys.
  - The Problems list warns about a missing key or system, and about encrypted or scrambled voice on an amateur
    frequency (this now covers an NXDN scrambler key too). Nothing is blocked.
  - A saved image asks before it carries the keys, as it does for NXDN scrambler keys.
  - A qdmr file carries an AES system's network and a channel's PI key ID, never a key.
- **ID Database: remove and cut to fit (#1039, #1048).** A database header the editor can't read is shown and can be
  removed. A database that reaches past the end of the radio's image is flagged before a write, where it used to stop
  with an error, and **Cut to fit** keeps the records that fit, in one undo step.
- **RX Only channels (#946).** On an RX Only channel, or one with its RX in the air band or broadcast FM, the TX
  frequency, TX timeout, TX tone, power, VOX and tone burst are grayed, and the table's cells are blank. The values come
  back when RX Only is cleared. A TX frequency left in a receive-only band by an import stays open to be corrected.
  - The tone fields have a searchable list of the 50 CTCSS tones and 104 DCS codes; a tone can still be typed.
  - The TX timeout is a list: the radio's default, then 15 s steps to 3825 s.
  - A TX frequency typed in a receive-only band says why it was refused.
- **RT Systems cable (#371).** The USB-K4Y cable (vendor id 0x2100, an FTDI chip) is ranked as a likely cable in the
  port list.

### Changed

- The Mode drop-down is taller, so every mode shows.

### Checked

- 297 tests pass. The random-edit comparison with the web editor found 0 differences in 3 seeds of 100 cases and 6 of
  200, about a quarter of them with AES edits.
- Not tried on a real radio, or with the real USB cable.

## 0.5.1

- The recommended ham system dialog has a usable width.
- README: a note on brltty taking the radio's USB cable on Ubuntu.

## 0.5.0

Catches up with the web Codeplug Editor.

- Part 1: names, the CRC flag, listen bands, Tier III options.
- Part 2: NXDN systems, ham systems, the qdmr block.
- File choosers open on their own thread, so the window stays responsive.
- Docs: what was heard with Narrator on Windows and Orca on Linux.

## 0.4.2

- Screen readers: the window has a name, every control is named, and the interface is published to assistive technology.
- Linux: runs on X11 under a Wayland desktop again.
- The Linux tarball includes `install.sh`.

## 0.4.1

- ID Database tab: counts with thousands separators, a clearer source line, wider ID boxes.
- Added `tools/webcompare`: the web editor's side of every golden test, plus the differential and LZ4 tests.
- Notices, AUTHORS, README, plan, and an "adding a radio" page for the public repository.

## 0.4.0

- New **ID Database** tab: build the radio's DMR ID database from a RadioID `user.csv`, download the file from RadioID.net, or fetch users from RadioID's API. Choices are kept.
- **Add from RadioID** on the Contacts tab.
- The database is written in its own session. The radio's own ID lookup is used to check it.
- The editor refuses a radio whose INFO layout is not the one it knows.
- A self-undoing write test for the table edits (developer tool).

## 0.3.0

- New **DTMF** and **APRS** tabs; a channel can pick an APRS configuration. DTMF contacts and APRS configurations are in CSV and qdmr too.
- Systems tab: DMR Tier III systems and channels. The key shows only when asked.
- Note: the radio's firmware reads APRS configuration 1 even when no channel uses it.

## 0.2.0

- New **Systems** tab with P25 trunked systems, and P25T channels. The checks around them are included.
- **Save image** can leave secret keys out.

## 0.1.3

- Channels tab: P25 and NXDN conventional channel modes.
- General tab: the settings area's fields and the unit IDs, with GUI tests.

## 0.1.2

- Command-line modes: `--ports`, `--read`, `--version` and `--help` (headless).
- Windows: command-line modes attach to the parent console.
- Automated GUI tests; the port chooser and General tab have accessible names.

## 0.1.1

- Writing an empty codeplug needs an explicit tick, because it erases the radio's channels, zones and contacts.
- The toolbar is two fixed rows, so resizing the window no longer moves Undo.

## 0.1.0

First release of the RadioFoundry Codeplug Editor, a native (Rust, egui) editor for RadioFoundry radios. Linux (.tar.gz and .deb) and Windows builds.

- Read a codeplug from the radio over the debug link, open and save image files, undo.
- Edit channels, zones, contacts, RX group lists and general settings. Frequency rules follow the web editor.
- Import and export: CHIRP CSV, the web editor's CSV tables, the stock DM-32UV backup, and qdmr YAML.
- Problems list (the web editor's checks), with a one-click RX-only fix for weather channels.
- Write to radio: shows the change list, makes a backup first, writes only the changed blocks, then reads back to verify. Nothing is written when the radio already holds the codeplug.
- Edits are checked against the web editor byte for byte on a real image. A write was tested on a real DM-32UV (Windows write test passed).
