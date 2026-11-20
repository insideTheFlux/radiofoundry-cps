// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux

//! Bringing codeplug data in from files and writing it out: CHIRP and CSV, and a stock DM-32UV backup. Each import
//! returns the lines to show the person (what came in, and what could not).

use rfcps_core::csv::{self, TableKind};
use rfcps_core::{chirp, qdmr, stock_dm32, Codeplug};
use std::io::Read;
use std::path::Path;

pub fn import_chirp_file(cp: &mut Codeplug, path: &Path) -> Result<Vec<String>, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("Could not read {}: {e}", path.display()))?;
    let r = chirp::import_chirp(cp, &text, "CHIRP")?;
    let mut lines = vec![format!("Imported {} FM channel(s) in {} zone(s). The channels and zones that were here are replaced; contacts and settings are kept.", r.channels, r.zones)];
    lines.extend(r.notes.iter().map(|(n, c)| n.text(*c)));
    Ok(lines)
}

pub fn import_qdmr_file(cp: &mut Codeplug, path: &Path) -> Result<Vec<String>, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("Could not read {}: {e}", path.display()))?;
    let r = qdmr::import_qdmr(cp, &text)?;
    let mut lines = vec![format!(
        "Imported {} channel(s), {} contact(s), {} RX group list(s) and {} zone(s) from the qdmr file. The tables that were here are replaced; the VFOs and the settings qdmr does not hold are kept.",
        r.channels, r.contacts, r.rx_lists, r.zones
    )];
    lines.extend(r.notes.iter().map(|(n, c)| n.text(*c)));
    Ok(lines)
}

pub fn export_qdmr_file(cp: &Codeplug, path: &Path) -> Result<Vec<String>, String> {
    let (text, notes) = qdmr::export_qdmr(cp);
    std::fs::write(path, text).map_err(|e| format!("Could not write {}: {e}", path.display()))?;
    let mut out = vec![format!("Wrote the qdmr codeplug to {}.", path.display())];
    out.extend(notes.iter().map(|(n, c)| n.text(*c)));
    Ok(out)
}

pub fn import_csv_files(
    cp: &mut Codeplug,
    paths: &[std::path::PathBuf],
) -> Result<Vec<String>, String> {
    let mut files = Vec::new();
    for p in paths {
        let text = std::fs::read_to_string(p)
            .map_err(|e| format!("Could not read {}: {e}", p.display()))?;
        let name = p.file_name().map_or_else(
            || p.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        );
        files.push((name, text));
    }
    let (results, skipped) = csv::import_all(cp, &files);
    if results.is_empty() {
        return Err("None of those files is a codeplug table (no table's columns match).".into());
    }
    let mut lines = Vec::new();
    for (name, r) in &results {
        lines.push(format!(
            "{name}: {} added, {} updated, {} removed.",
            r.added, r.updated, r.removed
        ));
        for p in r.problems.iter().take(8) {
            let at = if p.row == 0 {
                String::new()
            } else {
                format!("row {}", p.row)
            };
            lines.push(format!(
                "  {at} {}: {}{}",
                p.column,
                p.message,
                if p.skipped { " (row skipped)" } else { "" }
            ));
        }
        if r.problems.len() > 8 {
            lines.push(format!("  and {} more.", r.problems.len() - 8));
        }
        if !r.unknown.is_empty() {
            lines.push(format!("  Columns not used: {}.", r.unknown.join(", ")));
        }
    }
    for s in skipped {
        lines.push(format!("{s}: not a codeplug table, skipped."));
    }
    Ok(lines)
}

/// The data flash of a stock DM-32UV from a backup: a `.zip` from the web installer (its `data-memory.bin`) or the raw
/// file.
fn stock_flash(path: &Path) -> Result<Vec<u8>, String> {
    let is_zip = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("zip"));
    if !is_zip {
        return std::fs::read(path).map_err(|e| format!("Could not read {}: {e}", path.display()));
    }
    let f =
        std::fs::File::open(path).map_err(|e| format!("Could not open {}: {e}", path.display()))?;
    let mut z = zip::ZipArchive::new(f)
        .map_err(|e| format!("{} is not a zip file: {e}", path.display()))?;
    let mut entry = z.by_name("data-memory.bin").map_err(|_| {
        "That zip has no data-memory.bin: it is not a radio backup from the web installer."
            .to_string()
    })?;
    let mut data = Vec::new();
    entry
        .read_to_end(&mut data)
        .map_err(|e| format!("Could not read data-memory.bin: {e}"))?;
    Ok(data)
}

