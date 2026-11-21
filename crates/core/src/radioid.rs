// SPDX-License-Identifier: MIT
// Copyright (c) 2026 The RadioFoundry Authors
// Copyright (c) 2026 insideTheFlux
//
// RadioID.net, the DMR ID registry, as a contacts source: users from its `user.csv` (downloaded by the user). Users
// come out as `User`; the editor picks regions, filters them, and formats each user's text for the DMR ID database or
// a contact name. Ported from RadioFoundry's `site/static/js/codeplug/sources/radioid.js` and `output.js`.

use crate::iddb::{self, Record};
use crate::regions::{ABBREV, COUNTRIES};
use crate::records::{CallType, Contact, MAX_CONTACTS};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct User {
    pub id: u32,
    pub call: String,
    pub name: String,
    pub last: String,
    pub city: String,
    pub state: String,
    pub country: String,
}

/// The address RadioID publishes `user.csv` at (the page links to it; this editor does not download it).
pub const DOWNLOAD: &str = "https://radioid.net/static/user.csv";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Col {
    Id,
    Call,
    Name,
    Last,
    City,
    State,
    Country,
}

fn column(header: &str) -> Option<Col> {
    Some(match header.trim().to_lowercase().as_str() {
        "radio_id" | "radioid" | "id" | "dmr_id" | "dmrid" | "dmr id" | "radio id" => Col::Id,
        "callsign" | "call" | "call_sign" | "call sign" => Col::Call,
        "first_name" | "fname" | "firstname" | "first name" | "name" => Col::Name,
        "last_name" | "surname" | "lastname" | "last name" => Col::Last,
        "city" => Col::City,
        "state" | "province" => Col::State,
        "country" => Col::Country,
        _ => return None,
    })
}

const NO_HEADER: [Option<Col>; 7] = [
    Some(Col::Id),
    Some(Col::Call),
    Some(Col::Name),
    Some(Col::Last),
    Some(Col::City),
    Some(Col::State),
    Some(Col::Country),
];

fn all_digits(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

/// One CSV line (`user.csv` has no fields with line breaks).
fn split_line(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let cs: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < cs.len() {
        let c = cs[i];
        if quoted {
            if c == '"' && cs.get(i + 1) == Some(&'"') {
                field.push('"');
                i += 1;
            } else if c == '"' {
                quoted = false;
            } else {
                field.push(c);
            }
        } else if c == '"' && field.is_empty() {
            quoted = true;
        } else if c == ',' {
            out.push(std::mem::take(&mut field));
        } else {
            field.push(c);
        }
        i += 1;
    }
    out.push(field);
    out
}

/// Users from RadioID's `user.csv` (or a file with the same columns, with or without a header row), sorted by ID,
/// one per ID. Lines that aren't a user (no numeric ID or no call sign) are skipped.
pub fn parse_users(text: &str) -> Vec<User> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let lines: Vec<&str> = text.split('\n').map(|l| l.strip_suffix('\r').unwrap_or(l)).collect();
    let mut cols: Vec<Option<Col>> = NO_HEADER.to_vec();
    let mut start = 0;
    let first = split_line(lines.first().copied().unwrap_or(""));
    if !all_digits(first[0].trim()) {
        let mapped: Vec<Option<Col>> = first.iter().map(|h| column(h)).collect();
        if mapped.contains(&Some(Col::Id)) && mapped.contains(&Some(Col::Call)) {
            cols = mapped;
            start = 1;
        }
    }
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for line in &lines[start..] {
        if line.is_empty() {
            continue;
        }
        let f = split_line(line);
        let mut u = User::default();
        let mut id = String::new();
        for (j, k) in cols.iter().enumerate() {
            let (Some(k), Some(v)) = (k, f.get(j)) else { continue };
            let v = v.trim().to_string();
            match k {
                Col::Id => id = v,
                Col::Call => u.call = v,
                Col::Name => u.name = v,
                Col::Last => u.last = v,
                Col::City => u.city = v,
                Col::State => u.state = v,
                Col::Country => u.country = v,
            }
        }
        let n = if all_digits(&id) { id.parse::<u64>().unwrap_or(0) } else { 0 };
        if n == 0 || n > 0xFF_FFFF || u.call.is_empty() || !seen.insert(n) {
            continue;
        }
        u.id = n as u32;
        u.call = u.call.to_uppercase();
        out.push(u);
    }
    out.sort_by_key(|u| u.id);
    out
}

// --- Regions and filters ---

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Region {
    pub country: String,
    pub count: usize,
    /// (state, users), by name.
    pub states: Vec<(String, usize)>,
}

