//! Terminal rendering: themes, render primitives and the ratatui widget layer.
//!
//! Owned by Workstream C.
//!
//! * [`primitives`] — the [`RenderTree`] contract produced by
//!   [`crate::layout`],
//! * [`ansi`] — SGR serialization of a [`RenderTree`] for the non-interactive
//!   `--color always` path,
//! * [`theme`] — styles, built-in dark/light themes and the colour downgrade
//!   chain,
//! * `terminal` — ratatui widgets (document view, status bar, TOC sidebar,
//!   help overlay); crate-internal, driven by `app`.
//!
//! Nothing here knows about Markdown semantics; the renderer only draws
//! primitives.

pub mod ansi;
pub mod primitives;
pub(crate) mod terminal;
pub mod theme;

pub use ansi::to_ansi_text;
pub use primitives::{ImageRef, LineKind, RenderLine, RenderTree, StyledSpan};
pub use theme::{Color, ColorLevel, Style, Theme};
