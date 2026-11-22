// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux

//! The ID database tab: names for DMR IDs that are not contacts, built from RadioID's `user.csv` (the web editor's
//! "Build from RadioID.net", file mode). The database lives in the document's image; Write to Radio sends it.

use crate::doc::Doc;
use crate::download::{self, Download};
use crate::radioid_api::{self, Fetch};
use crate::worker::RadioFacts;
use crate::theme;
use eframe::egui::{self, Ui};
use rfcps_core::iddb;
use rfcps_core::radioid::{self, Selection, User};
use rfcps_core::regions::COUNTRIES;
use std::collections::{BTreeSet, HashMap};
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// More users than this and the size and preview are worked out on a thread, after the choices have been still for a
/// moment; fewer and they come at once.
const SYNC_LIMIT: usize = 20_000;
const SETTLE: Duration = Duration::from_millis(300);

/// What the size and preview depend on, and the users to work them out from.
struct Job {
    users: Arc<Vec<User>>,
    selection: Selection,
    template: String,
    contact_template: String,
    upper: bool,
    text_len: usize,
    format: u8,
    end: usize,
}

struct Pending {
    key: String,
    since: Instant,
    job: Option<Job>,
    rx: Option<Receiver<Cached>>,
}

fn compute(key: String, j: &Job) -> Cached {
    let sel = radioid::select(&j.users, &j.selection);
    let size = if j.format == b'C' {
        radioid::iddb_size_c(&radioid::iddb_records(&sel, &j.template, j.upper, j.text_len), j.end)
    } else {
        radioid::iddb_size(sel.len(), j.text_len, j.end)
    };
    let prev = sel
        .iter()
        .take(6)
        .map(|u| (u.id, radioid::format(u, &j.template, j.upper, j.text_len), radioid::format(u, &j.contact_template, j.upper, 16)))
        .collect();
    (key, sel.len(), prev, size)
}

/// The inputs as one string, how many users they select, the first few as (ID, database text, contact name), and the
/// room the database takes.
type Cached = (String, usize, Vec<(u32, String, String)>, radioid::Size);

/// The choices the tab remembers between runs (the web editor's `rfoundry-editor-radioid`). The API key is not one of
/// them.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Prefs {
    api: bool,
    regions: Vec<(String, Option<String>)>,
    prefixes: String,
    id_min: String,
    id_max: String,
    template: String,
    contact_template: String,
    upper: bool,
    text_len: usize,
}

const DEFAULT_TEMPLATE: &str = "{call} {first} {city} {st}";
const DEFAULT_CONTACT_TEMPLATE: &str = "{call} {first}";

impl Prefs {
    fn to_json(&self) -> String {
        let regions: Vec<serde_json::Value> = self
            .regions
            .iter()
            .map(|(c, s)| match s {
                Some(s) => serde_json::json!({"country": c, "state": s}),
                None => serde_json::json!({"country": c}),
            })
            .collect();
        serde_json::to_string_pretty(&serde_json::json!({
            "mode": if self.api { "api" } else { "file" },
            "regions": regions,
            "prefixes": self.prefixes,
            "idMin": self.id_min,
            "idMax": self.id_max,
            "template": self.template,
            "contactTemplate": self.contact_template,
            "upper": self.upper,
            "textLen": self.text_len,
        }))
        .unwrap_or_default()
    }

    /// What a file holds; anything missing or of the wrong kind is the default.
    fn from_json(text: &str) -> Self {
        let v: serde_json::Value = serde_json::from_str(text).unwrap_or(serde_json::Value::Null);
        let s = |k: &str, d: &str| v[k].as_str().map_or_else(|| d.to_string(), String::from);
        let regions = v["regions"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|r| Some((r["country"].as_str()?.to_string(), r["state"].as_str().map(String::from))))
                    .collect()
            })
            .unwrap_or_default();
        let text_len = v["textLen"].as_u64().map(|n| n as usize).filter(|n| iddb::TEXT_LENGTHS.contains(n)).unwrap_or(32);
        Prefs {
            api: v["mode"].as_str() == Some("api"),
            regions,
            prefixes: s("prefixes", ""),
            id_min: s("idMin", ""),
            id_max: s("idMax", ""),
            template: s("template", DEFAULT_TEMPLATE),
            contact_template: s("contactTemplate", DEFAULT_CONTACT_TEMPLATE),
            upper: v["upper"].as_bool().unwrap_or(false),
            text_len,
        }
    }
}

