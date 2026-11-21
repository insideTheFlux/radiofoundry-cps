// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux

//! Automated GUI tests: the real `App` runs headless in egui_kittest (AccessKit), and the tests click its buttons,
//! tabs and menus by their labels. No window, no display, no radio: the app is built with no serial ports, and no test
//! opens a file dialog or presses the final write button.
//!
//! `RFCPS_IMAGE=<codeplug image> cargo test -p rfcps gui` also walks every tab of a real image (the file is only read).

use super::*;
use egui::accesskit::Role;
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

// --- The General tab's settings-area fields ---

/// The General tab is taller than the usual window, and a click outside the visible area lands on nothing: a tall
/// window shows every field.
fn harness_on_general() -> H {
    let mut app = App::with_ports(Vec::new());
    app.new_codeplug();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1240.0, 1800.0))
        .build_eframe(move |cc| {
            theme::apply(&cc.egui_ctx);
            app
        });
    h.run();
    click(&mut h, "General");
    h
}

/// Types into a text field: it must have focus first (a click alone doesn't give it in one frame).
fn type_into(h: &mut H, label: &str, text: &str) {
    h.get_by_role_and_label(Role::TextInput, label).focus();
    h.run();
    h.get_by_role_and_label(Role::TextInput, label).type_text(text);
    h.run();
}

/// Opens a drop-down and picks an entry.
fn pick(h: &mut H, combo: &str, entry: &str) {
    h.get_by_role_and_label(Role::ComboBox, combo).click();
    h.step();
    h.run();
    click(h, entry);
}

#[test]
fn the_general_tab_offers_the_keyed_settings() {
    let h = harness_on_general();
    for l in ["ARS server ID", "IP gateway ID"] {
        h.get_by_role_and_label(Role::TextInput, l);
    }
    for l in ["Call end tone", "Transmit time-out warning", "Low battery tone", "Transmit denied tone"] {
        h.get_by_role_and_label(Role::ComboBox, l);
    }
    h.get_by_label("Start of transmission");
    h.get_by_label("End of transmission");
    h.get_by_label("End of reception");
    // A new codeplug carries no key: every one is absent.
    assert_eq!(h.state().doc.as_ref().unwrap().cp.general.keys, rfcps_core::kv::Keys::default());
}

#[test]
fn typing_an_id_sets_the_key_and_clearing_it_removes_it() {
    let mut h = harness_on_general();
    type_into(&mut h, "ARS server ID", "3100001");
    assert_eq!(h.state().doc.as_ref().unwrap().cp.general.keys.ars_server, Some(3_100_001));
    // A value no DMR ID can be is not applied.
    type_into(&mut h, "IP gateway ID", "99999999");
    assert_eq!(h.state().doc.as_ref().unwrap().cp.general.keys.ip_gateway, None);
}

#[test]
fn a_tone_can_be_set_off_and_back_to_the_radios_own() {
    let mut h = harness_on_general();
    pick(&mut h, "Call end tone", "Off");
    assert_eq!(h.state().doc.as_ref().unwrap().cp.general.keys.call_end_tone, Some(false));
    pick(&mut h, "Call end tone", "The radio's own setting");
    assert_eq!(h.state().doc.as_ref().unwrap().cp.general.keys.call_end_tone, None);
}

#[test]
fn a_signaling_tone_is_switched_on_with_its_checkbox() {
    let mut h = harness_on_general();
    click(&mut h, "Start of transmission");
    let k = &h.state().doc.as_ref().unwrap().cp.general.keys;
    assert!(k.sig[0].is_some() && k.sig[1].is_none() && k.sig[2].is_none());
    // Switched off again, it is an absent key.
    click(&mut h, "Start of transmission");
    assert!(h.state().doc.as_ref().unwrap().cp.general.keys.sig[0].is_none());
}

#[test]
fn a_newer_settings_area_is_flagged() {
    let mut app = App::with_ports(Vec::new());
    let mut img = Image::blank();
    let at = rfcps_core::kv::KV_ADDR;
    img.bytes_mut()[at..at + 4].copy_from_slice(b"RFKV");
    img.bytes_mut()[at + 4] = 2; // a later version
    app.open_image(img.bytes(), "newer.bin".into());
    let mut h = harness_with(app);
    click(&mut h, "General");
    h.get_by_label_contains("settings area is from a newer version");
}



// --- Channel modes: P25 and NXDN on the Channels tab, and the unit IDs ---

use rfcps_core::modes::{self, Fallback, NxdnConv, P25Conv, Params, Tagged};
use rfcps_core::records::Channel;
use rfcps_core::{Codeplug, Image};

/// An app with a plain FM channel 1 open and selected. The channel table's own cells have no accessible names yet
/// (see the real-image test), so the accessibility check is off for these.
fn harness_with_channel(tweak: impl FnOnce(&mut Codeplug)) -> H {
    let base = Image::blank();
    let mut cp = Codeplug::decode(&base);
    let mut c = Channel::new(1, 146_520_000, 146_520_000);
    c.name = "Simplex".into();
    cp.channels.push(c);
    tweak(&mut cp);
    let img = cp.encode(&base);
    let mut app = App::with_ports(Vec::new());
    app.open_image(img.bytes(), "modes.bin".into());
    app.channel = Some(1);
    let mut h = Harness::builder()
        .with_size(egui::vec2(1240.0, 1400.0))
        .with_accessibility_check(false)
        .build_eframe(move |cc| {
            theme::apply(&cc.egui_ctx);
            app
        });
    h.run();
    h
}

