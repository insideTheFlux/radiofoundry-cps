// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux

//! Downloading RadioID's `user.csv` on a thread of its own. The only network use in the editor, and only when the
//! person presses the button: one GET of the file's address, nothing sent but the request itself.

use eframe::egui;
use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::time::Duration;

/// More than a registry file can be: a reply past this is refused instead of filling the memory.
const MAX_BYTES: u64 = 400 * 1024 * 1024;

pub enum Msg {
    /// Bytes received so far, and the file's size when the server said.
    Progress(u64, Option<u64>),
    Done(Result<String, String>),
}

pub struct Download {
    pub rx: Receiver<Msg>,
    stop: Arc<AtomicBool>,
    pub received: u64,
    pub total: Option<u64>,
}

impl Download {
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// Starts downloading `url` (a `user.csv`) on a new thread. The text comes back whole in the last message.
pub fn start(url: String, ctx: egui::Context) -> Download {
    let (tx, rx) = channel();
    let stop = Arc::new(AtomicBool::new(false));
    let flag = stop.clone();
    std::thread::spawn(move || {
        let result = get(&url, &flag, &mut |d, t| {
            let _ = tx.send(Msg::Progress(d, t));
            ctx.request_repaint();
        });
        let _ = tx.send(Msg::Done(result));
        ctx.request_repaint();
    });
    Download { rx, stop, received: 0, total: None }
}

fn get(url: &str, stop: &AtomicBool, progress: &mut dyn FnMut(u64, Option<u64>)) -> Result<String, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(20)))
        .timeout_recv_body(Some(Duration::from_secs(120)))
        .user_agent("RadioFoundry-CPS")
        .build()
        .into();
    let resp = agent.get(url).call().map_err(|e| format!("could not reach {url}: {e}"))?;
    let total = resp.body().content_length();
    if total.is_some_and(|t| t > MAX_BYTES) {
        return Err("the file is larger than a user list can be".into());
    }
    let mut body = resp.into_body();
    let mut reader = body.as_reader();
    let mut out: Vec<u8> = Vec::new();
    let mut chunk = vec![0u8; 64 * 1024];
    let mut last = 0u64;
    loop {
        if stop.load(Ordering::Relaxed) {
            return Err("stopped".into());
        }
        let n = reader.read(&mut chunk).map_err(|e| format!("the download broke off: {e}"))?;
        if n == 0 {
            break;
        }
        out.extend_from_slice(&chunk[..n]);
        if out.len() as u64 > MAX_BYTES {
            return Err("the file is larger than a user list can be".into());
        }
        if out.len() as u64 - last >= 256 * 1024 {
            last = out.len() as u64;
            progress(last, total);
        }
    }
    progress(out.len() as u64, total);
    Ok(String::from_utf8_lossy(&out).into_owned())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::io::Write;
    use std::net::TcpListener;

    /// A one-request server on this computer: `body` after `head`, then closed.
    pub(crate) fn serve(head: &'static str, body: Vec<u8>) -> String {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        std::thread::spawn(move || {
            if let Ok((mut s, _)) = l.accept() {
                let mut buf = [0u8; 2048];
                let _ = s.read(&mut buf);
                let _ = s.write_all(head.replace("{len}", &body.len().to_string()).as_bytes());
                let _ = s.write_all(&body);
            }
        });
        format!("http://{addr}/user.csv")
    }

    #[test]
    fn a_file_comes_down_whole_with_progress() {
        let body: Vec<u8> = (0..600_000).map(|i| b'a' + (i % 26) as u8).collect();
        let url = serve("HTTP/1.1 200 OK\r\nContent-Length: {len}\r\nConnection: close\r\n\r\n", body.clone());
        let stop = AtomicBool::new(false);
        let mut seen = Vec::new();
        let got = get(&url, &stop, &mut |d, t| seen.push((d, t))).unwrap();
        assert_eq!(got.as_bytes(), &body[..]);
        assert!(seen.len() >= 2);
        assert_eq!(seen.last(), Some(&(600_000, Some(600_000))));
    }

    #[test]
    fn an_error_status_is_an_error() {
        let url = serve("HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n", Vec::new());
        let stop = AtomicBool::new(false);
        let e = get(&url, &stop, &mut |_, _| {}).unwrap_err();
        assert!(e.contains("could not reach"), "{e}");
    }

    #[test]
    fn a_stop_ends_it() {
        let body = vec![b'x'; 3_000_000];
        let url = serve("HTTP/1.1 200 OK\r\nContent-Length: {len}\r\nConnection: close\r\n\r\n", body);
        let stop = AtomicBool::new(true);
        assert_eq!(get(&url, &stop, &mut |_, _| {}).unwrap_err(), "stopped");
    }

    #[test]
    fn nothing_listening_is_an_error() {
        let stop = AtomicBool::new(false);
        let e = get("http://127.0.0.1:1/user.csv", &stop, &mut |_, _| {}).unwrap_err();
        assert!(e.contains("could not reach"), "{e}");
    }

    /// Needs the network (about 17 MB from radioid.net): `cargo test -p rfcps radioid_net -- --ignored`.
    #[test]
    #[ignore]
    fn radioid_net_user_file_comes_down_over_tls_and_parses() {
        let stop = AtomicBool::new(false);
        let text = get(rfcps_core::radioid::DOWNLOAD, &stop, &mut |_, _| {}).unwrap();
        let users = rfcps_core::radioid::parse_users(&text);
        eprintln!("{} bytes, {} users", text.len(), users.len());
        assert!(users.len() > 100_000);
        // and the whole registry becomes a database: the time it takes and what it fills
        let all: Vec<&rfcps_core::radioid::User> = users.iter().collect();
        let recs = rfcps_core::radioid::iddb_records(&all, "{call} {first} {city} {st}", false, 32);
        let t = std::time::Instant::now();
        let est = rfcps_core::radioid::iddb_size_c(&recs, usize::MAX);
        eprintln!("estimate: {:?} in {:?}", est.bytes, t.elapsed());
        let t = std::time::Instant::now();
        let mut img = rfcps_core::Image::blank();
        let w = rfcps_core::iddb::write(&mut img, &recs, rfcps_core::iddb::Options { format: b'C', ..Default::default() }).unwrap();
        eprintln!("written: {} records, top {:#x} in {:?}", w.count, w.top, t.elapsed());
        assert_eq!(rfcps_core::iddb::read(img.bytes()).len(), users.len());
    }
}
