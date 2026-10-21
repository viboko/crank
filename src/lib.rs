// SPDX-FileCopyrightText: The crank contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Core library for the `crank` code generator.
//!
//! Parsing and rendering logic live here as pure functions so they can be
//! unit tested without touching the filesystem; `main.rs` is the thin I/O
//! layer that wires them together.

pub mod check;
pub mod crank_file;
pub mod data;
pub mod header;
pub mod manifest;
pub mod output;
pub mod pocket;
pub mod render;
pub mod rhai_filter;
pub mod run;
pub mod selector;
pub mod unicode;
