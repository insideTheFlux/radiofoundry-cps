// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux

//! The ID database tab: names for DMR IDs that are not contacts, built from RadioID's `user.csv` (the web editor's
//! "Build from RadioID.net", file mode). The database lives in the document's image; Write to Radio sends it.

use crate::doc::Doc;
use crate::theme;
use eframe::egui::{self, Ui};
use rfcps_core::iddb;
use rfcps_core::radioid::{self, Selection, User};
use rfcps_core::regions::COUNTRIES;
use std::collections::{BTreeSet, HashMap};

/// The inputs as one string, how many users they select, and the first few as (ID, database text, contact name).
type Cached = (String, usize, Vec<(u32, String, String)>);

pub struct IdDbUi {
    users: Option<Vec<User>>,
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
            template: "{call} {first} {city} {st}".into(),
            contact_template: "{call} {first}".into(),
            upper: false,
            text_len: 32,
            message: None,
            confirm: None,
            counts: HashMap::new(),
            cache: None,
        }
    }
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
            regions: self.regions.clone(),
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

    /// The users of a `user.csv`'s text, from the file called `name`.
    pub fn load_users(&mut self, text: &str, name: String) {
        let users = radioid::parse_users(text);
        if users.is_empty() {
            self.message = Some((format!("{name} holds no DMR users. Use the user.csv file from RadioID.net."), true));
        } else {
            self.counts = radioid::regions_of(&users).into_iter().map(|r| (r.country.to_lowercase(), r.count)).collect();
            self.users = Some(users);
            self.cache = None;
            self.source = name;
            self.message = None;
        }
    }

    fn write_db(&mut self, doc: &mut Doc) {
        let Some(users) = &self.users else { return };
        let sel = self.selected(users);
        let recs = radioid::iddb_records(&sel, &self.template, self.upper, self.text_len);
        let len = self.text_len;
        match doc.change_iddb(|img| iddb::write_n(img, &recs, len, usize::MAX)) {
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

pub fn show(ui: &mut Ui, doc: &mut Doc, st: &mut IdDbUi) {
    let pal = theme::palette(ui);
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        ui.set_max_width(760.0);
        // The current database
        match &doc.cp.iddb {
            Some(h) => {
                let n = h.count;
                let what = if h.format == b'C' { "a compact DMR ID database" } else { "a DMR ID database" };
                ui.label(format!("The codeplug holds {what} of {n} record{}, {} characters each.", if n == 1 { "" } else { "s" }, h.text_len));
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
            if ui.button("Open user.csv").clicked() {
                st.open_file();
            }
            ui.hyperlink_to("Download user.csv from RadioID.net", radioid::DOWNLOAD);
            if let Some(u) = &st.users {
                ui.label(egui::RichText::new(format!("{} users from {}.", u.len(), st.source)).color(pal.muted));
            }
        });

        ui.add_space(6.0);
        ui.strong("2  Regions");
        let have_users = st.users.is_some();
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
            ui.label(egui::RichText::new("No regions chosen: every user in the file is included.").color(pal.muted));
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

        ui.add_space(6.0);
        ui.strong("3  Filters");
        ui.horizontal(|ui| {
            ui.label("Call sign prefixes");
            ui.add(egui::TextEdit::singleline(&mut st.prefixes).hint_text("N4, K4, W4").desired_width(180.0));
            ui.label("DMR ID from");
            ui.add(egui::TextEdit::singleline(&mut st.id_min).hint_text("1").desired_width(90.0));
            ui.label("to");
            ui.add(egui::TextEdit::singleline(&mut st.id_max).hint_text("16777215").desired_width(90.0));
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
        let total = st.users.as_ref().map_or(0, Vec::len);
        let key = format!(
            "{:?}|{}|{}|{}|{}|{}|{}|{}",
            st.regions, st.prefixes, st.id_min, st.id_max, st.template, st.contact_template, st.upper, st.text_len
        );
        if st.cache.as_ref().map(|c| &c.0) != Some(&key) {
            let (count, preview) = match &st.users {
                Some(users) => {
                    let sel = st.selected(users);
                    let prev = sel
                        .iter()
                        .take(6)
                        .map(|u| {
                            (
                                u.id,
                                radioid::format(u, &st.template, st.upper, st.text_len),
                                radioid::format(u, &st.contact_template, st.upper, 16),
                            )
                        })
                        .collect();
                    (sel.len(), prev)
                }
                None => (0, Vec::new()),
            };
            st.cache = Some((key, count, preview));
        }
        let (count, preview) = st.cache.as_ref().map(|c| (c.1, c.2.clone())).unwrap_or_default();
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
            ui.label("Open a RadioID user file to begin.");
        } else {
            let size = radioid::iddb_size(count, st.text_len, usize::MAX);
            let mut t = format!("{count} of {total} users selected.");
            if count > 0 {
                t += &format!(" ID database size: {} KB.", size.bytes.div_ceil(1024));
            }
            ui.label(t);
        }
        ui.horizontal(|ui| {
            if ui.add_enabled(count > 0, theme::primary(ui, "Write ID Database", 14.0)).clicked() {
                if doc.cp.iddb.is_some() {
                    st.confirm = Some(Confirm::Replace);
                } else {
                    st.write_db(doc);
                }
            }
            if ui.add_enabled(count > 0, egui::Button::new("Add as Contacts")).clicked() {
                st.add_contacts(doc);
            }
        });
        if let Some((m, bad)) = &st.message {
            ui.colored_label(if *bad { pal.warn } else { pal.ok }, m);
        }
    });

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
                Confirm::Replace => st.write_db(doc),
                Confirm::Remove => {
                    doc.change_iddb(iddb::erase);
                    st.message = Some(("The ID database was removed. Write to Radio sends that.".into(), false));
                }
            }
        }
    }
}
