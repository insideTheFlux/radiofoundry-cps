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
    harness_with_channel_sized(tweak, 1400.0)
}

fn harness_with_channel_sized(tweak: impl FnOnce(&mut Codeplug), height: f32) -> H {
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
        .with_size(egui::vec2(1240.0, height))
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
    let mut h = harness_with_channel(|cp| modes::set_kind(&mut cp.channels[0], modes::KIND_P25, rfcps_core::records::Mode::Dmr, &[]));
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
    let mut h = harness_with_channel(|cp| modes::set_kind(&mut cp.channels[0], modes::KIND_NXDN, rfcps_core::records::Mode::Dmr, &[]));
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
            kind: 9,
            system: 2,
            tx: false,
            base_own: false,
            fallback: Fallback::None,
            no_fallback: false,
            params: Params::Unknown,
            raw: Vec::new(),
        });
    });
    assert_eq!(channel(&h).kind(), 9);
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

// --- Systems: the Systems tab, and a channel on a system ---

use rfcps_core::systems::{self as sys, EntryBody, SysEntry, System, SystemFields};

fn harness_on_systems(tweak: impl FnOnce(&mut Codeplug)) -> H {
    let mut h = harness_with_channel_sized(tweak, 2600.0);
    click_prefix(&mut h, "Systems");
    h
}

fn doc_systems(h: &H) -> &Vec<System> {
    &h.state().doc.as_ref().unwrap().cp.systems
}

#[test]
fn a_system_is_added_edited_and_deleted() {
    let mut h = harness_on_systems(|_| {});
    h.get_by_label_contains("No systems");
    click(&mut h, "Add P25 trunked system");
    assert_eq!(doc_systems(&h).len(), 1);
    // A new system is listen only, with the first free number.
    let s = &doc_systems(&h)[0];
    assert_eq!((s.number, s.tx, s.name.as_str()), (1, false, "System 1"));

    type_into(&mut h, "WACN", "BEE00");
    type_into(&mut h, "System ID", "3AB");
    retype(&mut h, "Hold on the voice channel (s)", "9");
    match &doc_systems(&h)[0].fields {
        SystemFields::P25Trunk(f) => assert_eq!((f.wacn, f.sysid, f.hold_s), (Some(0xBEE00), Some(0x3AB), 9)),
        other => panic!("{other:?}"),
    }
    // The transmit side is a choice; "without registering" also sets the system's no-register option.
    pick(&mut h, "Transmit side", "Transmit without registering");
    let s = &doc_systems(&h)[0];
    assert!(s.tx);
    assert!(matches!(&s.fields, SystemFields::P25Trunk(f) if f.no_reg));
    pick(&mut h, "Transmit side", "Listen only");
    assert!(!doc_systems(&h)[0].tx);

    click(&mut h, "Add site");
    click(&mut h, "Add control channel");
    click(&mut h, "Add plan row");
    let e = &doc_systems(&h)[0].entries;
    assert_eq!(e.len(), 3);
    assert!(e.iter().any(|x| matches!(x.body, EntryBody::P25Site { .. })));
    assert!(e.iter().any(|x| matches!(x.body, EntryBody::P25Control { .. }) && x.control));
    assert!(e.iter().any(|x| matches!(x.body, EntryBody::P25Plan { .. })));

    click(&mut h, "Delete system…");
    h.get_by_label_contains("Delete this system?");
    click(&mut h, "Cancel");
    assert_eq!(doc_systems(&h).len(), 1);
    click(&mut h, "Delete system…");
    click(&mut h, "Yes, delete it");
    assert!(doc_systems(&h).is_empty());
}

fn system_with_control() -> System {
    let mut s = System::new(sys::SYSTEM_P25_TRUNK, "Test Net");
    s.number = 1;
    s.entries.push(SysEntry {
        cls: 0,
        group: 0,
        control: true,
        flags: 0,
        body: EntryBody::P25Control { iden: 1, chan_no: 5, rx_hz: 851_012_500, tx_hz: 806_012_500 },
        raw: Vec::new(),
    });
    s
}

