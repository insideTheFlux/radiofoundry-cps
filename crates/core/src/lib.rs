// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux

//! The codeplug as data: channels, zones, contacts, groups and settings, and how they are laid out in the
//! radio's image. No serial port and no GUI in this crate, so everything in it is unit tested on the host.
//! Formats follow RadioFoundry's `docs/spec/` (open-firmware-cps.md 7).

pub mod fields;
pub mod freqs;
pub mod image;
pub mod model;
pub mod problems;
pub mod records;

pub use image::Image;
pub use model::Codeplug;
