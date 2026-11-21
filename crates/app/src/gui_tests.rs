// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux

//! Automated GUI tests: the real `App` runs headless in egui_kittest (AccessKit), and the tests click its buttons,
//! tabs and menus by their labels. No window, no display, no radio: the app is built with no serial ports, and no test
//! opens a file dialog or presses the final write button.
//!
//! `RFCPS_IMAGE=<codeplug image> cargo test -p rfcps gui` also walks every tab of a real image (the file is only read).

use super::*;
use egui_kittest::kittest::{NodeT, Queryable};
use egui_kittest::Harness;

type H = Harness<'static, App>;

fn harness_with(app: App) -> H {
    let mut h = Harness::builder()
        .with_size(egui::vec2(1240.0, 680.0))
        .build_eframe(move |cc| {
            theme::apply(&cc.egui_ctx);
            app
        });
    h.run();
    h
}

fn harness() -> H {
    harness_with(App::with_ports(Vec::new()))
}

/// A harness with an empty codeplug open (what "Start a new, empty codeplug" gives).
fn harness_with_doc() -> H {
    let mut app = App::with_ports(Vec::new());
    app.new_codeplug();
    harness_with(app)
}

fn click(h: &mut H, label: &str) {
    h.get_by_label(label).click();
    h.run();
}

fn click_prefix(h: &mut H, prefix: &str) {
    h.get_by_label_contains(prefix).click();
    h.run();
}

fn disabled(h: &H, label: &str) -> bool {
    h.get_by_label(label).accesskit_node().is_disabled()
}

#[test]
fn welcome_screen_offers_the_first_steps() {
    let h = harness();
    h.get_by_label("Program your radio's channels, zones and contacts.");
    // The welcome screen's big button and the toolbar's.
    assert_eq!(h.get_all_by_label("Read from radio").count(), 2);
    assert_eq!(h.get_all_by_label("Open image…").count(), 2);
    h.get_by_label("Start a new, empty codeplug");
    h.get_by_label("Open an image file, or read from a radio.");
    assert!(h.state().doc.is_none());
}

#[test]
fn with_no_port_nothing_can_be_written_or_saved() {
    let h = harness();
    assert!(disabled(&h, "Write to radio"));
    assert!(disabled(&h, "Save image…"));
    assert!(disabled(&h, "Undo"));
    assert!(disabled(&h, "Redo"));
}

#[test]
fn a_new_codeplug_opens_from_the_welcome_screen() {
    let mut h = harness();
    click(&mut h, "Start a new, empty codeplug");
    assert!(h.state().doc.is_some());
    assert_eq!(h.state().doc.as_ref().unwrap().source, "new codeplug");
    h.get_by_label_contains("Channels");
    h.get_by_label_contains("Zones");
    h.get_by_label_contains("Contacts");
    h.get_by_label_contains("RX Group Lists");
    h.get_by_label("General");
}

#[test]
fn every_tab_opens_and_draws() {
    let mut h = harness_with_doc();
    for (prefix, tab) in [
        ("Zones", Tab::Zones),
        ("Contacts", Tab::Contacts),
        ("RX Group Lists", Tab::Lists),
        ("Channels", Tab::Channels),
    ] {
        click_prefix(&mut h, prefix);
        assert!(h.state().tab == tab, "{prefix} did not open");
    }
    click(&mut h, "General");
    assert!(h.state().tab == Tab::General);
}

#[test]
fn the_import_and_export_menus_list_their_formats() {
    let mut h = harness_with_doc();
    click(&mut h, "Import");
    for item in [
        "CHIRP CSV…",
        "qdmr codeplug (YAML)…",
        "CSV tables…",
        "Stock DM-32UV backup…",
    ] {
        h.get_by_label(item);
    }
    click(&mut h, "Export");
    for item in [
        "CSV tables (a folder)…",
        "qdmr codeplug (YAML)…",
        "CHIRP CSV…",
    ] {
        h.get_by_label(item);
    }
}

