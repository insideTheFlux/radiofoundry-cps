# RadioFoundry CPS (Rust)

A native codeplug editor for [RadioFoundry](https://github.com/RadioFoundry/RadioFoundry) radios, written in Rust: a
Windows 11 `.exe` and a Linux program, no browser. First radio: the DM-32UV.

Private project for now. Anything ported from RadioFoundry keeps its MIT notice (see `LICENSE`, `NOTICE.md`).

## What it does

- Reads a radio's codeplug over its programming cable or USB (the fast codeplug protocol), in under a second.
- Edits channels, zones, contacts, RX group lists and the general settings, with undo, a Problems list (the web editor's
  checks) and the web editor's look, light or dark.
- Writes to the radio safely: a list of what will change, a backup of the radio's codeplug first, only the changed
  blocks, a refusal if the radio changed since it was read, and a read-back check.
- Imports a stock DM-32UV backup (the web installer's `.zip`), CHIRP CSV and CSV tables; exports CSV tables and CHIRP CSV.
  CSV and CHIRP files are the web editor's: they work in both editors.

## Run

```
cargo run -p rfcps            # the editor
cargo test --workspace        # the tests (no radio needed)
```

On Linux, to use a serial port add your user to the `dialout` group (`sudo usermod -aG dialout $USER`) and log in again.

## Packages

| Platform | How |
|---|---|
| Linux (Ubuntu, Debian) | `packaging/build-linux.sh` makes `dist/*.tar.gz` and `dist/*.deb` |
| Windows 11 | on a Windows computer with Rust and the Visual Studio C++ Build Tools: `powershell -ExecutionPolicy Bypass -File packaging\build-windows.ps1` makes `dist\*.zip` with `rfcps.exe` |
| Both | `.github/workflows/release.yml` builds and tests both on GitHub when it is run, or when a `v*` tag is pushed |

Windows: the radio's cable needs its serial driver (FTDI or CH340); Windows 11 usually installs it by itself. An unsigned
program makes SmartScreen ask "Run anyway?" once.

## Layout

| Crate | Holds |
|---|---|
| `rfcps-core` | the codeplug as data: the image format, validation, diff, CSV, CHIRP and the stock DM-32UV import. No I/O |
| `rfcps-link` | the serial link: the debug link's frames, the fast codeplug read and write, and a software radio for tests |
| `rfcps` | the window |

See `PLAN.md` for scope and phases and `docs/testing.md` for how the real-radio checks work.