fn channel(h: &H) -> &Channel {
    &h.state().doc.as_ref().unwrap().cp.channels[0]
}

/// Replaces a text field's text: select all, then type.
fn retype(h: &mut H, label: &str, text: &str) {
    h.get_by_role_and_label(Role::TextInput, label).focus();
    h.run();
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    h.run();
    h.get_by_role_and_label(Role::TextInput, label).type_text(text);
    h.run();
}

#[test]
fn a_channel_is_made_p25_and_its_fields_are_edited() {
    let mut h = harness_with_channel(|_| {});
    assert_eq!(channel(&h).kind(), 0);
    pick(&mut h, "Mode", "P25");
    let c = channel(&h);
    assert_eq!(c.kind(), modes::KIND_P25);
    // The record is the safe stand-in: DMR, receive only. Transmit allowed is the tag's.
    assert!(c.rx_only && c.mode == rfcps_core::records::Mode::Dmr);
    assert!(c.tagged.as_ref().unwrap().tx);
    h.get_by_role_and_label(Role::TextInput, "RX NAC");
    h.get_by_role_and_label(Role::TextInput, "TX NAC");

    retype(&mut h, "RX NAC", "123");
    retype(&mut h, "TX NAC", "456");
    pick(&mut h, "Squelch", "Selective (this radio's calls)");
    let t = channel(&h).tagged.as_ref().unwrap();
    assert_eq!(t.params, Params::P25(P25Conv { squelch: 1, rx_nac: 0x123, tx_nac: 0x456 }));
    // A value that isn't a NAC is not applied.
    retype(&mut h, "RX NAC", "ZZZ");
    assert_eq!(channel(&h).tagged.as_ref().unwrap().params, Params::P25(P25Conv { squelch: 1, rx_nac: 0x123, tx_nac: 0x456 }));
}

#[test]
fn a_channel_is_made_nxdn_and_its_fields_are_edited() {
    let mut h = harness_with_channel(|_| {});
    pick(&mut h, "Mode", "NXDN");
    assert_eq!(channel(&h).kind(), modes::KIND_NXDN);
    pick(&mut h, "Rate", "4800 bit/s");
    retype(&mut h, "RX RAN", "5");
    retype(&mut h, "TX RAN", "9");
    retype(&mut h, "RX RAN", "70"); // above 63: not applied
    let t = channel(&h).tagged.as_ref().unwrap();
    assert_eq!(t.params, Params::Nxdn(NxdnConv { rate9600: false, squelch: 0, rx_ran: 5, tx_ran: 9 }));
}

#[test]
fn rx_only_on_a_channel_of_a_kind_is_the_tags_transmit_flag() {
    let mut h = harness_with_channel(|cp| modes::set_kind(&mut cp.channels[0], modes::KIND_P25, rfcps_core::records::Mode::Dmr));
    assert!(channel(&h).tagged.as_ref().unwrap().tx);
    // "RX Only" is the first checkbox of the form that is labelled so.
    h.get_by_role_and_label(Role::CheckBox, "RX Only").click();
    h.run();
    let c = channel(&h);
    assert!(!c.tagged.as_ref().unwrap().tx);
    assert!(c.rx_only, "the record stays receive only");
}

#[test]
fn going_back_to_fm_removes_the_kind() {
    let mut h = harness_with_channel(|cp| modes::set_kind(&mut cp.channels[0], modes::KIND_NXDN, rfcps_core::records::Mode::Dmr));
    assert_eq!(channel(&h).kind(), modes::KIND_NXDN);
    pick(&mut h, "Mode", "FM");
    let c = channel(&h);
    assert_eq!((c.kind(), c.mode), (0, rfcps_core::records::Mode::Fm));
    assert!(!c.rx_only, "transmit allowed returns as the record's receive-only bit");
    h.get_by_label("FM");
}

#[test]
fn a_channel_of_a_mode_this_editor_does_not_edit_is_shown_and_kept() {
    let h = harness_with_channel(|cp| {
        cp.channels[0].tagged = Some(Tagged {
            kind: modes::KIND_P25_TRUNK,
            system: 2,
            tx: false,
            base_own: false,
            fallback: Fallback::None,
            params: Params::Unknown,
            raw: Vec::new(),
        });
    });
    assert_eq!(channel(&h).kind(), modes::KIND_P25_TRUNK);
    h.get_by_label_contains("this editor does not edit");
    // Nothing about it changed: encoding the open codeplug again leaves the image as it is.
    let doc = h.state().doc.as_ref().unwrap();
    let again = doc.cp.encode(&doc.base);
    assert_eq!(again.bytes(), doc.base.bytes());
}

#[test]
fn the_unit_ids_are_set_on_the_general_tab() {
    let mut h = harness_on_general();
    h.get_by_role_and_label(Role::TextInput, "P25 unit ID");
    type_into(&mut h, "P25 unit ID", "3100001");
    type_into(&mut h, "NXDN unit ID", "70000"); // above 65519: not applied
    let ms = &h.state().doc.as_ref().unwrap().cp.mode_settings;
    assert_eq!((ms.p25_unit_id, ms.nxdn_unit_id), (3_100_001, 0));
    retype(&mut h, "NXDN unit ID", "4242");
    assert_eq!(h.state().doc.as_ref().unwrap().cp.mode_settings.nxdn_unit_id, 4242);
}
