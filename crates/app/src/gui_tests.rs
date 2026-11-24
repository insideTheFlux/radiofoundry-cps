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

const USERS: &str = "RADIO_ID,CALLSIGN,FIRST_NAME,LAST_NAME,CITY,STATE,COUNTRY\n\
    3100001,K6ABC,Ann,Smith,San Jose,California,United States\n\
    3100002,W1AW,Hiram,Maxim,Newington,Connecticut,United States\n\
    2620001,DL1XYZ,Hans,Mueller,Berlin,,Germany\n";

#[test]
fn the_id_database_is_built_from_a_user_file_and_undone() {
    let mut app = App::with_ports(Vec::new());
    app.new_codeplug();
    app.iddb.load_users(USERS, "user.csv".into());
    let mut h = tall(app);
    click_prefix(&mut h, "ID Database");
    assert!(h.state().tab == Tab::IdDb);
    h.get_by_label_contains("3 of 3 users selected. ID database size:");
    h.get_by_label("The codeplug has no DMR ID database.");
    click(&mut h, "Write ID Database");
    {
        let d = h.state().doc.as_ref().unwrap();
        assert_eq!(d.cp.iddb.as_ref().map(|x| x.count), Some(3));
        assert!(d.iddb_pending);
    }
    h.get_by_label_contains("The ID database now holds 3 records");
    // writing again asks first
    click(&mut h, "Write ID Database");
    h.get_by_label_contains("This replaces the current ID database of 3 records");
    click(&mut h, "Cancel");
    // the toolbar's write is on, though no codeplug record changed
    assert!(!disabled_by_prefix(&h, "Undo"));
    click(&mut h, "Remove ID Database");
    click(&mut h, "Remove");
    assert!(h.state().doc.as_ref().unwrap().cp.iddb.is_none());
    click(&mut h, "Undo");
    assert_eq!(h.state().doc.as_ref().unwrap().cp.iddb.as_ref().map(|x| x.count), Some(3));
}

/// A database of `n` short records in format N (16 characters each), in the open codeplug's image.
fn with_database(app: &mut App, n: u32) {
    let recs: Vec<rfcps_core::iddb::Record> = (1..=n).map(|i| rfcps_core::iddb::Record { id: i, text: "Ann".into() }).collect();
    let doc = app.doc.as_mut().unwrap();
    doc.change_iddb(|img| rfcps_core::iddb::write_n(img, &recs, 16, usize::MAX)).unwrap();
}

#[test]
fn a_database_too_big_for_the_radio_is_flagged_and_cut_to_fit() {
    use rfcps_core::iddb;
    let mut app = App::with_ports(Vec::new());
    app.new_codeplug();
    // area 1 holds `per1` records of this size; 60 more run on into area 2
    let per1 = ((iddb::AREA1_SIZE - iddb::HEADER) / 15) as u32;
    with_database(&mut app, per1 + 60);
    let end = iddb::AREA2 + 20 * 15;
    app.radio_facts = worker::RadioFacts { compact: Some(false), end: Some(end) };
    let mut h = tall(app);
    click_prefix(&mut h, "ID Database");
    h.get_by_label_contains("This database is too big for the radio");
    click(&mut h, "Cut to fit");
    h.get_by_label_contains(&format!("The ID database was cut to {} records", per1 + 20));
    let d = h.state().doc.as_ref().unwrap();
    assert!(iddb::overflow(d.base.bytes(), end).is_none());
    assert_eq!(d.cp.iddb.as_ref().map(|x| x.count), Some(per1 + 20));
    // the cut is one undo step
    click(&mut h, "Undo");
    assert_eq!(h.state().doc.as_ref().unwrap().cp.iddb.as_ref().map(|x| x.count), Some(per1 + 60));
}