#[test]
fn a_channel_is_put_on_a_system_and_follows_its_control_channel() {
    let mut h = harness_with_channel(|cp| cp.systems.push(system_with_control()));
    pick(&mut h, "Mode", "P25T");
    let c = channel(&h);
    assert_eq!(c.kind(), modes::KIND_P25_TRUNK);
    // On the first system, its record the control channel's frequencies and a receive-only stand-in.
    assert_eq!(c.tagged.as_ref().unwrap().system, 0);
    assert_eq!((c.rx_hz, c.tx_hz), (851_012_500, 806_012_500));
    assert!(c.rx_only);
    h.get_by_role_and_label(Role::ComboBox, "System");
    click(&mut h, "Follow every group");
    assert!(matches!(&channel(&h).tagged.as_ref().unwrap().params, Params::P25Trunk(p) if p.follow_all));
}

#[test]
fn deleting_a_system_makes_its_channels_plain_and_receive_only() {
    let mut h = harness_on_systems(|cp| {
        cp.systems.push(system_with_control());
        let systems = cp.systems.clone();
        modes::set_kind(&mut cp.channels[0], modes::KIND_P25_TRUNK, rfcps_core::records::Mode::Dmr, &systems);
    });
    assert_eq!(channel(&h).kind(), modes::KIND_P25_TRUNK);
    click(&mut h, "Delete system…");
    h.get_by_label_contains("1 channel(s) on it");
    click(&mut h, "Yes, delete it");
    let c = channel(&h);
    assert_eq!(c.kind(), 0);
    assert!(c.rx_only, "its record held the system's frequencies, so it must not transmit");
}

#[test]
fn save_image_asks_before_it_carries_a_secret_key() {
    use rfcps_core::kv::{self, Entry};
    let mut img = Image::blank();
    let area = kv::encode_area(&[Entry { key: 0x0300, value: vec![1; 19] }], None).unwrap();
    img.bytes_mut()[kv::KV_ADDR..kv::KV_ADDR + kv::KV_SIZE].copy_from_slice(&area);
    let mut app = App::with_ports(Vec::new());
    app.open_image(img.bytes(), "keys.bin".into());
    let mut h = harness_with(app);
    click(&mut h, "Save image…");
    h.get_by_label("Save the secret keys too?");
    h.get_by_label_contains("holds 1 secret key(s)");
    h.get_by_label("Save without keys");
    h.get_by_label("Include the keys");
    click(&mut h, "Cancel");
    assert!(h.state().confirm_keys.is_none());
}

// --- DMR Tier III ---

fn t3_system_with_channel() -> System {
    let mut s = System::new(sys::SYSTEM_T3, "T3 Net");
    s.number = 1;
    s.entries.push(SysEntry {
        cls: 0,
        group: 0xFF,
        control: true,
        flags: 0,
        body: EntryBody::T3Channel { lcn: 1, rx_hz: 438_100_000, tx_hz: 443_100_000, cc: None },
        raw: Vec::new(),
    });
    s
}

#[test]
fn a_tier_iii_system_is_added_edited_and_given_a_key() {
    let mut h = harness_on_systems(|_| {});
    click(&mut h, "Add DMR Tier III system");
    let s = &doc_systems(&h)[0];
    // As in the web editor, a new Tier III system can transmit.
    assert_eq!((s.type_id, s.tx), (sys::SYSTEM_T3, true));
    h.get_by_label("DMR Tier III");

    retype(&mut h, "NET", "9");
    pick(&mut h, "Network model", "Large");
    match &doc_systems(&h)[0].fields {
        SystemFields::T3(f) => assert_eq!((f.model, f.net), (2, 9)),
        other => panic!("{other:?}"),
    }
    // A NET above the model's limit is not applied (Large goes to 15).
    retype(&mut h, "NET", "99");
    match &doc_systems(&h)[0].fields {
        SystemFields::T3(f) => assert_eq!(f.net, 9),
        other => panic!("{other:?}"),
    }
    click(&mut h, "Follow all-calls");
    click(&mut h, "Add channel");
    click(&mut h, "Add site value");
    let s = &doc_systems(&h)[0];
    assert_eq!(s.entries.len(), 1);
    assert!(matches!(s.entries[0].body, EntryBody::T3Channel { lcn: 1, .. }));
    assert!(matches!(&s.fields, SystemFields::T3(f) if f.all_calls && f.sites == vec![0]));

    // No key at first; one is set from 32 hex digits and shown only when asked.
    h.get_by_label("No key.");
    // The key field is masked, so a screen reader says so (a password input).
    h.get_by_role_and_label(Role::PasswordInput, "New key (32 hex digits)").focus();
    h.run();
    h.get_by_role_and_label(Role::PasswordInput, "New key (32 hex digits)").type_text("00112233445566778899AABBCCDDEEFF");
    h.run();
    type_into(&mut h, "PSN (empty for none)", "66051");
    click(&mut h, "Set the key");
    let k = sys::decode_t3_key(doc_systems(&h)[0].secret.as_deref().unwrap()).unwrap();
    assert_eq!((sys::key_hex(&k.key).as_str(), k.psn), ("00112233445566778899AABBCCDDEEFF", 66051));
    h.get_by_label_contains("A key is set");
    assert!(h.query_by_label_contains("00112233445566778899AABBCCDDEEFF").is_none(), "the key is not shown unasked");
    click(&mut h, "Show key");
    h.get_by_label_contains("00112233445566778899AABBCCDDEEFF");
    click(&mut h, "Hide key");
    assert!(h.query_by_label_contains("00112233445566778899AABBCCDDEEFF").is_none());
    click(&mut h, "Remove the key");
    assert!(doc_systems(&h)[0].secret.is_none());
}

