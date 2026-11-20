// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux

//! Reads and writes a radio's codeplug image over its serial port (programming cable or USB), with the open
//! firmware CPS protocol (RadioFoundry `docs/spec/open-firmware-cps.md`).

/// Names of the serial ports on this computer, for the port chooser.
pub fn port_names() -> Vec<String> {
    serialport::available_ports()
        .map(|ports| ports.into_iter().map(|p| p.port_name).collect())
        .unwrap_or_default()
}
