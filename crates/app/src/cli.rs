// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux

//! Command-line modes that run without a window: `--ports`, `--read PORT OUT.bin`, `--version`, `--help`.
//! `--read` only reads from the radio; nothing here writes to it.

/// Runs a command-line mode when the first argument asks for one. Returns the exit code, or None to open the window.
pub fn run(args: &[String]) -> Option<i32> {
    let first = args.first()?.as_str();
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