#[test]
fn a_database_that_fits_or_a_radio_that_did_not_say_gets_no_warning() {
    use rfcps_core::iddb;
    let mut app = App::with_ports(Vec::new());
    app.new_codeplug();
    let per1 = ((iddb::AREA1_SIZE - iddb::HEADER) / 15) as u32;
    with_database(&mut app, per1 + 60);
    // the radio's end is not known: nothing to compare with
    let mut h = tall(app);
    click_prefix(&mut h, "ID Database");
    assert!(h.query_by_label_contains("too big for the radio").is_none());
    h.state_mut().radio_facts = worker::RadioFacts { compact: Some(false), end: Some(iddb::AREA2 + 100 * 15) };
    h.run();
    assert!(h.query_by_label_contains("too big for the radio").is_none());
}

#[test]
fn a_database_header_the_editor_cannot_parse_is_shown_and_removable() {
    use rfcps_core::iddb;
    let mut app = App::with_ports(Vec::new());
    app.new_codeplug();
    // the tag and format letter, a record size below the minimum, a count: not a header read_header takes
    let doc = app.doc.as_mut().unwrap();
    doc.change_iddb(|img| img.bytes_mut()[iddb::AREA1..iddb::AREA1 + 12].copy_from_slice(&[0x49, 0x64, b'N', 0x30, 0x30, 0x30, 0x31, 0, 5, 0, 0, 0]));
    let mut h = tall(app);
    click_prefix(&mut h, "ID Database");
    h.get_by_label_contains("an ID database this editor can't read or rebuild");
    click(&mut h, "Remove ID Database");
    click(&mut h, "Remove");
    assert!(!iddb::unreadable(h.state().doc.as_ref().unwrap().base.bytes()));
    h.get_by_label("The codeplug has no DMR ID database.");
}

#[test]
fn users_become_private_contacts_once() {
    let mut app = App::with_ports(Vec::new());
    app.new_codeplug();
    app.iddb.load_users(USERS, "user.csv".into());
    let mut h = tall(app);
    click_prefix(&mut h, "ID Database");
    click(&mut h, "Add as Contacts");
    assert_eq!(h.state().doc.as_ref().unwrap().cp.contacts.len(), 3);
    h.get_by_label_contains("3 contacts added.");
    click(&mut h, "Add as Contacts");
    h.get_by_label_contains("No contacts were added. 3 were already contacts.");
    assert_eq!(h.state().doc.as_ref().unwrap().cp.contacts.len(), 3);
    click(&mut h, "Undo");
    assert!(h.state().doc.as_ref().unwrap().cp.contacts.is_empty());
}

/// A tall window, so the whole ID database tab is on screen (a click outside the visible area lands on nothing).
fn tall(app: App) -> H {
    let mut h = Harness::builder()
        .with_size(egui::vec2(1240.0, 1800.0))
        .build_eframe(move |cc| {
            theme::apply(&cc.egui_ctx);
            app
        });
    h.run();
    h
}

#[test]
fn the_database_format_follows_what_the_radio_reads_and_an_old_one_can_be_converted() {
    use worker::RadioFacts;
    // a radio that reads only format N: the database is written as N
    let mut app = App::with_ports(Vec::new());
    app.new_codeplug();
    app.radio_facts = RadioFacts { compact: Some(false), end: None };
    app.iddb.load_users(USERS, "user.csv".into());
    let mut h = tall(app);
    click_prefix(&mut h, "ID Database");
    click(&mut h, "Write ID Database");
    let hd = h.state().doc.as_ref().unwrap().cp.iddb.clone().unwrap();
    assert_eq!((hd.format, hd.count), (b'N', 3));
    h.get_by_label_contains("holds a DMR ID database of 3 records");
    // the radio is then found to read format C: the offer to convert
    h.state_mut().radio_facts = RadioFacts { compact: Some(true), end: None };
    h.run();
    h.get_by_label_contains("This radio reads a compact database");
    click(&mut h, "Convert");
    let hd = h.state().doc.as_ref().unwrap().cp.iddb.clone().unwrap();
    assert_eq!((hd.format, hd.count), (b'C', 3));
    h.get_by_label_contains("holds a compact DMR ID database of 3 records");
    click(&mut h, "Undo");
    assert_eq!(h.state().doc.as_ref().unwrap().cp.iddb.as_ref().unwrap().format, b'N');
    // with no radio known, a new database is compact
    let mut app = App::with_ports(Vec::new());
    app.new_codeplug();
    app.iddb.load_users(USERS, "user.csv".into());
    let mut h = tall(app);
    click_prefix(&mut h, "ID Database");
    click(&mut h, "Write ID Database");
    assert_eq!(h.state().doc.as_ref().unwrap().cp.iddb.as_ref().unwrap().format, b'C');
}

