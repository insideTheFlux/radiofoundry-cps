// SPDX-License-Identifier: MIT
// Copyright (c) 2026 The RadioFoundry Authors
// Copyright (c) 2026 insideTheFlux

//! RadioID.net's public API as a source of users, a few regions at a time (the web editor's "Fetch from RadioID.net",
//! `site/static/js/codeplug/sources/radioid.js`). Pages come 200 users at a time, one request after another, as RadioID
//! asks of clients. The person's own API key (optional) goes only to RadioID.net, in `X-API-Token`. A native program
//! has no browser cross-origin rule, so every request goes straight to RadioID.net.

use eframe::egui;
use rfcps_core::radioid::User;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::time::Duration;

pub const API: &str = "https://database.radioid.net/api/dmr/user/";
pub const PER_PAGE: usize = 200;
/// Above this many pages a selection is better served by the user file.
pub const MAX_PAGES: usize = 40;
/// Between two requests.
const GAP: Duration = Duration::from_millis(300);

// --- The reply ---

/// JavaScript's `Number(x) || 0` of a JSON value.
fn number(v: &Value) -> f64 {
    let n = match v {
        Value::Number(n) => n.as_f64().unwrap_or(0.0),
        Value::String(s) => {
            let t = s.trim();
            let radix = |p: &[&str], r: u32| p.iter().find_map(|p| t.strip_prefix(p)).and_then(|d| u64::from_str_radix(d, r).ok()).map(|v| v as f64);
            if t.is_empty() {
                0.0
            } else {
                // JavaScript reads "0x10", "0o7" and "0b1" too
                radix(&["0x", "0X"], 16).or_else(|| radix(&["0o", "0O"], 8)).or_else(|| radix(&["0b", "0B"], 2)).unwrap_or_else(|| t.parse::<f64>().unwrap_or(0.0))
            }
        }
        Value::Bool(true) => 1.0,
        _ => 0.0,
    };
    if n.is_nan() {
        0.0
    } else {
        n
    }
}

/// JavaScript's `String(x || "")`.
fn text(v: Option<&Value>) -> String {
    match v {
        None | Some(Value::Null) | Some(Value::Bool(false)) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) if n.as_f64() == Some(0.0) => String::new(),
        Some(Value::Number(n)) => n.to_string(),
        Some(other) => other.to_string(),
    }
}

/// The users of one page's `results` (the web editor's `fromApi`): the ID from `id`, else `radio_id`; a user without a
/// call sign or with an ID outside 1 to 16777215 is left out.
pub fn from_api(results: &Value) -> Vec<User> {
    let Some(list) = results.as_array() else { return Vec::new() };
    list.iter()
        .filter_map(|r| {
            let id = number(match r.get("id") {
                Some(v) if !v.is_null() => v,
                _ => r.get("radio_id").unwrap_or(&Value::Null),
            });
            let call = text(r.get("callsign")).to_uppercase();
            // (a fractional ID, which the web editor would keep, is no ID)
            if !(1.0..=16_777_215.0).contains(&id) || id.fract() != 0.0 || call.is_empty() {
                return None;
            }
            let t = |k: &str| text(r.get(k)).trim().to_string();
            Some(User { id: id as u32, call, name: t("fname"), last: t("surname"), city: t("city"), state: t("state"), country: t("country") })
        })
        .collect()
}

// --- Requests ---

/// One page of a region: `{count, pages, results}`.
struct Page {
    count: usize,
    pages: usize,
    users: Vec<User>,
}

struct Client {
    base: String,
    key: String,
    agent: ureq::Agent,
    cache: HashMap<(String, Option<String>, usize), Page>,
}

impl Client {
    fn new(base: &str, key: &str) -> Self {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_connect(Some(Duration::from_secs(20)))
            .timeout_recv_body(Some(Duration::from_secs(45)))
            .user_agent("RadioFoundry-CPS")
            .build()
            .into();
        Self { base: base.to_string(), key: key.to_string(), agent, cache: HashMap::new() }
    }

