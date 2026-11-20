// SPDX-License-Identifier: MIT
// Copyright (c) 2026 insideTheFlux

//! Talks to a RadioFoundry radio over its serial port: the debug link's frames (RadioFoundry `docs/spec/debuglink.md`)
//! and the fast codeplug protocol on top of them (`docs/spec/cps-fast.md`).

pub mod fake;
pub mod fast;
pub mod frame;
pub mod radio;

pub use fast::{Block, Info, Sums};
pub use radio::{open, port_names, LinkError, Radio};
