// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux

//! The open codeplug: the image it came from, the decoded model being edited, and the undo history.

use eframe::egui;
use rfcps_core::{Codeplug, Image};
use std::time::{Duration, Instant};

const UNDO_LIMIT: usize = 200;
const COALESCE: Duration = Duration::from_millis(900);

/// One step of history: the model as it was, and the image too when the step changed the ID database (which lives in
/// the base image, so an undo of it has to bring the image back).
struct Snap {
    cp: Codeplug,
    base: Option<Image>,
}

pub struct Doc {
    pub source: String,
    pub base: Image,
    pub cp: Codeplug,
    undo: Vec<Snap>,
    redo: Vec<Snap>,
    last_push: Option<(Instant, Option<egui::Id>)>,
    /// Bumped on every edit, undo and redo.
    pub generation: u64,
    dirty_at: Option<(u64, bool)>,
    /// The image last saved to a file (or written to the radio): what "unsaved changes" is measured against. The
    /// write to the radio is measured against `base` instead, so saving a file never switches the write off.
    saved: Image,
    unsaved_at: Option<(u64, bool)>,
    /// Opened from a file and not yet written to a radio: the radio's own codeplug is unknown, so writing it is
    /// allowed even with no edits, and the write is measured against what the radio holds at that moment.
    pub from_file: bool,
    /// The ID database was changed here and the radio has not been sent it (an undo or redo of it sets this too: the
    /// radio may hold either state, and a write sends only the blocks that differ).
    pub iddb_pending: bool,
}

impl Doc {
    pub fn from_image(image: Image, source: String) -> Self {
        let cp = Codeplug::decode(&image);
        let base_copy = image.clone();
        Self {
            source,
            base: image,
            cp,
            undo: Vec::new(),
            redo: Vec::new(),
            last_push: None,
            generation: 0,
            dirty_at: None,
            saved: base_copy,
            unsaved_at: None,
            from_file: false,
            iddb_pending: false,
        }
    }

