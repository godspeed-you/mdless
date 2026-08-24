//! Terminal lifecycle: raw mode, alternate screen, mouse capture and a panic
//! hook that restores the terminal *before* the panic message is printed.
//!
//! No ANSI or style state may ever leak into the user's shell after exit, on
//! **every** exit path: normal return, `?` propagation, panic and Ctrl-C.
//!
//! The guard is RAII ([`TerminalGuard`]); restoration is idempotent and driven
//! by a process-global state word so the panic hook — which cannot borrow the
//! guard — can perform exactly the same cleanup.

use std::io::{self, IsTerminal, Write};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Once;

use crossterm::cursor::{Hide, Show};
use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};

/// Bit: raw mode is enabled.
const STATE_RAW: u8 = 1 << 0;
/// Bit: the alternate screen is active.
const STATE_ALT: u8 = 1 << 1;
/// Bit: mouse capture is active.
const STATE_MOUSE: u8 = 1 << 2;
/// Bit: the cursor is hidden.
const STATE_CURSOR_HIDDEN: u8 = 1 << 3;
/// Bit: keyboard enhancement flags were pushed.
const STATE_KEYBOARD: u8 = 1 << 4;

/// What the terminal currently owes us, shared with the panic hook.
static STATE: AtomicU8 = AtomicU8::new(0);
static HOOK_INSTALLED: Once = Once::new();
static IN_PANIC_RESTORE: AtomicBool = AtomicBool::new(false);

/// Errors from entering or leaving the terminal UI.
#[derive(Debug, thiserror::Error)]
pub enum TerminalError {
    /// stdout is not a terminal, so the interactive UI cannot be entered.
    #[error("standard output is not a terminal")]
    NotATerminal,
    /// No controlling terminal is available for keyboard input.
    #[error("no controlling terminal for keyboard input: {0}")]
    NoControllingTty(String),
    /// An underlying I/O error.
    #[error("terminal I/O error: {0}")]
    Io(#[from] io::Error),
}

/// Options for [`TerminalGuard::enter`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalOptions {
    /// Switch to the alternate screen buffer.
    pub alternate_screen: bool,
    /// Enable mouse reporting.
    pub mouse: bool,
    /// Hide the cursor while the UI is drawn.
    pub hide_cursor: bool,
    /// Request kitty keyboard-enhancement flags.
    ///
    /// Off by default: enabling it costs a terminal round trip
    /// (`supports_keyboard_enhancement`) and diple does not need
    /// disambiguated escape codes for its keymap.
    pub keyboard_enhancement: bool,
}

impl Default for TerminalOptions {
    fn default() -> Self {
        Self {
            alternate_screen: true,
            mouse: false,
            hide_cursor: true,
            keyboard_enhancement: false,
        }
    }
}

/// RAII guard owning the terminal's raw/alternate-screen state.
///
/// Dropping it restores the terminal in reverse order. Restoration is
/// idempotent: an explicit [`TerminalGuard::restore`] followed by `Drop`, or a
/// double `Drop`, performs the work exactly once.
#[derive(Debug)]
pub struct TerminalGuard {
    restored: bool,
}

impl TerminalGuard {
    /// Enter the terminal UI with the given options.
    ///
    /// Fails with [`TerminalError::NotATerminal`] when stdout is redirected, so
    /// the caller can fall back to non-interactive output.
    pub fn enter(options: TerminalOptions) -> Result<TerminalGuard, TerminalError> {
        if !stdout_is_tty() {
            return Err(TerminalError::NotATerminal);
        }
        install_panic_hook();

        let mut out = io::stdout();
        enable_raw_mode()?;
        set_bits(STATE_RAW);

        if options.alternate_screen {
            execute!(out, EnterAlternateScreen)?;
            set_bits(STATE_ALT);
        }
        if options.mouse {
            execute!(out, EnableMouseCapture)?;
            set_bits(STATE_MOUSE);
        }
        if options.hide_cursor {
            execute!(out, Hide)?;
            set_bits(STATE_CURSOR_HIDDEN);
        }
        if options.keyboard_enhancement && keyboard_enhancement_is_safe() {
            use crossterm::event::{KeyboardEnhancementFlags, PushKeyboardEnhancementFlags};
            if execute!(
                out,
                PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
            )
            .is_ok()
            {
                set_bits(STATE_KEYBOARD);
            }
        }
        out.flush()?;
        Ok(TerminalGuard { restored: false })
    }

