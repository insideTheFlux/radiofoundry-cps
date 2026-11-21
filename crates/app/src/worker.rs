// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux

//! Serial work on a thread of its own, so the window stays alive while a radio is read.

use eframe::egui;
use rfcps_core::Image;
use std::sync::mpsc::{channel, Receiver};

/// What a read learned about the radio besides its codeplug, for the ID database: whether its firmware reads a format C
/// database (`None`: it did not say) and where its image ends.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RadioFacts {
    pub compact: Option<bool>,
    pub end: Option<usize>,
}

pub enum Msg {
    Facts(RadioFacts),
    Progress(usize, usize),
    Done(Result<Image, String>),
}

/// Reads a radio's codeplug on a new thread. The receiver gets progress and then the result.
pub fn read_radio(port: String, ctx: egui::Context) -> Receiver<Msg> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let result = (|| {
            let mut radio = rfcps_link::open(&port).map_err(|e| e.to_string())?;
            radio.ping().map_err(|e| {
                format!("{e} (is it a RadioFoundry radio, switched on, on this port?)")
            })?;
            let img = radio
                .read_image(&mut |done, total| {
                    let _ = tx.send(Msg::Progress(done, total));
                    ctx.request_repaint();
                })
                .map_err(|e| e.to_string())?;
            let facts = RadioFacts {
                compact: radio.reads_compact_iddb().ok(),
                end: radio.checked_info().ok().map(|i| i.image_end as usize),
            };
            let _ = tx.send(Msg::Facts(facts));
            Ok(img)
        })();
        let _ = tx.send(Msg::Done(result));
        ctx.request_repaint();
    });
    rx
}

pub enum WriteMsg {
    Phase(String),
    Progress(usize, usize),
    Done(Result<WriteDone, String>),
}

pub struct WriteDone {
    pub blocks: usize,
    /// Blocks of the ID database sent (the header's counts twice); `None` when it was not part of the write.
    pub iddb_blocks: Option<usize>,
    /// Something to tell about how the database was written.
    pub iddb_note: Option<String>,
    pub backup: std::path::PathBuf,
}

/// Where a radio's codeplug is saved before every write: `RadioFoundry-CPS/backups` in the home folder.
pub fn backup_dir() -> std::path::PathBuf {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(std::path::PathBuf::from);
    home.unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("RadioFoundry-CPS")
        .join("backups")
}

/// UTC date and time as `YYYYMMDD-HHMMSS`, for file names.
pub fn stamp() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let (days, rem) = ((secs / 86_400) as i64, secs % 86_400);
    // days since 1970-01-01 to a civil date (H. Hinnant's algorithm)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let (y, doy) = (yoe + era * 400, doe - (365 * yoe + yoe / 4 - yoe / 100));
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{y:04}{m:02}{d:02}-{:02}{:02}{:02}",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// The ID database goes in a session of its own after the codeplug (iddb-compact.md 4.3). It is not part of the backup:
/// it can be megabytes, and the radio keeps its own copy only until it is replaced.
fn send_iddb<P: std::io::Read + std::io::Write>(
    radio: &mut rfcps_link::Radio<P>,
    image: &Image,
    say: &dyn Fn(WriteMsg),
) -> Result<(usize, Option<String>), String> {
    use rfcps_core::iddb;
    let mut note = None;
    let mut image = std::borrow::Cow::Borrowed(image);
    // A compact database goes to a radio that reads only format N as format N (iddb-compact.md 6)
    let compact = iddb::read_header(image.bytes()).is_some_and(|h| h.format == b'C');
    if compact && radio.reads_compact_iddb().is_ok_and(|c| !c) {
        let end = radio.checked_info().map_or(usize::MAX, |i| i.image_end as usize);
        let mut n = image.clone().into_owned();
        if let Some(w) = iddb::convert(&mut n, b'N', end) {
            note = Some(if w.dropped > 0 {
                format!(
                    "This radio's firmware reads only the older ID database format, so it was written in that format; {} name(s) did not fit and were left out. A firmware update makes room.",
                    w.dropped
                )
            } else {
                "This radio's firmware reads only the older ID database format, so it was written in that format.".to_string()
            });
            image = std::borrow::Cow::Owned(n);
        }
    }
    say(WriteMsg::Phase("Writing the ID database…".into()));
    radio
        .write_iddb(&image, &mut |d, t| say(WriteMsg::Progress(d, t)))
        .map(|r| (r.blocks, note))
        .map_err(|e| format!("{e}. The codeplug itself was written before the ID database."))
}