    fn get(&self, country: &str, state: Option<&str>, n: usize) -> Result<Page, String> {
        let mut req = self.agent.get(&self.base).query("country", country);
        if let Some(s) = state {
            req = req.query("state", s);
        }
        req = req.query("page", n.to_string()).header("Accept", "application/json");
        if !self.key.is_empty() {
            req = req.header("X-API-Token", &self.key);
        }
        let mut resp = req.call().map_err(|e| e.to_string())?;
        let body = resp.body_mut().read_to_string().map_err(|e| e.to_string())?;
        let v: Value = serde_json::from_str(&body).map_err(|e| format!("not JSON: {e}"))?;
        let results = v.get("results").filter(|r| r.is_array()).ok_or("unexpected reply")?;
        let users = from_api(results);
        let size = results.as_array().map_or(0, Vec::len);
        Ok(Page {
            count: v.get("count").and_then(Value::as_u64).map_or(size, |c| c as usize),
            pages: v.get("pages").and_then(Value::as_u64).map_or(1, |c| c as usize),
            users,
        })
    }

    /// A page, from the cache or the network; one more try after a pause when a request fails.
    fn page(&mut self, region: &(String, Option<String>), n: usize, stop: &AtomicBool) -> Result<&Page, String> {
        let key = (region.0.clone(), region.1.clone(), n);
        if !self.cache.contains_key(&key) {
            let mut last = String::new();
            let mut got = None;
            for pass in 0..2 {
                if pass > 0 {
                    sleep(Duration::from_secs(2), stop)?;
                }
                match self.get(&region.0, region.1.as_deref(), n) {
                    Ok(p) => {
                        got = Some(p);
                        break;
                    }
                    Err(e) => last = e,
                }
            }
            let p = got.ok_or_else(|| format!("RadioID.net could not be reached ({last})"))?;
            self.cache.insert(key.clone(), p);
        }
        Ok(&self.cache[&key])
    }
}

