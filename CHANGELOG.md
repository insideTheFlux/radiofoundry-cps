# Changes

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