#[test]
fn new_asks_before_it_throws_away_edits() {
    let mut h = harness_with_doc();
    h.state_mut().confirm_new = true;
    h.run();
    h.get_by_label("Start a new codeplug?");
    click(&mut h, "Cancel");
    assert!(!h.state().confirm_new);
    assert!(h.state().doc.is_some());

    h.state_mut().confirm_new = true;
    h.run();
    click(&mut h, "Start new");
    assert!(!h.state().confirm_new);
    assert_eq!(h.state().doc.as_ref().unwrap().source, "new codeplug");
}

#[test]
fn the_notice_dialog_closes_with_ok() {
    let mut h = harness_with_doc();
    h.state_mut().notice = Some((
        "Import CHIRP CSV".into(),
        vec!["3 channels imported".into()],
    ));
    h.run();
    h.get_by_label("Import CHIRP CSV");
    h.get_by_label("3 channels imported");
    click(&mut h, "OK");
    assert!(h.state().notice.is_none());
}

#[test]
fn the_theme_buttons_switch_the_theme() {
    let mut h = harness_with_doc();
    click(&mut h, "Light");
    assert_eq!(
        h.ctx.options(|o| o.theme_preference),
        egui::ThemePreference::Light
    );
    click(&mut h, "Dark");
    assert_eq!(
        h.ctx.options(|o| o.theme_preference),
        egui::ThemePreference::Dark
    );
    click(&mut h, "Auto");
    assert_eq!(
        h.ctx.options(|o| o.theme_preference),
        egui::ThemePreference::System
    );
}

#[test]
fn the_problems_button_hides_and_shows_the_list() {
    let mut h = harness_with_doc();
    let (e, w) = {
        let p = &h.state().problems;
        (
            problems::error_count(p),
            p.iter().filter(|p| p.level == Level::Warning).count(),
        )
    };
    let label = if e + w == 0 {
        "No problems".to_string()
    } else {
        format!("{e} error(s), {w} warning(s)")
    };
    let before = h.state().show_problems;
    click(&mut h, &label);
    assert_ne!(h.state().show_problems, before);
    click(&mut h, &label);
    assert_eq!(h.state().show_problems, before);
}

#[test]
fn a_failed_write_banner_can_be_closed() {
    let mut h = harness_with_doc();
    h.state_mut().banner = Some((false, "Write failed: no port".into()));
    h.run();
    h.get_by_label("Write failed: no port");
    click(&mut h, "x");
    assert!(h.state().banner.is_none());
}

/// Every tab of a real image, with its first row selected. Reads the file only.
#[test]
fn a_real_image_draws_on_every_tab() {
    let Some(path) = std::env::var_os("RFCPS_IMAGE") else {
        eprintln!("RFCPS_IMAGE not set: skipped");
        return;
    };
    let bytes = std::fs::read(path).expect("RFCPS_IMAGE");
    let mut app = App::with_ports(Vec::new());
    app.open_image(&bytes, "real.bin".into());
    app.doc.as_mut().unwrap().from_file = true;
    // Known gap: the Channels table's text fields and drop-downs have no accessible names (about a hundred for a
    // full radio), so the harness's accessibility check stays off here. The tests above keep it on.
    let mut h = Harness::builder()
        .with_size(egui::vec2(1240.0, 680.0))
        .with_accessibility_check(false)
        .build_eframe(move |cc| {
            theme::apply(&cc.egui_ctx);
            app
        });
    h.run();
    for (prefix, tab) in [
        ("Channels", Tab::Channels),
        ("Zones", Tab::Zones),
        ("Contacts", Tab::Contacts),
        ("RX Group Lists", Tab::Lists),
        ("General", Tab::General),
    ] {
        click_prefix(&mut h, prefix);
        assert!(h.state().tab == tab);
        h.run();
    }
    // An image opened from a file is unedited: no unsaved changes, and undo has nothing to undo.
    assert!(disabled(&h, "Undo"));
    h.get_by_label("No unsaved changes");
}