fn sleep(d: Duration, stop: &AtomicBool) -> Result<(), String> {
    let end = std::time::Instant::now() + d;
    while std::time::Instant::now() < end {
        if stop.load(Ordering::Relaxed) {
            return Err("stopped".into());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    Ok(())
}

/// What a fetch reports while it runs.
pub enum Msg {
    /// Counting the users of region `i` of `n`.
    Counting(usize, usize),
    /// Request `done` of `pages`.
    Progress(usize, usize),
    /// The users (one per ID, by ID), or why not. "stopped" when the person stopped it.
    Done(Result<Vec<User>, String>),
}

/// A fetch in progress.
pub struct Fetch {
    pub rx: Receiver<Msg>,
    stop: Arc<AtomicBool>,
    /// The last progress: (done, of) pages, or the regions counted.
    pub status: String,
}

impl Fetch {
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// Fetches every user of `regions` (a country, or one of its states) on a new thread.
pub fn start(base: String, key: String, regions: Vec<(String, Option<String>)>, ctx: egui::Context) -> Fetch {
    let (tx, rx) = channel();
    let stop = Arc::new(AtomicBool::new(false));
    let flag = stop.clone();
    std::thread::spawn(move || {
        let r = fetch(&base, &key, &regions, &flag, &mut |m| {
            let _ = tx.send(m);
            ctx.request_repaint();
        });
        let _ = tx.send(Msg::Done(r));
        ctx.request_repaint();
    });
    Fetch { rx, stop, status: "Starting…".into() }
}

fn fetch(base: &str, key: &str, regions: &[(String, Option<String>)], stop: &AtomicBool, say: &mut dyn FnMut(Msg)) -> Result<Vec<User>, String> {
    let mut c = Client::new(base, key);
    // sizes first, so a selection too big for the API is caught before the long part
    let (mut pages, mut count) = (0, 0);
    for (i, r) in regions.iter().enumerate() {
        say(Msg::Counting(i, regions.len()));
        if stop.load(Ordering::Relaxed) {
            return Err("stopped".into());
        }
        let p = c.page(r, 1, stop)?;
        pages += p.pages;
        count += p.count;
    }
    if pages > MAX_PAGES {
        return Err(format!(
            "The chosen regions hold {count} users, more than fetching suits (about {}). Choose fewer regions, or use the user file.",
            MAX_PAGES * PER_PAGE
        ));
    }
    let mut users: Vec<User> = Vec::new();
    let mut done = 0;
    for r in regions {
        let n_pages = c.page(r, 1, stop)?.pages;
        users.extend(c.page(r, 1, stop)?.users.clone());
        done += 1;
        say(Msg::Progress(done, pages));
        for n in 2..=n_pages {
            sleep(GAP, stop)?;
            users.extend(c.page(r, n, stop)?.users.clone());
            done += 1;
            say(Msg::Progress(done, pages));
        }
    }
    let mut seen = std::collections::HashSet::new();
    users.retain(|u| seen.insert(u.id));
    users.sort_by_key(|u| u.id);
    Ok(users)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::Mutex;

    /// A server on this computer that answers `respond(path_and_query, headers)` for each request until dropped; the
    /// requests seen are kept.
    pub(crate) fn serve(respond: impl Fn(&str, &str) -> (u16, String) + Send + 'static) -> (String, Arc<Mutex<Vec<String>>>) {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let log = seen.clone();
        std::thread::spawn(move || {
            for s in l.incoming() {
                let Ok(mut s) = s else { break };
                let mut buf = [0u8; 4096];
                let n = s.read(&mut buf).unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]).to_string();
                let path = req.lines().next().unwrap_or("").split(' ').nth(1).unwrap_or("").to_string();
                log.lock().unwrap().push(req.clone());
                let (code, body) = respond(&path, &req);
                let _ = s.write_all(format!("HTTP/1.1 {code} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes());
            }
        });
        (format!("http://{addr}/api/dmr/user/"), seen)
    }

    fn user_json(id: u32, call: &str) -> String {
        format!(r#"{{"id":{id},"callsign":"{call}","fname":" Ann ","surname":"Smith","city":"X","state":"Ohio","country":"United States"}}"#)
    }

    /// A region of `n` users in pages of 2, ids from `base`.
    fn pages_of(n: u32, base: u32, path: &str) -> (u16, String) {
        let page: u32 = path.split("page=").nth(1).and_then(|p| p.split('&').next()).and_then(|p| p.parse().ok()).unwrap_or(1);
        let pages = n.div_ceil(2);
        let rows: Vec<String> = ((page - 1) * 2..(page * 2).min(n)).map(|i| user_json(base + i, &format!("K{}AA", i % 10))).collect();
        (200, format!(r#"{{"count":{n},"pages":{pages},"results":[{}]}}"#, rows.join(",")))
    }

    #[test]
    fn pages_are_followed_and_users_come_back_by_id_once() {
        let (url, seen) = serve(|p, _| if p.contains("state=Ohio") { pages_of(5, 100, p) } else { pages_of(3, 102, p) });
        let stop = AtomicBool::new(false);
        let regions = vec![("United States".to_string(), Some("Ohio".to_string())), ("Canada".to_string(), None)];
        let mut said = Vec::new();
        let users = fetch(&url, "", &regions, &stop, &mut |m| {
            if let Msg::Progress(d, t) = m {
                said.push((d, t));
            }
        })
        .unwrap();
        // 5 users from 100 and 3 from 102: ids 100..104 and 102..104, one each
        assert_eq!(users.iter().map(|u| u.id).collect::<Vec<_>>(), vec![100, 101, 102, 103, 104]);
        assert_eq!(said.last(), Some(&(5, 5)), "3 pages of Ohio, 2 of Canada");
        let log = seen.lock().unwrap();
        // the sizes first (page 1 of each region), then the rest, page 1 not asked twice
        assert_eq!(log.len(), 5);
        assert!((log[0].contains("country=United%20States") || log[0].contains("country=United+States")) && log[0].contains("state=Ohio") && log[0].contains("page=1"));
        assert!(log[1].contains("country=Canada") && !log[1].contains("state="));
        assert_eq!(users[0].name, "Ann");
    }

    #[test]
    fn the_key_goes_only_in_its_header_and_only_when_given() {
        let (url, seen) = serve(|p, _| pages_of(1, 5, p));
        let stop = AtomicBool::new(false);
        let r = vec![("Canada".to_string(), None)];
        fetch(&url, "secret-key", &r, &stop, &mut |_| {}).unwrap();
        fetch(&url, "", &r, &stop, &mut |_| {}).unwrap();
        let log = seen.lock().unwrap();
        assert!(log[0].to_lowercase().contains("x-api-token: secret-key"));
        assert_eq!(log[0].matches("secret-key").count(), 1);
        assert!(!log[1].to_lowercase().contains("x-api-token"));
        assert!(!log[0].lines().next().unwrap().contains("secret-key"), "never in the address");
    }

    #[test]
    fn a_selection_too_big_for_the_api_is_refused_before_the_long_part() {
        let (url, seen) = serve(|_, _| (200, r#"{"count":50000,"pages":250,"results":[]}"#.into()));
        let stop = AtomicBool::new(false);
        let e = fetch(&url, "", &[("Germany".to_string(), None)], &stop, &mut |_| {}).unwrap_err();
        assert!(e.contains("50000 users") && e.contains("user file"), "{e}");
        assert_eq!(seen.lock().unwrap().len(), 1);
    }

    #[test]
    fn a_failing_server_is_an_error_after_one_more_try_and_a_stop_ends_it() {
        let (url, seen) = serve(|_, _| (500, "no".into()));
        let stop = AtomicBool::new(false);
        let e = fetch(&url, "", &[("Germany".to_string(), None)], &stop, &mut |_| {}).unwrap_err();
        assert!(e.contains("could not be reached"), "{e}");
        assert_eq!(seen.lock().unwrap().len(), 2);
        let stop = AtomicBool::new(true);
        assert_eq!(fetch(&url, "", &[("Germany".to_string(), None)], &stop, &mut |_| {}).unwrap_err(), "stopped");
    }

    #[test]
    fn an_unexpected_reply_is_an_error() {
        let (url, _) = serve(|_, _| (200, r#"{"detail":"nope"}"#.into()));
        let stop = AtomicBool::new(false);
        let e = fetch(&url, "", &[("Germany".to_string(), None)], &stop, &mut |_| {}).unwrap_err();
        assert!(e.contains("unexpected reply"), "{e}");
    }

    /// Needs the network: `cargo test -p rfcps radioid_net_api -- --ignored`. One small country.
    #[test]
    #[ignore]
    fn radioid_net_api_answers_for_a_small_country() {
        let stop = AtomicBool::new(false);
        let users = fetch(API, "", &[("Aruba".to_string(), None)], &stop, &mut |_| {}).unwrap();
        eprintln!("{} users from Aruba", users.len());
        assert!(!users.is_empty());
    }

    #[test]
    fn from_api_reads_what_the_web_editor_reads() {
        let v: Value = serde_json::from_str(
            r#"[{"id":"3100001","callsign":"k6abc","fname":" Ann ","surname":null,"city":0,"state":"Ohio","country":"US"},
                {"radio_id":5,"callsign":"w1aw"},{"id":0,"callsign":"x"},{"id":16777216,"callsign":"x"},{"id":7,"callsign":""},{"id":null,"radio_id":9,"callsign":"n0"}]"#,
        )
        .unwrap();
        let u = from_api(&v);
        assert_eq!(u.iter().map(|u| (u.id, u.call.as_str())).collect::<Vec<_>>(), [(3100001, "K6ABC"), (5, "W1AW"), (9, "N0")]);
        assert_eq!((u[0].name.as_str(), u[0].last.as_str(), u[0].city.as_str()), ("Ann", "", ""));
    }

    /// The web editor's `fromApi` on random replies (`RFCPS_RIDAPI_DIR/api.json`, `ridapi_gold.mjs`).
    #[test]
    fn from_api_matches_the_web_editor() {
        let Ok(dir) = std::env::var("RFCPS_RIDAPI_DIR") else {
            eprintln!("RFCPS_RIDAPI_DIR not set: skipped");
            return;
        };
        let cases: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(format!("{dir}/api.json")).unwrap()).unwrap();
        assert!(cases.len() >= 5);
        for c in cases {
            let got = from_api(&c["results"]);
            let want: Vec<User> = c["users"]
                .as_array()
                .unwrap()
                .iter()
                .map(|u| {
                    let s = |k: &str| u[k].as_str().unwrap().to_string();
                    User { id: u["id"].as_u64().unwrap() as u32, call: s("call"), name: s("name"), last: s("last"), city: s("city"), state: s("state"), country: s("country") }
                })
                .collect();
            assert_eq!(got, want, "results {}", c["results"]);
        }
    }
}
