// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux

//! On a Windows build host, puts the icon and the program's name into the .exe. A failure here is only a warning: the
//! program works without them.

fn main() {
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../../packaging/rfcps.ico");
        res.set("ProductName", "RadioFoundry CPS");
        res.set("FileDescription", "RadioFoundry CPS");
        if let Err(e) = res.compile() {
            println!("cargo:warning=could not embed the icon: {e}");
        }
    }
}
