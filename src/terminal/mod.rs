//! Terminal runtime: capability detection, raw-mode lifecycle and image /
//! hyperlink protocol encoders.
//!
//! Owned by Workstream A.
//!
//! * [`capabilities`] — what the terminal can do, decided from environment
//!   variables only (never by querying the terminal at startup).
//! * [`lifecycle`] — the RAII [`TerminalGuard`], panic-safe restoration and
//!   `/dev/tty` keyboard input when the document arrives on stdin.
//! * `protocols` — pure encoders for OSC 8, Kitty graphics, iTerm2 inline
//!   images, Sixel and tmux passthrough; crate-internal, funnelled through
//!   `ImageSupport::encode`.

pub mod capabilities;
pub mod lifecycle;
pub(crate) mod protocols;

pub use capabilities::{
    detect, detect_from, Capabilities, CapabilityOverrides, ColorLevel, Evidence, ImageSupport,
    TerminalSize,
};
pub use lifecycle::{
    install_panic_hook, is_interactive, open_input_tty, restore_terminal, stdin_is_tty,
    stdout_is_tty, with_terminal, TerminalError, TerminalGuard, TerminalOptions,
};