/// Writes a codeplug to a radio on a new thread: the radio's current codeplug is read and saved first (and the write
/// does not start if that fails), then the changed blocks are written and the radio is read back.
pub fn write_radio(
    port: String,
    base: Option<Image>,
    image: Image,
    iddb: bool,
    ctx: egui::Context,
) -> Receiver<WriteMsg> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let say = |m: WriteMsg| {
            let _ = tx.send(m);
            ctx.request_repaint();
        };
        let result = (|| {
            let mut radio = rfcps_link::open(&port).map_err(|e| e.to_string())?;
            radio.ping().map_err(|e| {
                format!("{e} (is it a RadioFoundry radio, switched on, on this port?)")
            })?;
            // What the radio holds now, which is also the backup. The image to send keeps the radio's settings area
            // unless the edit changed a key (codeplug-settings.md 6), so a key changed from the radio's own menu does
            // not count as a difference.
            let current = radio
                .read_image(&mut |_, _| {})
                .map_err(|e| e.to_string())?;
            let basis = base.as_ref().unwrap_or(&current);
            let sent = radio.image_for_write(basis, &image).map_err(|e| e.to_string())?;
            // Nothing differs: no backup and no write. (The blocks' checksums are compared, which is quick.)
            let differs = radio.verify(&sent).map_err(|e| e.to_string())?;
            // The ID database goes in a session of its own after the codeplug (iddb-compact.md 4.3). It is not part of
            // the backup: it can be megabytes, and the radio keeps its own copy only until it is replaced.
            if differs.is_empty() {
                let (iddb_blocks, iddb_note) = if iddb { let (b, n) = send_iddb(&mut radio, &image, &say)?; (Some(b), n) } else { (None, None) };
                return Ok(WriteDone {
                    blocks: 0,
                    iddb_blocks,
                    iddb_note,
                    backup: std::path::PathBuf::new(),
                });
            }
            say(WriteMsg::Phase(
                "Saving a backup of the radio's codeplug…".into(),
            ));
            let dir = backup_dir();
            std::fs::create_dir_all(&dir)
                .map_err(|e| format!("could not make {}: {e}", dir.display()))?;
            let name = port
                .rsplit(['/', '\\'])
                .next()
                .unwrap_or("radio")
                .to_string();
            let backup = dir.join(format!("codeplug-{}-{name}.bin", stamp()));
            std::fs::write(&backup, current.bytes())
                .map_err(|e| format!("could not save the backup {}: {e}", backup.display()))?;
            say(WriteMsg::Phase("Writing to the radio…".into()));
            let report = radio
                .write_image(basis, &image, &mut |d, t| say(WriteMsg::Progress(d, t)))
                .map_err(|e| {
                    format!(
                        "{e}. The radio's codeplug was saved first as {}.",
                        backup.display()
                    )
                })?;
            let (iddb_blocks, iddb_note) = if iddb { let (b, n) = send_iddb(&mut radio, &image, &say)?; (Some(b), n) } else { (None, None) };
            Ok(WriteDone {
                blocks: report.blocks,
                iddb_blocks,
                iddb_note,
                backup,
            })
        })();
        say(WriteMsg::Done(result));
    });
    rx
}

#[cfg(test)]
mod tests {
    use rfcps_core::iddb::{self, Options, Record};
    use rfcps_core::Image;
    use rfcps_link::{fake::FakeRadio, Radio};

    fn compact_image() -> Image {
        let recs: Vec<Record> = (1..=200).map(|i| Record { id: 5000 + i, text: format!("K{i}ABC Name {i}") }).collect();
        let mut img = Image::blank();
        iddb::write(&mut img, &recs, Options { format: b'C', ..Default::default() }).unwrap();
        img
    }

    fn radio(features: &str) -> Radio<FakeRadio> {
        let mut f = FakeRadio::new(Image::blank().bytes());
        f.serve_iddb = true;
        f.features = features.into();
        Radio::new(f)
    }

    #[test]
    fn a_compact_database_goes_to_an_old_radio_as_format_n() {
        let mut r = radio("FM,DMR,CPS_FAST");
        let (blocks, note) = super::send_iddb(&mut r, &compact_image(), &|_| {}).unwrap();
        assert!(blocks > 0);
        assert!(note.unwrap().contains("older ID database format"));
        let got = Image::from_bytes(r.port_mut().image());
        let h = iddb::read_header(got.bytes()).unwrap();
        assert_eq!((h.format, h.count), (b'N', 200));
        assert_eq!(iddb::read(got.bytes()).len(), 200);
    }

    #[test]
    fn a_compact_database_goes_to_a_radio_that_reads_it_as_it_is() {
        let mut r = radio("FM,DMR,CPS_FAST,IDDB_COMPACT");
        let (_, note) = super::send_iddb(&mut r, &compact_image(), &|_| {}).unwrap();
        assert!(note.is_none());
        let got = Image::from_bytes(r.port_mut().image());
        assert_eq!(iddb::read_header(got.bytes()).unwrap().format, b'C');
    }

    #[test]
    fn the_stamp_is_a_date_and_a_time() {
        let s = super::stamp();
        assert_eq!(s.len(), 15);
        assert_eq!(&s[8..9], "-");
        assert!(s[..4].parse::<u32>().unwrap() >= 2026);
    }
}
