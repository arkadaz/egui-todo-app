//! Focus Hub's logic: plain Rust with no Flutter code, ported from the desktop (egui) app.
//! `crate::api` exposes it to Flutter.

pub mod dates;
pub mod domain;
pub mod hub;
pub mod persistence;
pub mod timer;