/// The countries of a set of users with their states, most users first.
pub fn regions_of(users: &[User]) -> Vec<Region> {
    let mut order: Vec<String> = Vec::new();
    let mut map: HashMap<String, (usize, HashMap<String, usize>)> = HashMap::new();
    for u in users {
        let e = map.entry(u.country.clone()).or_insert_with(|| {
            order.push(u.country.clone());
            (0, HashMap::new())
        });
        e.0 += 1;
        *e.1.entry(u.state.clone()).or_insert(0) += 1;
    }
    let mut out: Vec<Region> = order
        .into_iter()
        .map(|c| {
            let (count, st) = &map[&c];
            let mut states: Vec<(String, usize)> = st.iter().filter(|(s, _)| !s.is_empty()).map(|(s, n)| (s.clone(), *n)).collect();
            states.sort_by_key(|(s, _)| s.to_lowercase());
            Region { country: c, count: *count, states }
        })
        .collect();
    out.sort_by_key(|r| std::cmp::Reverse(r.count));
    out
}

fn key(s: &str) -> String {
    s.trim().to_lowercase()
}

#[derive(Clone, Debug)]
pub struct Selection {
    /// (country, state): a country alone is the whole country; none at all is every user.
    pub regions: Vec<(String, Option<String>)>,
    /// Call sign prefixes; none is every call sign.
    pub prefixes: Vec<String>,
    pub id_min: u32,
    pub id_max: u32,
}

impl Default for Selection {
    fn default() -> Self {
        Self { regions: Vec::new(), prefixes: Vec::new(), id_min: 0, id_max: 0xFF_FFFF }
    }
}

/// The users in the regions, with call signs starting with one of the prefixes, in the ID range.
pub fn select<'a>(users: &'a [User], sel: &Selection) -> Vec<&'a User> {
    let mut whole = HashSet::new();
    let mut part = HashSet::new();
    for (c, s) in &sel.regions {
        match s {
            Some(s) if !s.is_empty() => part.insert(format!("{}\n{}", key(c), key(s))),
            _ => whole.insert(key(c)),
        };
    }
    let pre: Vec<String> = sel.prefixes.iter().map(|p| p.trim().to_uppercase()).filter(|p| !p.is_empty()).collect();
    users
        .iter()
        .filter(|u| {
            if u.id < sel.id_min || u.id > sel.id_max {
                return false;
            }
            if !pre.is_empty() && !pre.iter().any(|p| u.call.starts_with(p.as_str())) {
                return false;
            }
            if sel.regions.is_empty() {
                return true;
            }
            let c = key(&u.country);
            whole.contains(&c) || part.contains(&format!("{c}\n{}", key(&u.state)))
        })
        .collect()
}

// --- Text ---

pub const TOKENS: [&str; 10] = ["call", "name", "first", "last", "city", "state", "st", "country", "cc", "id"];

fn value(u: &User, token: &str) -> String {
    let name = || u.name.split_whitespace().collect::<Vec<_>>().join(" ");
    match token {
        "call" => u.call.clone(),
        "name" => name(),
        "first" => name().split(' ').next().unwrap_or("").to_string(),
        "last" => u.last.clone(),
        "city" => u.city.clone(),
        "state" => u.state.clone(),
        "st" => {
            let (c, s) = (key(&u.country), key(&u.state));
            ABBREV
                .iter()
                .filter(|(country, _)| key(country) == c)
                .flat_map(|(_, m)| m.iter())
                .find(|(state, _)| key(state) == s)
                .map_or_else(|| u.state.clone(), |(_, a)| (*a).to_string())
        }
        "country" => u.country.clone(),
        "cc" => {
            let c = key(&u.country);
            COUNTRIES.iter().rev().find(|(n, _, _)| key(n) == c).map_or_else(|| u.country.clone(), |(_, cc, _)| (*cc).to_string())
        }
        "id" => u.id.to_string(),
        _ => String::new(),
    }
}

/// `template` split at its `{token}`s: literal, token, literal, ... literal.
fn split_template(t: &str) -> Vec<String> {
    let cs: Vec<char> = t.chars().collect();
    let mut segs = Vec::new();
    let mut cur = String::new();
    let mut i = 0;
    while i < cs.len() {
        if cs[i] == '{' {
            let mut j = i + 1;
            while j < cs.len() && cs[j].is_ascii_lowercase() {
                j += 1;
            }
            if j > i + 1 && cs.get(j) == Some(&'}') {
                segs.push(std::mem::take(&mut cur));
                segs.push(cs[i..=j].iter().collect());
                i = j + 1;
                continue;
            }
        }
        cur.push(cs[i]);
        i += 1;
    }
    segs.push(cur);
    segs
}

