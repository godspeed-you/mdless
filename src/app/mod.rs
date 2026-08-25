//! Interactive application: state, event loop, navigation, search UI and TOC.
//!
//! Owned by Workstream D. [`Action`], the bindable-action vocabulary, lives in
//! [`crate::config::actions`] and is re-exported here so that the Phase 1
//! interface contract paths keep resolving.
//!
//! * `state` — [`App`], the layout cache and every action handler,
//! * [`events`] — the crossterm event loop and the draw pass (including image
//!   and OSC 8 painting, which ratatui cannot do),
//! * `adapters` — the conversions between the workstream boundary enums,
//! * `diagrams` — the Mermaid → layout adapter with its render cache and
//!   image registry,
//! * `toc` / `search_ui` — sidebar and search prompt state,
//! * `hints` — the right-hand key hints sidebar and its context rules.

pub(crate) mod adapters;
pub(crate) mod command;
pub(crate) mod diagrams;
pub mod events;
pub(crate) mod hints;
pub(crate) mod search_ui;
pub(crate) mod state;
pub(crate) mod toc;

pub use crate::config::actions::{self, Action};
pub use adapters::{color_level, render_environment, resolve_theme};
pub use diagrams::DiagramProvider;
pub use state::{App, AppEnv, AppOptions, Mode};
