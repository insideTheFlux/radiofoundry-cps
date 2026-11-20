# Testing

`cargo test --workspace` runs the unit tests on synthetic images. No radio and no personal data are needed.

## Against the web editor on a real image (local only)

The decoder and encoder are ported from RadioFoundry's web editor. Three more tests compare them with it on a real
radio image. They run only when these variables are set, and otherwise pass without checking anything:

| Variable | What |
|---|---|
| `RFCPS_IMAGE` | a codeplug image read from a radio (`tools/cps_fast.py read`, `--regions radio`) |
| `RFCPS_GOLDEN` | the web editor's decode of it, as JSON (`golden.mjs`, a small Node script over `model.js decode()`) |
| `RFCPS_EDITED_JS` | the web editor's encode of a fixed set of edits to the same image (`edit_js.mjs`) |

```
RFCPS_IMAGE=... RFCPS_GOLDEN=... RFCPS_EDITED_JS=... cargo test -p rfcps-core --test golden -- --nocapture
```

What they check: the same records and fields; the image decoded and encoded again, unedited, is byte for byte the same;
and the edits (rename, tone, squelch, power, a new channel, a new contact, a renamed zone, radio name, boot text) give
the same image as the web editor's encoder.

A real image holds a callsign, a DMR ID and channel names. Keep these files out of the repository and never write an
edited test image to a radio.
