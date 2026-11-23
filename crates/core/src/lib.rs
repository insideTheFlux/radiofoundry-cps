// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux

//! The codeplug as data: channels, zones, contacts, groups and settings, and how they are laid out in the
//! radio's image. No serial port and no GUI in this crate, so everything in it is unit tested on the host.
//! Formats follow RadioFoundry's `docs/spec/` (open-firmware-cps.md 7).

pub mod chirp;
pub mod csv;
pub mod diff;
pub mod fields;
pub mod freqs;
pub mod iddb;
mod iddb_compact;
pub mod image;
pub mod kv;
pub mod model;
pub mod modes;
pub mod problems;
pub mod qdmr;
mod qdmr_modes;
pub mod radioid;
pub mod regions;
pub mod records;
pub mod stock_dm32;
pub mod systems;
pub mod yaml;

pub use image::Image;
pub use model::Codeplug;