#[test]
fn the_user_file_downloads_on_a_click_and_a_failure_is_said() {
    let mut app = App::with_ports(Vec::new());
    app.new_codeplug();
    app.iddb.url = download::tests::serve(
        "HTTP/1.1 200 OK\r\nContent-Length: {len}\r\nConnection: close\r\n\r\n",
        USERS.as_bytes().to_vec(),
    );
    let mut h = tall(app);
    click_prefix(&mut h, "ID Database");
    h.get_by_label("Open a RadioID user file to begin.");
    click(&mut h, "Download from RadioID.net");
    for _ in 0..200 {
        std::thread::sleep(std::time::Duration::from_millis(10));
        h.run();
        if h.query_by_label_contains("3 users from").is_some() {
            break;
        }
    }
    h.get_by_label_contains("3 users from RadioID.net.");
    h.get_by_label_contains("3 of 3 users selected.");
    // a server that is not there
    h.state_mut().iddb.url = "http://127.0.0.1:1/user.csv".into();
    h.run();
    click(&mut h, "Download from RadioID.net");
    for _ in 0..200 {
        std::thread::sleep(std::time::Duration::from_millis(10));
        h.run();
        if h.query_by_label_contains("could not reach").is_some() {
            break;
        }
    }
    h.get_by_label_contains("could not reach");
}

