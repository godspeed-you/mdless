//! `mdless` — an interactive terminal Markdown reader.
//!
//! The crate is organised as follows:
//!
//! * [`document`] — parsing and the semantic document model (sections, anchors,
//!   links, search index).
//! * [`layout`] — spatial decisions (wrapping, tables, widths).
//! * [`render`] — terminal output only.
//! * [`mermaid`] — Mermaid subset renderer and backends.
//! * [`terminal`] — capability detection and terminal lifecycle.
//! * [`config`] / [`cli`] — configuration schema, loader, keybindings and CLI.
//! * [`app`] — interactive state machine and actions.
//!
//! The library form exists so that integration and snapshot tests can use the
//! same code as the binary.

#![warn(missing_docs)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod app;
pub mod cli;
pub mod config;
pub mod document;
pub mod layout;
pub mod mermaid;
pub mod render;
pub mod terminal;
pub mod util;

/// Shared test-support vocabulary.
///
/// `#[cfg(test)]` — not a cargo feature — so it is compiled only when the
/// library is built as a test target and is entirely absent from
/// `cargo build --release`; a feature could be switched on from outside the
/// crate, which is exactly the leak to avoid.
#[cfg(test)]
pub(crate) mod testing;