    /// Call after the UI ran, with the model as it was before: records the edit for undo. Typing in one field
    /// counts as one edit.
    pub fn note(&mut self, before: Codeplug, focus: Option<egui::Id>) {
        if self.cp == before {
            return;
        }
        let now = Instant::now();
        let same = matches!(self.last_push, Some((t, f)) if now.duration_since(t) < COALESCE && f == focus && focus.is_some());
        if !same {
            self.undo.push(Snap { cp: before, base: None });
            if self.undo.len() > UNDO_LIMIT {
                self.undo.remove(0);
            }
        }
        self.last_push = Some((now, focus));
        self.redo.clear();
        self.generation += 1;
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Takes the history step `snap` back, returning what it replaces (for the other stack).
    fn swap(&mut self, snap: Snap) -> Snap {
        let now = Snap {
            cp: std::mem::replace(&mut self.cp, snap.cp),
            base: snap.base.as_ref().map(|_| self.base.clone()),
        };
        if let Some(b) = snap.base {
            self.base = b;
            self.iddb_pending = true;
        }
        self.last_push = None;
        self.generation += 1;
        now
    }

    pub fn undo(&mut self) {
        if let Some(prev) = self.undo.pop() {
            let now = self.swap(prev);
            self.redo.push(now);
        }
    }

    pub fn redo(&mut self) {
        if let Some(next) = self.redo.pop() {
            let now = self.swap(next);
            self.undo.push(now);
        }
    }

    /// Changes the ID database, which lives in the base image: `f` edits a copy of the image, and the step can be
    /// undone like any edit. The model's header follows.
    pub fn change_iddb<R>(&mut self, f: impl FnOnce(&mut Image) -> R) -> R {
        let (cp, base) = (self.cp.clone(), self.base.clone());
        let r = f(&mut self.base);
        self.cp.iddb = rfcps_core::iddb::read_header(self.base.bytes());
        self.undo.push(Snap { cp, base: Some(base) });
        if self.undo.len() > UNDO_LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.last_push = None;
        self.iddb_pending = true;
        self.generation += 1;
        r
    }

    /// The edited image: the model written onto the image it came from.
    pub fn encoded(&self) -> Image {
        self.cp.encode(&self.base)
    }

    /// Whether the edits change the image (cached per edit).
    pub fn dirty(&mut self) -> bool {
        if let Some((g, d)) = self.dirty_at {
            if g == self.generation {
                return d;
            }
        }
        let d = self.encoded().bytes() != self.base.bytes();
        self.dirty_at = Some((self.generation, d));
        d
    }

    /// Whether the edits are not in a saved file (or the radio) yet.
    pub fn unsaved(&mut self) -> bool {
        if let Some((g, d)) = self.unsaved_at {
            if g == self.generation {
                return d;
            }
        }
        let d = self.encoded().bytes() != self.saved.bytes();
        self.unsaved_at = Some((self.generation, d));
        d
    }

    /// After the image was written to the radio: the edited image becomes the base, and counts as saved.
    pub fn mark_saved(&mut self) {
        self.base = self.encoded();
        self.saved = self.base.clone();
        self.dirty_at = None;
        self.unsaved_at = None;
        self.from_file = false;
        self.iddb_pending = false;
    }

    /// After the image was saved to a file. The radio still holds the old image, so the base stays as it is.
    pub fn mark_file_saved(&mut self) {
        self.saved = self.encoded();
        self.unsaved_at = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rfcps_core::records::{CallType, Contact};

    fn doc() -> Doc {
        Doc::from_image(Image::blank(), "test".into())
    }

    fn add_contact(d: &mut Doc, focus: Option<egui::Id>) {
        let before = d.cp.clone();
        d.cp.contacts.push(Contact {
            number: 1,
            name: "A".into(),
            id: 5,
            call_type: CallType::Group,
            timeslot: 0,
        });
        d.note(before, focus);
    }

    #[test]
    fn an_edit_makes_the_document_dirty_and_undo_takes_it_back() {
        let mut d = doc();
        assert!(!d.dirty());
        add_contact(&mut d, None);
        assert!(d.dirty() && d.can_undo() && !d.can_redo());
        d.undo();
        assert!(!d.dirty() && !d.can_undo() && d.can_redo());
        assert!(d.cp.contacts.is_empty());
        d.redo();
        assert!(d.dirty() && d.cp.contacts.len() == 1);
    }

    #[test]
    fn typing_in_one_field_is_one_undo_step() {
        let mut d = doc();
        add_contact(&mut d, None);
        let id = Some(egui::Id::unique("name"));
        for name in ["B", "Bo", "Bob"] {
            let before = d.cp.clone();
            d.cp.contacts[0].name = name.into();
            d.note(before, id);
        }
        // the add and one coalesced rename
        d.undo();
        assert_eq!(d.cp.contacts[0].name, "A");
        d.undo();
        assert!(d.cp.contacts.is_empty());
    }

    #[test]
    fn saving_makes_the_edited_image_the_base() {
        let mut d = doc();
        add_contact(&mut d, None);
        assert!(d.dirty());
        d.mark_saved();
        assert!(!d.dirty());
    }

    #[test]
    fn no_change_records_nothing() {
        let mut d = doc();
        let before = d.cp.clone();
        d.note(before, None);
        assert!(!d.can_undo());
    }

    #[test]
    fn setting_weather_channels_rx_only_makes_the_document_dirty() {
        // Real data stays out of the repo: the test runs only when RFCPS_TEST_IMAGE names a codeplug image.
        let Some(path) = std::env::var_os("RFCPS_TEST_IMAGE") else {
            return;
        };
        let mut d = Doc::from_image(Image::from_bytes(&std::fs::read(path).unwrap()), "real".into());
        assert!(!d.dirty());
        d.mark_saved();
        let before = d.cp.clone();
        let n = rfcps_core::freqs::set_weather_rx_only(&mut d.cp);
        d.note(before, None);
        assert!(n > 0, "no weather channels found");
        assert!(d.dirty(), "the edit did not make the document dirty");
    }

    #[test]
    fn saving_a_file_does_not_switch_off_the_write_to_the_radio() {
        let mut d = doc();
        add_contact(&mut d, None);
        assert!(d.dirty() && d.unsaved());
        d.mark_file_saved();
        assert!(d.dirty(), "the radio still lacks the edit");
        assert!(!d.unsaved());
        d.mark_saved();
        assert!(!d.dirty() && !d.unsaved());
    }

    #[test]
    fn a_file_opened_document_stays_writable_until_it_is_written() {
        let mut d = doc();
        d.from_file = true;
        assert!(!d.dirty(), "no edits, yet the main window enables the write by from_file");
        d.mark_file_saved();
        assert!(d.from_file, "saving a file does not count as writing to the radio");
        d.mark_saved();
        assert!(!d.from_file);
    }

    fn put_db(d: &mut Doc, n: u32) {
        use rfcps_core::iddb::{write_n, Record};
        let recs: Vec<Record> = (1..=n).map(|i| Record { id: 1000 + i, text: format!("Name {i}") }).collect();
        d.change_iddb(|img| write_n(img, &recs, 16, usize::MAX)).unwrap();
    }

    #[test]
    fn an_id_database_lives_in_the_image_and_undo_brings_the_old_one_back() {
        let mut d = doc();
        assert!(d.cp.iddb.is_none() && !d.iddb_pending);
        put_db(&mut d, 3);
        assert_eq!(d.cp.iddb.as_ref().map(|h| h.count), Some(3));
        assert!(d.iddb_pending && d.can_undo());
        assert_eq!(rfcps_core::iddb::read(d.encoded().bytes()).len(), 3);
        // replacing it, then undoing, brings the three back
        put_db(&mut d, 5);
        d.undo();
        assert_eq!(d.cp.iddb.as_ref().map(|h| h.count), Some(3));
        assert_eq!(rfcps_core::iddb::read(d.base.bytes()).len(), 3);
        d.redo();
        assert_eq!(rfcps_core::iddb::read(d.base.bytes()).len(), 5);
        d.undo();
        d.undo();
        assert!(d.cp.iddb.is_none());
        assert!(rfcps_core::iddb::read(d.base.bytes()).is_empty());
    }

    #[test]
    fn an_id_database_does_not_make_the_codeplug_dirty_but_writing_clears_the_pending_flag() {
        let mut d = doc();
        put_db(&mut d, 2);
        assert!(!d.dirty(), "the codeplug records did not change");
        assert!(d.iddb_pending);
        d.mark_saved();
        assert!(!d.iddb_pending);
        assert_eq!(d.cp.iddb.as_ref().map(|h| h.count), Some(2));
    }

    #[test]
    fn a_codeplug_edit_between_database_changes_undoes_in_order() {
        let mut d = doc();
        put_db(&mut d, 2);
        add_contact(&mut d, None);
        put_db(&mut d, 4);
        d.undo();
        assert_eq!(d.cp.iddb.as_ref().map(|h| h.count), Some(2));
        assert_eq!(d.cp.contacts.len(), 1);
        d.undo();
        assert!(d.cp.contacts.is_empty());
        assert_eq!(d.cp.iddb.as_ref().map(|h| h.count), Some(2));
    }
}