    /// Restore the terminal now. Safe to call repeatedly.
    pub fn restore(&mut self) -> Result<(), TerminalError> {
        if self.restored {
            return Ok(());
        }
        self.restored = true;
        restore_terminal()
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        // Errors during unwinding cannot be reported; the global state word
        // makes a second attempt harmless.
        let _ = self.restore();
    }
}

/// Restore the terminal to its pre-`diple` state, in reverse order.
///
/// Idempotent and safe to call when nothing was entered — each step runs only
/// if the corresponding bit is still set. Used by [`TerminalGuard::restore`]
/// and by the panic hook.
pub fn restore_terminal() -> Result<(), TerminalError> {
    let state = STATE.swap(0, Ordering::SeqCst);
    if state == 0 {
        return Ok(());
    }
    let mut out = io::stdout();
    let mut first_error: Option<io::Error> = None;
    let mut step = |result: io::Result<()>| {
        if let Err(e) = result {
            if first_error.is_none() {
                first_error = Some(e);
            }
        }
    };

    if state & STATE_KEYBOARD != 0 {
        use crossterm::event::PopKeyboardEnhancementFlags;
        step(execute!(out, PopKeyboardEnhancementFlags));
    }
    if state & STATE_CURSOR_HIDDEN != 0 {
        step(execute!(out, Show));
    }
    if state & STATE_MOUSE != 0 {
        step(execute!(out, DisableMouseCapture));
    }
    if state & STATE_ALT != 0 {
        step(execute!(out, LeaveAlternateScreen));
    }
    // Reset any lingering SGR/attribute state before leaving raw mode so no
    // styling can bleed into the shell.
    step(out.write_all(b"\x1b[0m").and_then(|()| out.flush()));
    if state & STATE_RAW != 0 {
        step(disable_raw_mode());
    }
    match first_error {
        Some(e) => Err(TerminalError::Io(e)),
        None => Ok(()),
    }
}

/// Install the panic hook that restores the terminal before the previous hook
/// prints the panic message. Installed automatically by [`TerminalGuard::enter`]
/// and safe to call more than once.
pub fn install_panic_hook() {
    HOOK_INSTALLED.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            // Guard against a panic inside the restore path recursing.
            if !IN_PANIC_RESTORE.swap(true, Ordering::SeqCst) {
                let _ = restore_terminal();
                IN_PANIC_RESTORE.store(false, Ordering::SeqCst);
            }
            previous(info);
        }));
    });
}

/// Whether standard output is connected to a terminal.
pub fn stdout_is_tty() -> bool {
    io::stdout().is_terminal()
}

/// Whether standard input is connected to a terminal (false when the document
/// is piped in).
pub fn stdin_is_tty() -> bool {
    io::stdin().is_terminal()
}

/// Whether an interactive session is possible: stdout is a terminal and a
/// keyboard source exists (stdin, or the controlling terminal `/dev/tty`).
pub fn is_interactive() -> bool {
    stdout_is_tty() && (stdin_is_tty() || open_input_tty().is_ok())
}

/// Open the controlling terminal for keyboard input.
///
/// The document may arrive on stdin (`cat doc.md | diple`), in which case
/// stdin cannot also deliver key events; `/dev/tty` is then the keyboard
/// source. Returns [`TerminalError::NoControllingTty`] when there is none (CI,
/// `setsid`, cron), so the caller can fall back to non-interactive output
/// instead of hanging.
#[cfg(unix)]
pub fn open_input_tty() -> Result<std::fs::File, TerminalError> {
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/tty")
        .map_err(|e| TerminalError::NoControllingTty(format!("/dev/tty: {e}")))
}

/// Open the controlling terminal for keyboard input.
///
/// Only Unix has `/dev/tty`; elsewhere the caller must use stdin.
#[cfg(not(unix))]
pub fn open_input_tty() -> Result<std::fs::File, TerminalError> {
    Err(TerminalError::NoControllingTty(
        "/dev/tty is not available on this platform".to_string(),
    ))
}

