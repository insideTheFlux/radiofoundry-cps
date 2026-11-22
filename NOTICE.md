# Notice

This project follows RadioFoundry's `FORKING.md`: code or file layouts ported from RadioFoundry keep its MIT license,
the `AUTHORS` and `CONTRIBUTORS` files (copied from RadioFoundry) and the "The RadioFoundry Authors" copyright line, with
this project's copyright added below.

Formats and protocols are implemented from RadioFoundry's `docs/spec/` (open-firmware-cps.md, cps-fast.md,
codeplug-native.md, dm32-stock-codeplug.md, iddb-compact.md and others) and, where a behavior is the web editor's, from the
web editor's code. Nothing here is taken from any other radio's firmware or CPS source (qdmr's YAML format is implemented from
RadioFoundry's `docs/spec/qdmr-yaml.md` and the web editor, never from qdmr's source).

The third-party libraries a built program contains, with their licenses, are listed in `THIRD_PARTY_LICENSES.txt`, which
every package includes.

One library is under the Mozilla Public License 2.0: `serialport` (the serial port access), used unmodified. Its source is at
https://github.com/serialport/serialport-rs, and its license text is in `THIRD_PARTY_LICENSES.txt`. The libraries offered
under a choice of licenses (for example "MIT OR GPL-3.0-only") are used under their permissive choice.
