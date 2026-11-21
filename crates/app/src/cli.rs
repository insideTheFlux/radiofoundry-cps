// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux

//! Command-line modes that run without a window: `--ports`, `--read PORT OUT.bin`, `--version`, `--help`.
//! `--read` only reads from the radio; nothing here writes to it.

/// Runs a command-line mode when the first argument asks for one. Returns the exit code, or None to open the window.
pub fn run(args: &[String]) -> Option<i32> {
    let first = args.first()?.as_str();
    if first.starts_with('-') {
        attach_console();
    }
    match first {
        "--version" | "-V" => {
            println!("rfcps {}", env!("CARGO_PKG_VERSION"));
            Some(0)
        }
        "--help" | "-h" => {
            println!("{USAGE}");
            Some(0)
        }
        "--ports" => {
            for p in rfcps_link::ports() {
                let tag = if p.likely { " (likely a radio)" } else { "" };
                println!("{}\t{}{}", p.name, p.description, tag);
            }
            Some(0)
        }
        "--read" => Some(match args {
            [_, port, out] => read(port, out),
            _ => {
                eprintln!("usage: rfcps --read PORT OUT.bin");
                2
            }
        }),
        _ if first.starts_with("--") => {
            eprintln!("unknown option {first}\n{USAGE}");
            Some(2)
        }
        _ => None,
    }
}

/// The Windows release is a GUI program with no console, so a command-line mode would print nowhere. Attach to the
/// console of the terminal that started it and point stdout and stderr there, unless they are already redirected to a
/// file or a pipe. A double-click launch has no parent console and stays windowed.
#[cfg(windows)]
fn attach_console() {
    use std::os::windows::io::IntoRawHandle;
    #[link(name = "kernel32")]
    extern "system" {
        fn AttachConsole(process_id: u32) -> i32;
        fn GetStdHandle(which: u32) -> isize;
        fn SetStdHandle(which: u32, handle: isize) -> i32;
    }
    const ATTACH_PARENT_PROCESS: u32 = u32::MAX;
    const STD_OUTPUT_HANDLE: u32 = -11i32 as u32;
    const STD_ERROR_HANDLE: u32 = -12i32 as u32;
    // SAFETY: plain Win32 calls with valid arguments; the handles opened here are kept for the life of the process.
    unsafe {
        if AttachConsole(ATTACH_PARENT_PROCESS) == 0 {
            return;
        }
        for which in [STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
            let h = GetStdHandle(which);
            if h == 0 || h == -1 {
                if let Ok(f) = std::fs::OpenOptions::new().write(true).open("CONOUT$") {
                    SetStdHandle(which, f.into_raw_handle() as isize);
                }
            }
        }
    }
}

#[cfg(not(windows))]
fn attach_console() {}

const USAGE: &str = "usage: rfcps [FILE]            open the editor (optionally with a codeplug file)\n       rfcps --ports           list serial ports\n       rfcps --read PORT OUT   read the radio's codeplug into a file (read only)\n       rfcps --version";

fn read(port: &str, out: &str) -> i32 {
    match read_inner(port, out) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("error: {e}");
            1
        }
    }
}

fn read_inner(port: &str, out: &str) -> Result<(), String> {
    let mut radio = rfcps_link::open(port).map_err(|e| e.to_string())?;
    radio.ping().map_err(|e| e.to_string())?;
    let t = std::time::Instant::now();
    let image = radio
        .read_image(&mut |done, total| {
            if done % 8 == 0 || done == total {
                eprint!("\r{done}/{total} blocks");
            }
        })
        .map_err(|e| e.to_string())?;
    eprintln!();
    std::fs::write(out, image.bytes()).map_err(|e| format!("{out}: {e}"))?;
    println!("read {} bytes in {:.1} s to {out}", image.len(), t.elapsed().as_secs_f32());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::run;

    fn a(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn no_arguments_or_a_file_opens_the_window() {
        assert_eq!(run(&[]), None);
        assert_eq!(run(&a(&["codeplug.bin"])), None);
    }

    #[test]
    fn bad_options_exit_with_a_usage_code() {
        assert_eq!(run(&a(&["--nope"])), Some(2));
        assert_eq!(run(&a(&["--read", "only-a-port"])), Some(2));
        assert_eq!(run(&a(&["--version"])), Some(0));
    }
}
