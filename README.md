# RadioFoundry CPS (Rust)

A native codeplug editor for [RadioFoundry](https://github.com/RadioFoundry/RadioFoundry) radios, written in Rust: a
Windows 11 `.exe` and a Linux program, no browser. First radio: the DM-32UV.

An independent editor, not an official RadioFoundry release. MIT licensed. Anything ported from RadioFoundry keeps its
notice (see `LICENSE`, `AUTHORS`, `CONTRIBUTORS` and `NOTICE.md`).

**Tested on one radio:** the DM-32UV, on Windows 11 and Linux. Other RadioFoundry radios may work, since RadioFoundry's spec says the protocol and the
image layout are shared, but none has been tried; see [Adding a radio](docs/adding-a-radio.md).

## What it does

- Reads a radio's codeplug over its programming cable or USB (the fast codeplug protocol), in under a second.
- Edits channels, zones, contacts, RX group lists and the general settings (including the settings area: ARS server, IP gateway, tones and signaling tones), P25 and NXDN conventional channels (mode, NAC or RAN, squelch, rate) and the P25 and NXDN unit IDs, P25 trunked systems (identity, sites, control channels, channel plan; listen only until you turn the transmit side on) and DMR Tier III systems (network, options, fixed plan, channels, key) with channels on them, DTMF contacts and APRS configurations, with undo, a Problems list (the web editor's
  checks) and the web editor's look, light or dark.
- Builds the DMR ID database from a RadioID `user.csv` (downloaded here on a button, fetched for a few regions through RadioID's API, or opened from a file; the choices are kept in `~/RadioFoundry-CPS/id-database-settings.json`, never the API key; regions, call
  sign prefixes, name templates), or adds those users as contacts; the database is sent in its own step after the
  codeplug. The download is the only network use: one request to radioid.net, and only when you press the button.
- Writes to the radio safely: a list of what will change, a backup of the radio's codeplug first, only the changed
  blocks, a refusal if the radio changed since it was read, and a read-back check.
- Imports a stock DM-32UV backup (the web installer's `.zip`), qdmr YAML codeplugs, CHIRP CSV and CSV tables; exports qdmr YAML, CSV tables and CHIRP CSV.
  CSV, CHIRP and qdmr files are the web editor's: they work in both editors.

## Run

```
cargo run -p rfcps            # the editor
cargo test --workspace        # the tests (no radio needed)
```

Without a window: `rfcps --ports` lists serial ports, `rfcps --read PORT out.bin` saves the radio's codeplug to a file (read only), `rfcps --help` shows the rest.

On Linux, to use a serial port add your user to the `dialout` group (`sudo usermod -aG dialout $USER`) and log in again.

On a Wayland desktop (the Ubuntu default) the window runs on X11 through XWayland, so the desktop draws its normal title
bar and buttons (a native Wayland window gets a plain frame there). `RFCPS_WAYLAND=1 rfcps` keeps native Wayland.

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
| `rfcps-core` | the codeplug as data: the image format, validation, diff, CSV, CHIRP, qdmr YAML and the stock DM-32UV import. No I/O |
| `rfcps-link` | the serial link: the debug link's frames, the fast codeplug read and write, and a software radio for tests |
| `rfcps` | the window |

See `PLAN.md` for scope and status, `docs/testing.md` for how the tests work (and `tools/webcompare/` to check the parity
with the web editor yourself), and `docs/adding-a-radio.md` for trying another radio.