/// Run `f` with the terminal in UI mode, restoring it on every exit path
/// (including an `Err` return or a panic inside `f`).
pub fn with_terminal<T, E>(
    options: TerminalOptions,
    f: impl FnOnce(&mut TerminalGuard) -> Result<T, E>,
) -> Result<T, E>
where
    E: From<TerminalError>,
{
    let mut guard = TerminalGuard::enter(options).map_err(E::from)?;
    let result = f(&mut guard);
    let restore = guard.restore();
    match result {
        Ok(value) => {
            restore.map_err(E::from)?;
            Ok(value)
        }
        Err(e) => Err(e),
    }
}

/// Whether asking crossterm for keyboard-enhancement support is safe here.
///
/// The query writes an escape sequence and waits for an answer, so it is only
/// attempted on a real tty and never inside a multiplexer, where the reply may
/// be swallowed (see the module comment in
/// [`capabilities`](super::capabilities)).
fn keyboard_enhancement_is_safe() -> bool {
    if !stdout_is_tty() || std::env::var_os("TMUX").is_some() || std::env::var_os("STY").is_some() {
        return false;
    }
    crossterm::terminal::supports_keyboard_enhancement().unwrap_or(false)
}

fn set_bits(bits: u8) {
    STATE.fetch_or(bits, Ordering::SeqCst);
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// `STATE` is process-global and these tests write to it, so they must not
    /// run against each other inside the shared test binary.
    static STATE_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn restore_without_enter_is_a_noop() {
        let _guard = STATE_LOCK.lock();
        STATE.store(0, Ordering::SeqCst);
        assert!(restore_terminal().is_ok());
        assert!(restore_terminal().is_ok());
    }

    #[test]
    fn enter_fails_cleanly_without_a_tty() {
        // The test harness captures stdout, so this is never a terminal.
        if stdout_is_tty() {
            return;
        }
        let err = TerminalGuard::enter(TerminalOptions::default()).unwrap_err();
        assert!(matches!(err, TerminalError::NotATerminal));
    }

    #[test]
    fn with_terminal_propagates_the_not_a_terminal_error() {
        if stdout_is_tty() {
            return;
        }
        let r: Result<(), TerminalError> =
            with_terminal(TerminalOptions::default(), |_| Ok::<(), TerminalError>(()));
        assert!(matches!(r, Err(TerminalError::NotATerminal)));
    }

    /// A panic must leave nothing for the terminal to owe us.
    ///
    /// This used to be `panic_hook_installs_once`, which called
    /// `install_panic_hook()` twice and asserted nothing — it proved only that
    /// neither call panicked. Terminal restoration is the failure mode that
    /// leaves the user's shell in raw mode on the alternate screen, so it is
    /// the wrong place for a test that looks like coverage and is not.
    ///
    /// What *is* observable without changing production code is the state
    /// word: `restore_terminal` swaps it to zero and performs exactly the
    /// steps whose bits were set. `restore_terminal` writes to `io::stdout()`
    /// directly rather than to an injectable writer, so asserting on the bytes
    /// would mean widening the production signature for the test's benefit;
    /// the state word is the same claim — the hook ran the restore — at no
    /// such cost. `STATE_CURSOR_HIDDEN` is the bit chosen because its
    /// restoration (show the cursor, reset SGR) is invisible when nothing was
    /// hidden, unlike leaving an alternate screen that was never entered.
    #[test]
    fn a_panic_runs_the_terminal_restore() {
        let _guard = STATE_LOCK.lock();
        // Idempotent: two calls install exactly one hook.
        install_panic_hook();
        install_panic_hook();

        STATE.store(STATE_CURSOR_HIDDEN, Ordering::SeqCst);
        let panicked = std::panic::catch_unwind(|| panic!("deliberate test panic"));
        assert!(panicked.is_err(), "the closure really did panic");
        assert_eq!(
            STATE.load(Ordering::SeqCst),
            0,
            "the panic hook must restore the terminal before the message is printed"
        );

        // And the restore is idempotent: the guard's own `Drop` afterwards
        // must not repeat the work.
        assert!(restore_terminal().is_ok());
        assert_eq!(STATE.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn options_default_is_conservative() {
        let o = TerminalOptions::default();
        assert!(o.alternate_screen);
        assert!(!o.mouse);
        assert!(!o.keyboard_enhancement);
    }
}