#[test]
fn users_are_fetched_from_the_api_for_chosen_regions() {
    let (url, seen) = radioid_api::tests::serve(|p, _| {
        let page: u32 = p.split("page=").nth(1).and_then(|x| x.parse().ok()).unwrap_or(1);
        let row = |id: u32, c: &str| format!(r#"{{"id":{id},"callsign":"{c}","fname":"Ann","surname":"S","city":"X","state":"Ohio","country":"United States"}}"#);
        let body = if page == 1 { format!(r#"{{"count":3,"pages":2,"results":[{},{}]}}"#, row(10, "K1AA"), row(11, "K2BB")) } else { format!(r#"{{"count":3,"pages":2,"results":[{}]}}"#, row(12, "W3CC")) };
        (200, body)
    });
    let mut app = App::with_ports(Vec::new());
    app.new_codeplug();
    app.iddb.use_api(url, vec![("United States".into(), Some("Ohio".into()))]);
    let mut h = tall(app);
    click_prefix(&mut h, "ID Database");
    h.get_by_label("Add regions and fetch their users to begin.");
    h.get_by_label_contains("RadioID API key");
    click(&mut h, "Fetch Users");
    for _ in 0..400 {
        std::thread::sleep(std::time::Duration::from_millis(10));
        h.run();
        if h.query_by_label_contains("3 users from RadioID.net").is_some() {
            break;
        }
    }
    h.get_by_label_contains("3 users from RadioID.net.");
    h.get_by_label_contains("3 of 3 users selected.");
    assert_eq!(seen.lock().unwrap().len(), 2, "page 1 counted once, then page 2");
    click(&mut h, "Add as Contacts");
    assert_eq!(h.state().doc.as_ref().unwrap().cp.contacts.len(), 3);
}

#[test]
fn a_choice_on_the_tab_is_kept_for_the_next_run() {
    let path = std::env::temp_dir().join(format!("rfcps-gui-prefs-{}", std::process::id())).join("settings.json");
    let mut app = App::with_ports(Vec::new());
    app.new_codeplug();
    app.iddb.keep_choices_in(path.clone());
    let mut h = tall(app);
    click_prefix(&mut h, "ID Database");
    assert!(!path.exists());
    click(&mut h, "Fetch from RadioID.net");
    h.run();
    let text = std::fs::read_to_string(&path).expect("written after the change");
    assert!(text.contains("\"mode\": \"api\""), "{text}");
    // a new run starts in that mode
    let mut app = App::with_ports(Vec::new());
    app.new_codeplug();
    app.iddb.keep_choices_in(path.clone());
    let mut h = tall(app);
    click_prefix(&mut h, "ID Database");
    h.get_by_label("Add regions and fetch their users to begin.");
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}

#[test]
fn add_from_radioid_on_the_contacts_tab_opens_the_id_database_tab() {
    let mut app = App::with_ports(Vec::new());
    app.new_codeplug();
    app.iddb.load_users(USERS, "user.csv".into());
    let mut h = tall(app);
    click_prefix(&mut h, "Contacts");
    assert!(h.state().tab == Tab::Contacts);
    click(&mut h, "Add from RadioID");
    assert!(h.state().tab == Tab::IdDb);
    h.get_by_label_contains("3 of 3 users selected.");
}

#[test]
fn a_big_user_list_is_counted_off_the_screen_thread_and_the_buttons_wait() {
    // more users than the tab works out at once
    let mut csv = String::from("RADIO_ID,CALLSIGN,FIRST_NAME,LAST_NAME,CITY,STATE,COUNTRY\n");
    for i in 0..25_000u32 {
        csv += &format!("{},K{}ABC,Ann,Smith,Town {},Ohio,United States\n", 3_000_000 + i, i % 10, i % 50);
    }
    let mut app = App::with_ports(Vec::new());
    app.new_codeplug();
    app.iddb.load_users(&csv, "big.csv".into());
    let mut h = tall(app);
    click_prefix(&mut h, "ID Database");
    // at first the answer is on its way and the buttons are off
    h.get_by_label_contains("Counting…");
    assert!(disabled(&h, "Write ID Database"));
    for _ in 0..1000 {
        std::thread::sleep(std::time::Duration::from_millis(10));
        h.run();
        if h.query_by_label_contains("25,000 of 25,000 users selected.").is_some() {
            break;
        }
    }
    h.get_by_label_contains("25,000 of 25,000 users selected.");
    assert!(!disabled(&h, "Write ID Database"));
    // a change of the choices starts it again
    h.state_mut().iddb.set_prefixes("K1");
    for _ in 0..1000 {
        std::thread::sleep(std::time::Duration::from_millis(10));
        h.run();
        if h.query_by_label_contains("2,500 of 25,000 users selected.").is_some() {
            break;
        }
    }
    h.get_by_label_contains("2,500 of 25,000 users selected.");
}

fn disabled_by_prefix(h: &H, prefix: &str) -> bool {
    h.get_by_label_contains(prefix).accesskit_node().is_disabled()
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
    let mut h = Harness::builder()
        .with_size(egui::vec2(1240.0, 680.0))
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
    assert_eq!(t.params, Params::Nxdn(NxdnConv { rate9600: false, squelch: 0, rx_ran: 5, tx_ran: 9, tx_key_id: None }));
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

#[test]
fn an_nxdn_system_keeps_scrambler_keys_shown_only_when_asked() {
    let mut h = harness_on_systems(|_| {});
    click(&mut h, "Add NXDN conventional system");
    let s = &doc_systems(&h)[0];
    // Like a Tier III system, a new NXDN system can transmit.
    assert_eq!((s.type_id, s.tx), (sys::SYSTEM_NXDN, true));
    h.get_by_label("NXDN conventional");
    click(&mut h, "Add key");
    click(&mut h, "Add key");
    let ids = |h: &H| doc_systems(h)[0].traffic_keys.iter().map(|k| k.id).collect::<Vec<_>>();
    assert_eq!(ids(&h), vec![0, 1]);
    // A seed is a secret: not shown until asked.
    assert!(h.query_by_role_and_label(Role::TextInput, "Scrambler seed, key 1").is_none());
    click(&mut h, "Show scrambler seeds");
    retype(&mut h, "Scrambler seed, key 1", "4242");
    retype(&mut h, "Scrambler seed, key 2", "0"); // a seed is 1 to 32767: not applied
    assert_eq!(doc_systems(&h)[0].traffic_keys.iter().map(|k| k.seed).collect::<Vec<_>>(), vec![4242, 1]);
    retype(&mut h, "Key ID, key 2", "64"); // a key ID is 0 to 63: not applied
    assert_eq!(ids(&h), vec![0, 1]);
    retype(&mut h, "Key ID, key 2", "0");
    h.get_by_label_contains("Key ID 0 is already used");
    retype(&mut h, "Key ID, key 2", "7");
    assert_eq!(ids(&h), vec![0, 7]);
    // The radio's ID on an NXDN system is a unit ID: up to 65519.
    type_into(&mut h, "This radio's ID on the system", "70000");
    assert_eq!(doc_systems(&h)[0].own_id, 0);
    retype(&mut h, "This radio's ID on the system", "1234");
    assert_eq!(doc_systems(&h)[0].own_id, 1234);
}

#[test]
fn an_nxdn_channel_picks_its_system_and_transmit_key() {
    let mut h = harness_with_channel(|cp| {
        let mut s = System::new(sys::SYSTEM_NXDN, "Net");
        s.traffic_keys = vec![sys::TrafficKey { slot: None, id: 5, seed: 99, flags: 0 }];
        cp.systems.push(s);
    });
    pick(&mut h, "Mode", "NXDN");
    // The system is optional: a new NXDN channel is on none, and sends clear.
    let t = channel(&h).tagged.clone().unwrap();
    assert_eq!(t.system, 0xFF);
    pick(&mut h, "System", "1  Net");
    assert_eq!(channel(&h).tagged.as_ref().unwrap().system, 0);
    pick(&mut h, "Transmit key", "Key 5");
    match &channel(&h).tagged.as_ref().unwrap().params {
        modes::Params::Nxdn(p) => assert_eq!(p.tx_key_id, Some(5)),
        other => panic!("{other:?}"),
    }
    pick(&mut h, "Transmit key", "Clear");
    match &channel(&h).tagged.as_ref().unwrap().params {
        modes::Params::Nxdn(p) => assert_eq!(p.tx_key_id, None),
        other => panic!("{other:?}"),
    }
}

#[test]
fn an_aes_system_keeps_aes_keys_shown_only_when_asked() {
    let mut h = harness_on_systems(|_| {});
    click(&mut h, "Add DMR AES-256 system");
    let s = &doc_systems(&h)[0];
    assert_eq!((s.type_id, s.tx), (sys::SYSTEM_AES, true));
    assert_eq!(s.fields, SystemFields::Aes { network: 1 });
    h.get_by_label("DMR AES-256");
    click(&mut h, "Add AES-256 key");
    click(&mut h, "Add AES-256 key");
    let keys = |h: &H| doc_systems(h)[0].aes_keys.clone();
    let ids = |h: &H| keys(h).iter().map(|k| k.id).collect::<Vec<_>>();
    assert_eq!(ids(&h), vec![0, 1]);
    // A new key starts random, and no two are alike
    let (a, b) = (keys(&h)[0].key, keys(&h)[1].key);
    assert!(a != [0u8; 32] && b != [0u8; 32] && a != b);
    // A key is a secret: not shown until asked.
    assert!(h.query_by_role_and_label(Role::TextInput, "AES-256 key, key 1").is_none());
    click(&mut h, "Show AES keys");
    let hex64 = "00112233445566778899AABBCCDDEEFF00112233445566778899AABBCCDDEEFF";
    retype(&mut h, "AES-256 key, key 1", hex64);
    assert_eq!(rfcps_core::systems::key_hex_32(&keys(&h)[0].key), hex64);
    // Not 64 hex digits: not applied
    retype(&mut h, "AES-256 key, key 2", "1234");
    assert_eq!(keys(&h)[1].key, b);
    // Generate replaces a key
    h.get_all_by_label("Generate").next().unwrap().click();
    h.run();
    assert!(keys(&h)[0].key != rfcps_core::systems::parse_key_hex_32(hex64).unwrap());
    // A PI key ID is 0 to 255, and unique in the system
    retype(&mut h, "PI key ID, key 2", "256");
    assert_eq!(ids(&h), vec![0, 1]);
    retype(&mut h, "PI key ID, key 2", "0");
    h.get_by_label_contains("PI key ID 0 is already used");
    retype(&mut h, "PI key ID, key 2", "200");
    assert_eq!(ids(&h), vec![0, 200]);
    retype(&mut h, "Network ID", "9");
    assert_eq!(doc_systems(&h)[0].fields, SystemFields::Aes { network: 9 });
}

#[test]
fn an_aes_channel_picks_its_system_and_key_and_keeps_its_dmr_fields() {
    let mut h = harness_with_channel(|cp| {
        let mut s = System::new(sys::SYSTEM_AES, "Fire");
        s.aes_keys = vec![sys::AesKey { slot: None, id: 12, key: [7; 32], flags: 0 }];
        cp.systems.push(s);
        sys::number_systems(&mut cp.systems);
    });
    pick(&mut h, "Mode", "AES");
    let t = channel(&h).tagged.clone().unwrap();
    // It starts on the first AES system, with no key, and transmits (as the web editor starts one)
    assert_eq!((t.kind, t.system, t.tx), (modes::KIND_AES, 0, true));
    assert_eq!(t.params, Params::Aes(modes::AesChan { key_id: None }));
    assert!(channel(&h).rx_only && channel(&h).mode == rfcps_core::records::Mode::Dmr);
    // The DMR fields stay; the FM ones and APRS do not
    h.get_by_role_and_label(Role::ComboBox, "Color Code");
    h.get_by_role_and_label(Role::ComboBox, "TX Contact");
    assert!(h.query_by_role_and_label(Role::ComboBox, "APRS").is_none());
    pick(&mut h, "PI key ID", "Key 12");
    assert_eq!(channel(&h).tagged.as_ref().unwrap().params, Params::Aes(modes::AesChan { key_id: Some(12) }));
    pick(&mut h, "PI key ID", "None");
    assert_eq!(channel(&h).tagged.as_ref().unwrap().params, Params::Aes(modes::AesChan { key_id: None }));
}

#[test]
fn the_recommended_ham_systems_are_added_from_a_site_s_control_channel() {
    let mut h = harness_on_systems(|_| {});
    click(&mut h, "Recommended ham Tier III system…");
    type_into(&mut h, "Control channel RX (MHz)", "438.1");
    retype(&mut h, "Site number (0 to 1022)", "17");
    click(&mut h, "Add system");
    let s = &doc_systems(&h)[0];
    assert_eq!((s.type_id, s.name.as_str(), s.tx), (sys::SYSTEM_T3, "Ham Tier III", true));
    match &s.fields {
        SystemFields::T3(f) => assert_eq!((f.model, f.net, f.color_code, f.register_talkgroups), (3, 1, 1, true)),
        other => panic!("{other:?}"),
    }
    // The control channel's channel number is 4 x site + 1, and its TX is its RX when none is given.
    assert!(matches!(s.entries[0].body, EntryBody::T3Channel { lcn: 69, rx_hz: 438_100_000, tx_hz: 438_100_000, .. }));

    click(&mut h, "Recommended ham P25 system…");
    type_into(&mut h, "Control channel RX (MHz)", "851.0125");
    type_into(&mut h, "Control channel TX (MHz, blank: as RX)", "806.0125");
    type_into(&mut h, "Name for the new system", "Pacific");
    click(&mut h, "Add system");
    let s = &doc_systems(&h)[1];
    assert_eq!((s.type_id, s.name.as_str()), (sys::SYSTEM_P25_TRUNK, "Pacific"));
    assert!(matches!(&s.fields, SystemFields::P25Trunk(f) if f.wacn == Some(0x32350) && f.cc_nac == Some(0x293) && f.skip_enc));
    assert!(matches!(s.entries[0].body, EntryBody::P25Control { rx_hz: 851_012_500, tx_hz: 806_012_500, .. }));

    // Nothing is added without a control channel.
    click(&mut h, "Recommended ham P25 system…");
    assert!(disabled(&h, "Add system"));
    click(&mut h, "Cancel");
    assert_eq!(doc_systems(&h).len(), 2);
}

#[test]
fn a_tx_frequency_in_a_receive_only_band_is_refused_unless_the_rx_is_there() {
    let mut h = harness_with_channel(|_| {});
    let before = channel(&h).tx_hz;
    // The channel's RX is a repeater band: a TX typed in the air band (118 MHz) is not applied.
    type_into(&mut h, "TX MHz", "118.0");
    assert_eq!(channel(&h).tx_hz, before);
    // An RX typed in the air band makes the channel RX only with TX on RX.
    retype(&mut h, "RX MHz", "118.0");
    let c = channel(&h);
    assert_eq!((c.rx_hz, c.tx_hz, c.rx_only), (118_000_000, 118_000_000, true));
}

#[test]
fn an_rx_only_channel_grays_its_transmit_fields_and_keeps_their_values() {
    let mut h = harness_with_channel(|cp| {
        let c = &mut cp.channels[0];
        c.tx_hz = 146_940_000; // an offset repeater pair
        c.tot = 180;
        c.tx_tone = "88.5".into();
        c.vox = true;
        c.rx_only = false;
    });
    let off = |h: &H| {
        let text = |l: &str| h.get_by_role_and_label(Role::TextInput, l).accesskit_node().is_disabled();
        (
            text("TX MHz"),
            h.get_by_role_and_label(Role::ComboBox, "TX Timeout (s)").accesskit_node().is_disabled(),
            // the table has a cell of the same name: the form's is the last
            h.get_all_by_role_and_label(Role::TextInput, "TX tone, channel 1").last().unwrap().accesskit_node().is_disabled(),
            h.get_by_role_and_label(Role::ComboBox, "Power").accesskit_node().is_disabled(),
            h.get_by_label("VOX Allowed").accesskit_node().is_disabled(),
            h.get_by_label("1750 Hz Tone Burst").accesskit_node().is_disabled(),
        )
    };
    // transmitting: every one is open
    assert_eq!(off(&h), (false, false, false, false, false, false));
    click(&mut h, "RX Only");
    assert!(channel(&h).rx_only);
    assert_eq!(off(&h), (true, true, true, true, true, true));
    // The values are kept, and the table's cells are blank and locked
    let c = channel(&h);
    assert_eq!((c.tx_hz, c.tot, c.tx_tone.as_str(), c.vox), (146_940_000, 180, "88.5", true));
    assert!(h.get_by_role_and_label(Role::TextInput, "TX MHz, channel 1").accesskit_node().is_disabled());
    assert!(h.query_by_role_and_label(Role::ComboBox, "Power, channel 1").is_none());
    // RX Only off again: they come back as they were
    click(&mut h, "RX Only");
    assert_eq!(off(&h), (false, false, false, false, false, false));
    let c = channel(&h);
    assert_eq!((c.tx_hz, c.tot, c.tx_tone.as_str(), c.vox), (146_940_000, 180, "88.5", true));
}

#[test]
fn a_tx_frequency_in_a_receive_only_band_stays_open_to_be_corrected() {
    // an import left TX in the air band with RX on a repeater: RX Only, with the TX cell open
    let mut h = harness_with_channel(|cp| {
        let c = &mut cp.channels[0];
        c.tx_hz = 121_500_000;
        rfcps_core::freqs::follow_listen(c);
    });
    assert!(channel(&h).rx_only);
    assert!(!h.get_by_role_and_label(Role::TextInput, "TX MHz").accesskit_node().is_disabled());
    assert!(h.get_by_role_and_label(Role::ComboBox, "TX Timeout (s)").accesskit_node().is_disabled());
    // the RX in the air band: TX is locked
    retype(&mut h, "RX MHz", "118.0");
    assert!(h.get_by_role_and_label(Role::TextInput, "TX MHz").accesskit_node().is_disabled());
}

#[test]
fn a_tone_is_picked_from_the_standard_list_and_searched() {
    let mut h = harness_with_channel(|_| {});
    // only the form has the list button: the table has no room for it
    assert_eq!(h.get_all_by_label("Tones, tx tone, channel 1").count(), 1);
    h.get_by_label("Tones, tx tone, channel 1").click();
    h.step();
    h.run();
    // the list is searchable: D023I is the second entry of the codes
    type_into(&mut h, "Search tones", "D023");
    click(&mut h, "D023I");
    assert_eq!(channel(&h).tx_tone, "D023I");
    // and a tone typed by hand is still taken
    h.get_all_by_role_and_label(Role::TextInput, "TX tone, channel 1").last().unwrap().focus();
    h.run();
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    h.run();
    h.get_all_by_role_and_label(Role::TextInput, "TX tone, channel 1").last().unwrap().type_text("100");
    h.run();
    assert_eq!(channel(&h).tx_tone, "100.0");
}

#[test]
fn the_tx_timeout_is_a_list_of_the_radio_default_and_15_second_steps() {
    let mut h = harness_with_channel(|_| {});
    assert_eq!(channel(&h).tot, 0);
    pick(&mut h, "TX Timeout (s)", "1:30 (90 s)");
    assert_eq!(channel(&h).tot, 90);
    pick(&mut h, "TX Timeout (s)", "45 s");
    assert_eq!(channel(&h).tot, 45);
    pick(&mut h, "TX Timeout (s)", "Radio default");
    assert_eq!(channel(&h).tot, 0);
}

#[test]
fn a_refused_tx_frequency_says_why() {
    let mut h = harness_with_channel(|_| {});
    let before = channel(&h).tx_hz;
    retype(&mut h, "TX MHz", "121.5");
    assert_eq!(channel(&h).tx_hz, before);
    h.get_by_label_contains("121.500 MHz is not a supported transmit frequency.");
    // a usable one does not
    retype(&mut h, "TX MHz", "146.94");
    assert_eq!(channel(&h).tx_hz, 146_940_000);
    assert!(h.query_by_label_contains("not a supported transmit frequency").is_none());
}

#[test]
fn accept_invalid_crc_is_a_dmr_channel_setting() {
    let mut h = harness_with_channel(|_| {});
    pick(&mut h, "Mode", "DMR");
    assert!(!channel(&h).accept_bad_crc);
    click(&mut h, "Accept Invalid CRC");
    assert!(channel(&h).accept_bad_crc);
}

// --- The file chooser runs on its own thread ---

#[test]
fn a_file_chooser_does_not_hold_the_window_and_its_answer_is_taken_when_it_comes() {
    let mut h = harness();
    let path = std::env::temp_dir().join(format!("rfcps-chooser-test-{}.bin", std::process::id()));
    std::fs::write(&path, rfcps_core::Image::blank().bytes()).unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    h.state_mut().chooser = Some((Choose::Open, rx));
    // The chooser is open: the window keeps running, and a second click on Open does not start another chooser.
    h.run();
    h.get_all_by_label("Open image…").next().unwrap().click();
    h.run();
    assert!(h.state().doc.is_none());
    assert!(h.state().chooser.is_some());
    // The answer arrives: the file is opened.
    tx.send(vec![path.clone()]).unwrap();
    h.run();
    assert!(h.state().chooser.is_none());
    assert!(h.state().doc.as_ref().is_some_and(|d| d.from_file));
    // Cancelled (no path): nothing changes.
    let (tx, rx) = std::sync::mpsc::channel();
    h.state_mut().chooser = Some((Choose::Open, rx));
    tx.send(Vec::new()).unwrap();
    h.run();
    assert!(h.state().chooser.is_none());
    assert!(h.state().doc.is_some());
    let _ = std::fs::remove_file(path);
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