pub fn import_stock_file(cp: &mut Codeplug, path: &Path) -> Result<Vec<String>, String> {
    let data = stock_flash(path)?;
    if !stock_dm32::detect(&data) {
        return Err(
            "That is not a stock DM-32UV backup: no stock codeplug was found in it.".into(),
        );
    }
    let r = stock_dm32::import_stock(cp, &data)?;
    let mut lines = vec![format!(
        "Imported {} channel(s), {} contact(s), {} RX group list(s) and {} zone(s) from the stock codeplug. Everything that was here is replaced.",
        r.channels, r.contacts, r.rx_lists, r.zones
    )];
    lines.extend(r.notes.iter().map(|(n, c)| n.text(*c)));
    Ok(lines)
}

/// Writes every table as a CSV file into `dir`.
pub fn export_csv_folder(cp: &Codeplug, dir: &Path) -> Result<Vec<String>, String> {
    let mut lines = Vec::new();
    for (name, text) in csv::export_all(cp) {
        let p = dir.join(name);
        std::fs::write(&p, text).map_err(|e| format!("Could not write {}: {e}", p.display()))?;
        lines.push(p.display().to_string());
    }
    let mut out = vec![format!(
        "Wrote {} CSV files to {}:",
        lines.len(),
        dir.display()
    )];
    out.extend(lines.into_iter().map(|l| format!("  {l}")));
    out.push(format!(
        "({})",
        [
            TableKind::Contacts,
            TableKind::RxLists,
            TableKind::Channels,
            TableKind::Zones
        ]
        .map(TableKind::name)
        .join(", ")
    ));
    Ok(out)
}

pub fn export_chirp_file(cp: &Codeplug, path: &Path) -> Result<Vec<String>, String> {
    let (text, dmr) = chirp::export_chirp(cp);
    std::fs::write(path, text).map_err(|e| format!("Could not write {}: {e}", path.display()))?;
    let mut out = vec![format!("Wrote the FM channels to {}.", path.display())];
    if dmr > 0 {
        out.push(chirp::Note::Dmr.text(dmr));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rfcps_core::Image;

    #[test]
    fn a_chirp_file_round_trips_through_the_files_api() {
        let dir = std::env::temp_dir().join(format!("rfcps-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut cp = Codeplug::decode(&Image::blank());
        cp.channels.push(rfcps_core::records::Channel::new(
            1,
            146_520_000,
            146_520_000,
        ));
        let p = dir.join("c.csv");
        export_chirp_file(&cp, &p).unwrap();
        let mut back = Codeplug::decode(&Image::blank());
        let lines = import_chirp_file(&mut back, &p).unwrap();
        assert_eq!(back.channels.len(), 1);
        assert!(lines[0].starts_with("Imported 1 FM channel"));
        let out = dir.join("csv");
        std::fs::create_dir_all(&out).unwrap();
        export_csv_folder(&cp, &out).unwrap();
        let files: Vec<_> = [
            "channels.csv",
            "contacts.csv",
            "rx-group-lists.csv",
            "zones.csv",
        ]
        .iter()
        .map(|n| out.join(n))
        .collect();
        let mut again = Codeplug::decode(&Image::blank());
        import_csv_files(&mut again, &files).unwrap();
        assert_eq!(again.channels.len(), 1);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_missing_or_wrong_file_is_an_error_not_a_panic() {
        let mut cp = Codeplug::decode(&Image::blank());
        assert!(import_chirp_file(&mut cp, Path::new("/nonexistent/x.csv")).is_err());
        assert!(import_stock_file(&mut cp, Path::new("/nonexistent/x.zip")).is_err());
    }

    #[test]
    fn a_qdmr_file_round_trips_through_the_files_api() {
        let dir = std::env::temp_dir().join(format!("rfcps-qdmr-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut cp = Codeplug::decode(&Image::blank());
        cp.channels.push(rfcps_core::records::Channel::new(1, 146_520_000, 146_520_000));
        let f = dir.join("c.yaml");
        let out = export_qdmr_file(&cp, &f).unwrap();
        assert!(out[0].contains("c.yaml"));
        let mut again = Codeplug::decode(&Image::blank());
        let lines = import_qdmr_file(&mut again, &f).unwrap();
        assert!(lines[0].contains("1 channel(s)"), "{lines:?}");
        assert_eq!(again.channels[0].rx_hz, 146_520_000);
        std::fs::write(dir.join("bad.yaml"), "a: [1\n").unwrap();
        assert!(import_qdmr_file(&mut again, &dir.join("bad.yaml")).is_err());
        assert!(import_qdmr_file(&mut again, &dir.join("missing.yaml")).is_err());
        std::fs::remove_dir_all(&dir).ok();
    }
}