pub struct IdDbUi {
    users: Option<Arc<Vec<User>>>,
    source: String,
    country: String,
    states: BTreeSet<String>,
    regions: Vec<(String, Option<String>)>,
    prefixes: String,
    id_min: String,
    id_max: String,
    template: String,
    contact_template: String,
    upper: bool,
    text_len: usize,
    /// The last result, and whether it is a problem.
    message: Option<(String, bool)>,
    confirm: Option<Confirm>,
    /// Users per country (lower case), for the country list.
    counts: HashMap<String, usize>,
    /// What the inputs selected: the inputs as one string, how many users, and the first few as the radio shows them.
    cache: Option<Cached>,
    /// A size and preview being worked out off the screen thread (a big user list makes it take a second).
    pending: Option<Pending>,
    /// Set by the Contacts tab's shortcut: the next draw scrolls to the buttons.
    scroll_to_actions: bool,
    /// A download of the user file in progress, and where it comes from.
    download: Option<Download>,
    pub url: String,
    /// Where the users come from: the user file, or RadioID's API for the chosen regions.
    mode: Mode,
    /// The person's RadioID API key (optional), kept only while the program runs.
    key: String,
    /// A fetch from the API in progress, and where it asks.
    fetch: Option<Fetch>,
    pub api_url: String,
    /// Where the choices are kept, and what the file holds now (no file in tests).
    prefs_file: Option<(std::path::PathBuf, Prefs)>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    File,
    Api,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Confirm {
    Replace,
    Remove,
}

impl Default for IdDbUi {
    fn default() -> Self {
        Self {
            users: None,
            source: String::new(),
            country: String::new(),
            states: BTreeSet::new(),
            regions: Vec::new(),
            prefixes: String::new(),
            id_min: String::new(),
            id_max: String::new(),
            template: DEFAULT_TEMPLATE.into(),
            contact_template: DEFAULT_CONTACT_TEMPLATE.into(),
            upper: false,
            text_len: 32,
            message: None,
            confirm: None,
            counts: HashMap::new(),
            cache: None,
            pending: None,
            scroll_to_actions: false,
            download: None,
            url: radioid::DOWNLOAD.to_string(),
            mode: Mode::File,
            key: String::new(),
            fetch: None,
            api_url: radioid_api::API.to_string(),
            prefs_file: None,
        }
    }
}

/// The database format to write: C unless the radio is known to read only N. Without a radio, C: a write to a radio
/// without it converts (iddb-compact.md 6).
fn format_of(facts: RadioFacts) -> u8 {
    if facts.compact == Some(false) {
        b'N'
    } else {
        b'C'
    }
}

/// 317956 as "317,956".
fn thousands(n: usize) -> String {
    let d = n.to_string();
    let mut out = String::new();
    for (i, c) in d.chars().enumerate() {
        if i > 0 && (d.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn number(s: &str, default: u32) -> u32 {
    let s = s.trim();
    if !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()) {
        s.parse().unwrap_or(default)
    } else {
        default
    }
}

impl IdDbUi {
    fn selection(&self) -> Selection {
        Selection {
            // fetched users are the regions already
            regions: if self.mode == Mode::File { self.regions.clone() } else { Vec::new() },
            prefixes: self.prefixes.split(|c: char| c == ',' || c.is_whitespace()).map(String::from).collect(),
            id_min: number(&self.id_min, 0),
            id_max: number(&self.id_max, 0xFF_FFFF),
        }
    }

    fn selected<'a>(&self, users: &'a [User]) -> Vec<&'a User> {
        radioid::select(users, &self.selection())
    }

    fn open_file(&mut self) {
        let Some(path) = rfd::FileDialog::new().add_filter("RadioID user file", &["csv", "txt"]).pick_file() else {
            return;
        };
        let name = path.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        match std::fs::read(&path) {
            Ok(b) => self.load_users(&String::from_utf8_lossy(&b), name),
            Err(e) => self.message = Some((format!("Could not read {}: {e}", path.display()), true)),
        }
    }

    /// Takes what the download has sent; when it is done, the users.
    fn poll_download(&mut self) {
        let Some(d) = &mut self.download else { return };
        let mut done = None;
        while let Ok(m) = d.rx.try_recv() {
            match m {
                download::Msg::Progress(n, t) => {
                    d.received = n;
                    d.total = t;
                }
                download::Msg::Done(r) => done = Some(r),
            }
        }
        if let Some(r) = done {
            self.download = None;
            match r {
                Ok(text) => self.load_users(&text, "RadioID.net".into()),
                Err(e) if e == "stopped" => self.message = Some(("The download was stopped.".into(), false)),
                Err(e) => self.message = Some((format!("{e}. Try again later, or open a user.csv you downloaded."), true)),
            }
        }
    }

    fn prefs(&self) -> Prefs {
        Prefs {
            api: self.mode == Mode::Api,
            regions: self.regions.clone(),
            prefixes: self.prefixes.clone(),
            id_min: self.id_min.clone(),
            id_max: self.id_max.clone(),
            template: self.template.clone(),
            contact_template: self.contact_template.clone(),
            upper: self.upper,
            text_len: self.text_len,
        }
    }

    /// Takes the choices kept in `path` (if it is there), and keeps them there from now on.
    pub fn keep_choices_in(&mut self, path: std::path::PathBuf) {
        let p = std::fs::read_to_string(&path).map(|t| Prefs::from_json(&t)).unwrap_or_else(|_| self.prefs());
        self.mode = if p.api { Mode::Api } else { Mode::File };
        self.regions = p.regions.clone();
        self.prefixes = p.prefixes.clone();
        self.id_min = p.id_min.clone();
        self.id_max = p.id_max.clone();
        self.template = p.template.clone();
        self.contact_template = p.contact_template.clone();
        self.upper = p.upper;
        self.text_len = p.text_len;
        self.prefs_file = Some((path, p));
    }

    /// Writes the choices when they changed (a failure to write is not worth a message: they last for this run).
    fn save_choices(&mut self) {
        let now = self.prefs();
        let Some((path, saved)) = &mut self.prefs_file else { return };
        if *saved == now {
            return;
        }
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let tmp = path.with_extension("json.tmp");
        if std::fs::write(&tmp, now.to_json()).is_ok() && std::fs::rename(&tmp, &path).is_ok() {
            *saved = now;
        }
    }

    /// The Contacts tab's "Add from RadioID": the next draw scrolls to the buttons.
    pub fn show_actions(&mut self) {
        self.scroll_to_actions = true;
    }

    #[cfg(test)]
    pub fn set_prefixes(&mut self, p: &str) {
        self.prefixes = p.into();
    }

    /// For a test: the fetch mode against `url`, with `regions` chosen.
    #[cfg(test)]
    pub fn use_api(&mut self, url: String, regions: Vec<(String, Option<String>)>) {
        self.mode = Mode::Api;
        self.api_url = url;
        self.regions = regions;
    }

    /// Takes what the fetch has sent; when it is done, the users.
    fn poll_fetch(&mut self) {
        let Some(f) = &mut self.fetch else { return };
        let mut done = None;
        while let Ok(m) = f.rx.try_recv() {
            match m {
                radioid_api::Msg::Counting(i, n) => f.status = format!("Counting users… {} of {n}", i + 1),
                radioid_api::Msg::Progress(d, t) => f.status = format!("Request {d} of {t}"),
                radioid_api::Msg::Done(r) => done = Some(r),
            }
        }
        if let Some(r) = done {
            self.fetch = None;
            match r {
                Ok(users) if users.is_empty() => self.message = Some(("RadioID.net has no users for those regions.".into(), true)),
                Ok(users) => self.set_users(users, "RadioID.net".into()),
                Err(e) if e == "stopped" => self.message = Some(("The fetch was stopped.".into(), false)),
                Err(e) => self.message = Some((format!("{e} Try again later, or use the user file."), true)),
            }
        }
    }

    fn set_users(&mut self, users: Vec<User>, source: String) {
        self.counts = radioid::regions_of(&users).into_iter().map(|r| (r.country.to_lowercase(), r.count)).collect();
        self.users = Some(Arc::new(users));
        self.cache = None;
        self.pending = None;
        self.source = source;
        self.message = None;
    }

    /// The users of a `user.csv`'s text, from the file called `name`.
    pub fn load_users(&mut self, text: &str, name: String) {
        let users = radioid::parse_users(text);
        if users.is_empty() {
            self.message = Some((format!("{name} holds no DMR users. Use the user.csv file from RadioID.net."), true));
        } else {
            self.set_users(users, name);
        }
    }

    fn write_db(&mut self, doc: &mut Doc, facts: RadioFacts) {
        let Some(users) = &self.users else { return };
        let sel = self.selected(users);
        let recs = radioid::iddb_records(&sel, &self.template, self.upper, self.text_len);
        let o = iddb::Options { format: format_of(facts), text_len: self.text_len, end: facts.end.unwrap_or(usize::MAX), fresh: false };
        match doc.change_iddb(|img| iddb::write(img, &recs, o)) {
            Ok(w) => {
                let mut m = format!("The ID database now holds {} record{}. Write to Radio sends it.", w.count, if w.count == 1 { "" } else { "s" });
                if w.dropped > 0 {
                    m += &format!(" {} did not fit and {} left out.", w.dropped, if w.dropped == 1 { "was" } else { "were" });
                }
                self.message = Some((m, w.dropped > 0));
            }
            Err(e) => self.message = Some((e.to_string(), true)),
        }
    }

    fn add_contacts(&mut self, doc: &mut Doc) {
        let Some(users) = &self.users else { return };
        let sel = self.selected(users);
        let r = radioid::add_contacts(&mut doc.cp.contacts, &sel, &self.contact_template, self.upper);
        let plural = |n: usize, one: &str, many: &str| if n == 1 { one.to_string() } else { many.to_string() };
        let mut m = vec![if r.added > 0 { format!("{} contact{} added.", r.added, plural(r.added, "", "s")) } else { "No contacts were added.".into() }];
        if r.existing > 0 {
            m.push(format!("{} {} already {}.", r.existing, plural(r.existing, "was", "were"), plural(r.existing, "a contact", "contacts")));
        }
        if r.full > 0 {
            m.push(format!("{} {} left out: the contact list is full.", r.full, plural(r.full, "was", "were")));
        }
        self.message = Some((m.join(" "), r.full > 0));
    }
}

pub fn show(ui: &mut Ui, doc: &mut Doc, st: &mut IdDbUi, facts: RadioFacts) {
    let pal = theme::palette(ui);
    st.poll_download();
    st.poll_fetch();
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        ui.set_max_width(760.0);
        // The current database
        match &doc.cp.iddb {
            Some(h) => {
                let n = h.count;
                let (what, each) = if h.format == b'C' { ("a compact DMR ID database", "up to") } else { ("a DMR ID database", "") };
                ui.label(format!("The codeplug holds {what} of {n} record{}, {each} {} characters each.", if n == 1 { "" } else { "s" }, h.text_len).replace("  ", " "));
                // a database in format N for a radio that reads format C: the offer to convert (iddb-compact.md 6)
                if h.format == b'N' && facts.compact == Some(true) {
                    ui.horizontal(|ui| {
                        ui.colored_label(pal.warn, "This radio reads a compact database: smaller, with room for more names and faster writes.");
                        if ui.button("Convert").clicked() {
                            let end = facts.end.unwrap_or(usize::MAX);
                            let r = doc.change_iddb(|img| iddb::convert(img, b'C', end));
                            st.message = r.map(|w| (format!("The ID database was converted: {} records. Write to Radio sends it.", w.count), false));
                        }
                    });
                }
                if ui.button("Remove ID Database").clicked() {
                    st.confirm = Some(Confirm::Remove);
                }
            }
            None => {
                ui.label("The codeplug has no DMR ID database.");
            }
        }
        if doc.iddb_pending {
            ui.colored_label(pal.warn, "The ID database changed here: Write to Radio sends it.");
        }
        ui.add_space(8.0);
        ui.heading("Build from RadioID.net");
        ui.label(egui::RichText::new("Names for DMR IDs come from the DMR ID registry at RadioID.net.").color(pal.muted));

        ui.add_space(6.0);
        ui.strong("1  Users");
        ui.horizontal(|ui| {
            let busy = st.download.is_some() || st.fetch.is_some();
            ui.add_enabled_ui(!busy, |ui| {
                ui.selectable_value(&mut st.mode, Mode::File, "RadioID User File").on_hover_text("Download user.csv from RadioID.net and open it here. Any region, any size.");
                ui.selectable_value(&mut st.mode, Mode::Api, "Fetch from RadioID.net").on_hover_text("Fetch the users of a few states or a small country directly.");
            });
        });
        if st.mode == Mode::File {
        ui.horizontal(|ui| {
            if ui.add_enabled(st.download.is_none(), egui::Button::new("Open user.csv")).clicked() {
                st.open_file();
            }
            if st.download.is_none() {
                if ui.button("Download from RadioID.net").clicked() {
                    st.message = None;
                    st.download = Some(download::start(st.url.clone(), ui.ctx().clone()));
                }
            } else if ui.button("Stop").clicked() {
                if let Some(d) = &st.download {
                    d.stop();
                }
            }
            if let Some(d) = &st.download {
                let mb = |n: u64| format!("{:.1} MB", n as f64 / 1e6);
                ui.label(match d.total {
                    Some(t) => format!("Downloading… {} of {}", mb(d.received), mb(t)),
                    None => format!("Downloading… {}", mb(d.received)),
                });
            }
        });
        ui.label(egui::RichText::new("Download connects to radioid.net once and sends nothing but the request. Or download user.csv yourself and open it.").color(pal.muted));
        } else {
            ui.label(format!(
                "RadioID.net returns 200 users per request, so fetching suits up to about {} users. For more, use the user file.",
                radioid_api::MAX_PAGES * radioid_api::PER_PAGE
            ));
            ui.horizontal(|ui| {
                let lbl = ui.label("RadioID API key (optional)");
                ui.add(egui::TextEdit::singleline(&mut st.key).password(true).hint_text("Your API key").desired_width(260.0)).labelled_by(lbl.id);
            });
            ui.label(egui::RichText::new("Create a key in your RadioID.net account. It is kept only until you close the program and sent only to RadioID.net.").color(pal.muted));
        }
        if let Some(u) = &st.users {
            ui.label(egui::RichText::new(format!("{} users from {}.", thousands(u.len()), st.source)).color(pal.muted));
        }

        ui.add_space(6.0);
        ui.strong("2  Regions");
        let have_users = st.users.is_some() && st.mode == Mode::File;
        ui.horizontal(|ui| {
            let lbl = ui.label("Country");
            let shown = if st.country.is_empty() { "Choose a country".to_string() } else { st.country.clone() };
            let combo = egui::ComboBox::from_id_salt("rid_country").selected_text(shown).width(260.0).show_ui(ui, |ui| {
                for (name, _, _) in COUNTRIES {
                    let n = st.counts.get(&name.to_lowercase()).copied();
                    if have_users && n.is_none() {
                        continue;
                    }
                    let label = n.map_or_else(|| (*name).to_string(), |n| format!("{name} ({n})"));
                    if ui.selectable_label(st.country == *name, label).clicked() {
                        st.country = (*name).to_string();
                        st.states.clear();
                    }
                }
            });
            combo.response.labelled_by(lbl.id);
            let ok = COUNTRIES.iter().any(|(n, _, _)| *n == st.country);
            if ui.add_enabled(ok, egui::Button::new("Add Region")).clicked() {
                let country = st.country.clone();
                if st.states.is_empty() {
                    st.regions.retain(|(c, _)| *c != country);
                    st.regions.push((country, None));
                } else {
                    let whole = st.regions.iter().any(|(c, s)| *c == country && s.is_none());
                    if !whole {
                        for s in std::mem::take(&mut st.states) {
                            if !st.regions.iter().any(|(c, x)| *c == country && x.as_deref() == Some(s.as_str())) {
                                st.regions.push((country.clone(), Some(s)));
                            }
                        }
                    }
                    st.states.clear();
                }
            }
        });
        if let Some((_, _, states)) = COUNTRIES.iter().find(|(n, _, _)| *n == st.country) {
            if !states.is_empty() {
                ui.label(egui::RichText::new("Check states or provinces to add only those, or none for the whole country.").color(pal.muted));
                ui.horizontal_wrapped(|ui| {
                    for s in *states {
                        let mut on = st.states.contains(*s);
                        if ui.checkbox(&mut on, *s).changed() {
                            if on {
                                st.states.insert((*s).to_string());
                            } else {
                                st.states.remove(*s);
                            }
                        }
                    }
                });
            }
        }
        if st.regions.is_empty() {
            let t = if st.mode == Mode::File { "No regions chosen: every user in the file is included." } else { "Add at least one region to fetch." };
            ui.label(egui::RichText::new(t).color(pal.muted));
        } else {
            let mut remove = None;
            ui.horizontal_wrapped(|ui| {
                for (i, (c, s)) in st.regions.iter().enumerate() {
                    let label = s.as_ref().map_or_else(|| c.clone(), |s| format!("{s}, {c}"));
                    if ui.button(format!("{label}  ×")).on_hover_text("Remove region").clicked() {
                        remove = Some(i);
                    }
                }
            });
            if let Some(i) = remove {
                st.regions.remove(i);
            }
        }

        if st.mode == Mode::Api {
            ui.horizontal(|ui| {
                if st.fetch.is_none() {
                    if ui.add_enabled(!st.regions.is_empty(), theme::primary(ui, "Fetch Users", 14.0)).clicked() {
                        st.message = None;
                        st.fetch = Some(radioid_api::start(st.api_url.clone(), st.key.trim().to_string(), st.regions.clone(), ui.ctx().clone()));
                    }
                } else {
                    if ui.button("Stop").clicked() {
                        if let Some(f) = &st.fetch {
                            f.stop();
                        }
                    }
                    if let Some(f) = &st.fetch {
                        ui.label(f.status.clone());
                    }
                }
            });
        }

        ui.add_space(6.0);
        ui.strong("3  Filters");
        ui.horizontal(|ui| {
            ui.label("Call sign prefixes");
            ui.add(egui::TextEdit::singleline(&mut st.prefixes).hint_text("N4, K4, W4").desired_width(180.0));
            ui.label("DMR ID from");
            ui.add(egui::TextEdit::singleline(&mut st.id_min).hint_text("1").desired_width(110.0));
            ui.label("to");
            ui.add(egui::TextEdit::singleline(&mut st.id_max).hint_text("16777215").desired_width(130.0));
        });

        ui.add_space(6.0);
        ui.strong("4  Text");
        let token_row = |ui: &mut Ui, text: &mut String| {
            ui.horizontal_wrapped(|ui| {
                for tok in radioid::TOKENS {
                    if ui.small_button(format!("{{{tok}}}")).clicked() {
                        if !text.is_empty() && !text.ends_with(' ') {
                            text.push(' ');
                        }
                        text.push_str(&format!("{{{tok}}}"));
                    }
                }
            });
        };
        ui.horizontal(|ui| {
            let lbl = ui.label("ID database text");
            ui.add(egui::TextEdit::singleline(&mut st.template).desired_width(300.0)).labelled_by(lbl.id);
        });
        token_row(ui, &mut st.template);
        ui.horizontal(|ui| {
            let lbl = ui.label("Text length");
            let combo = egui::ComboBox::from_id_salt("rid_len").selected_text(format!("{} characters", st.text_len)).show_ui(ui, |ui| {
                for n in iddb::TEXT_LENGTHS {
                    ui.selectable_value(&mut st.text_len, n, format!("{n} characters"));
                }
            });
            combo.response.labelled_by(lbl.id);
            ui.checkbox(&mut st.upper, "All capitals");
        });
        ui.horizontal(|ui| {
            let lbl = ui.label("Contact name");
            ui.add(egui::TextEdit::singleline(&mut st.contact_template).desired_width(300.0)).labelled_by(lbl.id);
        });
        token_row(ui, &mut st.contact_template);
        ui.label(egui::RichText::new("Used for Add as Contacts. Contact names hold 16 characters.").color(pal.muted));

        // Preview, summary and the two actions
        let total = st.users.as_ref().map_or(0, |u| u.len());
        let end = facts.end.unwrap_or(usize::MAX);
        let key = format!(
            "{:?}|{}|{}|{}|{}|{}|{}|{}|{}|{end}",
            st.regions, st.prefixes, st.id_min, st.id_max, st.template, st.contact_template, st.upper, st.text_len, format_of(facts)
        );
        if st.cache.as_ref().map(|c| &c.0) != Some(&key) {
            let job = Job {
                users: st.users.clone().unwrap_or_default(),
                selection: st.selection(),
                template: st.template.clone(),
                contact_template: st.contact_template.clone(),
                upper: st.upper,
                text_len: st.text_len,
                format: format_of(facts),
                end,
            };
            if job.users.len() <= SYNC_LIMIT {
                st.cache = Some(compute(key.clone(), &job));
                st.pending = None;
            } else {
                // a thread, once the choices have been still for a moment
                if st.pending.as_ref().map(|p| &p.key) != Some(&key) {
                    st.pending = Some(Pending { key: key.clone(), since: Instant::now(), job: Some(job), rx: None });
                }
                if let Some(p) = &mut st.pending {
                    if p.rx.is_none() {
                        let wait = SETTLE.saturating_sub(p.since.elapsed());
                        if wait.is_zero() {
                            if let Some(job) = p.job.take() {
                                let (tx, rx) = channel();
                                let (k, ctx) = (p.key.clone(), ui.ctx().clone());
                                std::thread::spawn(move || {
                                    let _ = tx.send(compute(k, &job));
                                    ctx.request_repaint();
                                });
                                p.rx = Some(rx);
                            }
                        } else {
                            ui.ctx().request_repaint_after(wait);
                        }
                    }
                    if let Some(c) = p.rx.as_ref().and_then(|rx| rx.try_recv().ok()) {
                        if c.0 == key {
                            st.cache = Some(c);
                        }
                        st.pending = None;
                    }
                }
            }
        }
        // what is shown while a new answer is on its way is the last one, marked
        let updating = st.cache.as_ref().map(|c| &c.0) != Some(&key);
        let (count, preview, size) = st.cache.as_ref().map(|c| (c.1, c.2.clone(), c.3)).unwrap_or((0, Vec::new(), radioid::iddb_size(0, 16, end)));
        ui.add_space(6.0);
        ui.strong("Preview");
        if preview.is_empty() {
            ui.label(egui::RichText::new("No users match yet.").color(pal.muted));
        } else {
            egui::Grid::new("rid_preview").striped(true).show(ui, |ui| {
                ui.strong("DMR ID");
                ui.strong(format!("ID database ({})", st.text_len));
                ui.strong("Contact name");
                ui.end_row();
                for (id, db, contact) in &preview {
                    ui.label(id.to_string());
                    ui.monospace(db);
                    ui.monospace(contact);
                    ui.end_row();
                }
            });
        }
        ui.add_space(6.0);
        if st.users.is_none() {
            ui.label(if st.mode == Mode::File { "Open a RadioID user file to begin." } else { "Add regions and fetch their users to begin." });
        } else {
            let mut t = if updating { format!("Counting… (of {} users)", thousands(total)) } else { format!("{} of {} users selected.", thousands(count), thousands(total)) };
            if count > 0 && !updating {
                t += &format!(" ID database size: {} KB.", size.bytes.div_ceil(1024));
                if size.fit != usize::MAX && count > size.fit {
                    t += &format!(" This radio holds only {} at this text length; the rest would be left out. Use a shorter text length or fewer regions.", thousands(size.fit));
                }
            }
            ui.label(t);
        }
        ui.horizontal(|ui| {
            let ready = count > 0 && !updating;
            let write = ui.add_enabled(ready, theme::primary(ui, "Write ID Database", 14.0));
            if std::mem::take(&mut st.scroll_to_actions) {
                write.scroll_to_me(Some(egui::Align::Center));
            }
            if write.clicked() {
                if doc.cp.iddb.is_some() {
                    st.confirm = Some(Confirm::Replace);
                } else {
                    st.write_db(doc, facts);
                }
            }
            if ui.add_enabled(ready, egui::Button::new("Add as Contacts")).clicked() {
                st.add_contacts(doc);
            }
        });
        if let Some((m, bad)) = &st.message {
            ui.colored_label(if *bad { pal.warn } else { pal.ok }, m);
        }
    });

    st.save_choices();

    if let Some(c) = st.confirm {
        let mut go = false;
        let mut cancel = false;
        let m = egui::Modal::new(egui::Id::unique("rid_confirm")).show(ui.ctx(), |ui| {
            ui.set_width(420.0);
            let n = doc.cp.iddb.as_ref().map_or(0, |h| h.count);
            match c {
                Confirm::Replace => ui.label(format!("This replaces the current ID database of {n} record{}.", if n == 1 { "" } else { "s" })),
                Confirm::Remove => ui.label("Remove the ID database? The radio will then show names only for DMR IDs that are contacts."),
            };
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let label = if c == Confirm::Replace { "Replace" } else { "Remove" };
                if ui.button(label).clicked() {
                    go = true;
                }
                if ui.button("Cancel").clicked() {
                    cancel = true;
                }
            });
        });
        if cancel || m.should_close() {
            st.confirm = None;
        } else if go {
            st.confirm = None;
            match c {
                Confirm::Replace => st.write_db(doc, facts),
                Confirm::Remove => {
                    doc.change_iddb(iddb::erase);
                    st.message = Some(("The ID database was removed. Write to Radio sends that.".into(), false));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn choices_survive_a_round_trip_and_a_bad_file_gives_the_defaults() {
        let p = Prefs {
            api: true,
            regions: vec![("United States".into(), Some("Ohio".into())), ("Canada".into(), None)],
            prefixes: "N4, K4".into(),
            id_min: "100".into(),
            id_max: "".into(),
            template: "{call} {name}".into(),
            contact_template: "{call}".into(),
            upper: true,
            text_len: 24,
        };
        assert_eq!(Prefs::from_json(&p.to_json()), p);
        let d = Prefs::from_json("this is not json");
        assert_eq!((d.api, d.regions.len(), d.text_len, d.template.as_str()), (false, 0, 32, DEFAULT_TEMPLATE));
        // wrong kinds and an odd text length fall back one field at a time
        let d = Prefs::from_json(r#"{"mode":"api","textLen":20,"upper":"yes","regions":[{"country":3},{"country":"Peru"}],"prefixes":5}"#);
        assert_eq!((d.api, d.text_len, d.upper, d.prefixes.as_str()), (true, 32, false, ""));
        assert_eq!(d.regions, vec![("Peru".to_string(), None)]);
    }

    #[test]
    fn the_file_is_written_on_a_change_and_read_back_and_holds_no_key() {
        let dir = std::env::temp_dir().join(format!("rfcps-prefs-{}", std::process::id()));
        let path = dir.join("settings.json");
        let _ = std::fs::remove_dir_all(&dir);
        let mut a = IdDbUi::default();
        a.keep_choices_in(path.clone());
        assert!(!path.exists(), "nothing is written until something changes");
        a.save_choices();
        assert!(!path.exists());
        a.prefixes = "W1".into();
        a.upper = true;
        a.key = "secret-key".into();
        a.save_choices();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("W1") && !text.contains("secret-key"));
        let mut b = IdDbUi::default();
        b.keep_choices_in(path.clone());
        assert_eq!((b.prefixes.as_str(), b.upper, b.key.as_str()), ("W1", true, ""));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn counts_get_thousands_separators() {
        assert_eq!([thousands(0), thousands(999), thousands(1000), thousands(317_956), thousands(16_777_215)], ["0", "999", "1,000", "317,956", "16,777,215"]);
    }
}