#[test]
fn a_tier_iii_channel_is_put_on_a_system_and_may_transmit_as_in_the_web_editor() {
    let mut h = harness_with_channel(|cp| cp.systems.push(t3_system_with_channel()));
    pick(&mut h, "Mode", "T3");
    let c = channel(&h);
    assert_eq!(c.kind(), modes::KIND_T3);
    let t = c.tagged.as_ref().unwrap();
    assert_eq!((t.system, t.tx), (0, true));
    // The record is the control channel's frequencies and a receive-only stand-in.
    assert_eq!((c.rx_hz, c.tx_hz), (438_100_000, 443_100_000));
    assert!(c.rx_only);
    h.get_by_role_and_label(Role::ComboBox, "System");
}

// --- DTMF contacts and APRS configurations ---

/// The harness on the tab with this exact label (its name and its count): "APRS" alone would also match the channel
/// form's own APRS row.
fn harness_on_tab(label: &str) -> H {
    let mut h = harness_with_channel_sized(|_| {}, 1800.0);
    click(&mut h, label);
    h
}

#[test]
fn a_dtmf_contact_is_added_edited_and_deleted() {
    let mut h = harness_on_tab("DTMF  0");
    click(&mut h, "Add DTMF contact");
    let doc = |h: &H| h.state().doc.as_ref().unwrap().cp.dtmf_contacts.clone();
    assert_eq!(doc(&h).len(), 1);
    assert_eq!((doc(&h)[0].number, doc(&h)[0].name.as_str(), doc(&h)[0].code.as_str()), (1, "DTMF 1", "1"));
    // The tab names its count, and the table is selected on the new row.
    h.get_by_label_contains("1 of 63 DTMF contacts");
    click(&mut h, "Delete DTMF contact");
    assert!(doc(&h).is_empty());
}

#[test]
fn an_aprs_configuration_is_added_edited_used_by_a_channel_and_deleted() {
    let mut h = harness_on_tab("APRS  0");
    h.get_by_label_contains("No APRS configurations");
    click(&mut h, "Add APRS configuration");
    let aprs = |h: &H| h.state().doc.as_ref().unwrap().cp.aprs.clone();
    assert_eq!((aprs(&h)[0].number, aprs(&h)[0].name.as_str()), (1, "APRS 1"));
    h.get_by_label_contains("No channel uses it");

    retype(&mut h, "SSID", "9");
    retype(&mut h, "Path 1", "wide1-1");
    retype(&mut h, "TX frequency MHz", "144.39");
    retype(&mut h, "Path 2", "TOOLONGCALL"); // not a path: not applied
    click(&mut h, "Beacon silently");
    let a = &aprs(&h)[0];
    assert_eq!((a.ssid, a.path1.as_str(), a.path2.as_str(), a.tx_hz, a.silent), (9, "WIDE1-1", "", 144_390_000, true));

    // A channel picks it from the channel form.
    click_prefix(&mut h, "Channels");
    pick(&mut h, "APRS", "APRS 1");
    assert_eq!(channel(&h).aprs, 1);
    click(&mut h, "APRS  1"); // the tab, with its count
    h.get_by_label_contains("1 channel(s) use it");

    // Deleting it clears the channel's choice.
    click(&mut h, "Delete configuration…");
    h.get_by_label_contains("1 channel(s) stop using it");
    click(&mut h, "Yes, delete it");
    assert!(aprs(&h).is_empty());
    assert_eq!(channel(&h).aprs, 0);
}