/// The template with each `{token}` replaced. An empty value drops out with the text just before it, so
/// `"{call}, {city} {st}"` gives `"Q0TST OH"` for a user without a city. The text before the first token and after the
/// last stay when anything is shown. `upper`: all capitals; then cut to `max` characters (0: no cut).
pub fn format(u: &User, template: &str, upper: bool, max: usize) -> String {
    let segs = split_template(template);
    let mut out = String::new();
    let mut shown = false;
    let mut i = 1;
    while i < segs.len() {
        let name = &segs[i][1..segs[i].len() - 1];
        let val = if TOKENS.contains(&name) { value(u, name) } else { segs[i].clone() };
        if !val.is_empty() {
            if shown {
                out.push_str(&segs[i - 1]);
            }
            out.push_str(&val);
            shown = true;
        }
        i += 2;
    }
    if shown {
        let last = segs.len() - 2;
        let tail = if last > 0 {
            let n = &segs[last][1..segs[last].len() - 1];
            !TOKENS.contains(&n) || !value(u, n).is_empty()
        } else {
            false
        };
        out = format!("{}{}{}", segs[0], out, if tail { segs[segs.len() - 1].as_str() } else { "" });
    } else if segs.len() == 1 {
        out = segs[0].clone();
    }
    let mut out = out.split_whitespace().collect::<Vec<_>>().join(" ");
    if upper {
        out = out.to_uppercase();
    }
    if max > 0 {
        out = out.chars().take(max).collect::<String>().trim().to_string();
    }
    out
}

// --- The ID database from users ---

/// The records a database of these users holds.
pub fn iddb_records(users: &[&User], template: &str, upper: bool, text_len: usize) -> Vec<Record> {
    users.iter().map(|u| Record { id: u.id, text: format(u, template, upper, text_len) }).collect()
}

/// The room a database of format N takes: bytes and how many of `count` records fit before `end`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Size {
    pub rec: usize,
    pub bytes: usize,
    pub fit: usize,
}

pub fn iddb_size(count: usize, text_len: usize, end: usize) -> Size {
    let rec = iddb::record_size(text_len);
    let fit = iddb::capacity(rec, end);
    Size { rec, bytes: iddb::HEADER + count.min(fit) * rec, fit }
}

// --- Private-call contacts from users ---

/// Names are stored in Latin-1: other characters lose their accents (Ł to L) or are dropped.
fn latin1(s: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    let mut out = String::new();
    for c in s.chars() {
        if (c as u32) < 0x100 {
            out.push(c);
            continue;
        }
        match iddb::fold(c) {
            Some(f) => out.push_str(f),
            None => out.extend(c.to_string().nfd().filter(|d| (*d as u32) < 0x100)),
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Added {
    pub added: usize,
    pub existing: usize,
    pub full: usize,
}

/// Adds users as private-call contacts. A user whose ID is already a private contact is skipped; when the contact list
/// fills, the rest are left out. A new contact takes the lowest free number.
pub fn add_contacts(contacts: &mut Vec<Contact>, users: &[&User], template: &str, upper: bool) -> Added {
    let mut have: HashSet<u32> = contacts.iter().filter(|c| c.call_type == CallType::Private).map(|c| c.id).collect();
    let mut taken: HashSet<u16> = contacts.iter().map(|c| c.number).collect();
    let mut next = 1u16;
    let mut r = Added::default();
    for u in users {
        if have.contains(&u.id) {
            r.existing += 1;
            continue;
        }
        if contacts.len() >= MAX_CONTACTS {
            r.full += 1;
            continue;
        }
        let name: String = latin1(&format(u, template, upper, 0)).chars().take(16).collect::<String>().trim().to_string();
        let name = if name.is_empty() { u.call.clone() } else { name };
        while taken.contains(&next) {
            next += 1;
        }
        taken.insert(next);
        contacts.push(Contact { number: next, name, id: u.id, call_type: CallType::Private, timeslot: 0 });
        have.insert(u.id);
        r.added += 1;
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    const CSV: &str = "RADIO_ID,CALLSIGN,FIRST_NAME,LAST_NAME,CITY,STATE,COUNTRY\n\
        3100002,k6abc,Ann,Smith,San Jose,California,United States\n\
        3100001,W1AW,,,,,United States\n\
        3100001,DUP,x,,,,\n\
        abc,BAD,x,,,,\n";

    #[test]
    fn users_come_out_sorted_and_one_per_id() {
        let u = parse_users(CSV);
        assert_eq!(u.iter().map(|u| (u.id, u.call.as_str())).collect::<Vec<_>>(), [(3100001, "W1AW"), (3100002, "K6ABC")]);
    }

    #[test]
    fn a_missing_value_drops_out_with_the_text_before_it() {
        let u = parse_users(CSV);
        assert_eq!(format(&u[1], "{call}, {city} {st}", false, 0), "K6ABC, San Jose CA");
        assert_eq!(format(&u[0], "{call}, {city} {st}", false, 0), "W1AW");
        assert_eq!(format(&u[1], "{call} {first} ({cc})", true, 12), "K6ABC ANN (U");
    }

    #[test]
    fn selection_by_region_and_prefix() {
        let u = parse_users(CSV);
        let sel = Selection { regions: vec![("united states".into(), Some("CALIFORNIA".into()))], ..Selection::default() };
        assert_eq!(select(&u, &sel).len(), 1);
        let sel = Selection { prefixes: vec!["w1".into()], ..Selection::default() };
        assert_eq!(select(&u, &sel)[0].call, "W1AW");
    }

    #[test]
    fn the_database_size_and_room() {
        let s = iddb_size(10, 16, usize::MAX);
        assert_eq!((s.rec, s.bytes), (15, 12 + 150));
    }
}
