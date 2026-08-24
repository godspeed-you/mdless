//! `diple` — an interactive terminal Markdown reader.
//!
//! Flow: parse CLI → load and merge configuration → handle the diagnostic and
//! generator flags → read the document (file or stdin) → parse → detect
//! terminal capabilities → run the interactive pager, or print the plain
//! rendered document when the output is not a terminal.
//!
//! Exit codes: `0` success, `1` runtime error, `2` usage or configuration
//! error.

use std::io::{IsTerminal, Read, Write};
use std::path::Path;
use std::process::ExitCode;
use std::time::Instant;

use clap::Parser;
use diple::app::{self, App, AppEnv, AppOptions, DiagramProvider};
use diple::cli::CliArgs;
use diple::config::{self, Config, KeyMap};
use diple::document::{Document, NodeKind};
use diple::layout::{Layout, LayoutOptions};
use diple::mermaid::select_backend;
use diple::terminal::{self, lifecycle, Capabilities};

/// Exit code for a usage or configuration problem.
const EXIT_USAGE: u8 = 2;
/// Exit code for a runtime failure.
const EXIT_FAILURE: u8 = 1;

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(Failure::Usage(message)) => {
            eprintln!("diple: {message}");
            ExitCode::from(EXIT_USAGE)
        }
        Err(Failure::Runtime(message)) => {
            eprintln!("diple: {message}");
            ExitCode::from(EXIT_FAILURE)
        }
        Err(Failure::BrokenPipe) => ExitCode::SUCCESS,
    }
}

/// How `run` can fail; each variant maps to one exit code.
enum Failure {
    /// Usage or configuration error (exit 2).
    Usage(String),
    /// Runtime error (exit 1).
    Runtime(String),
    /// stdout was closed (`diple x.md | head`) — not an error.
    BrokenPipe,
}

impl From<std::io::Error> for Failure {
    fn from(error: std::io::Error) -> Failure {
        if error.kind() == std::io::ErrorKind::BrokenPipe {
            Failure::BrokenPipe
        } else {
            Failure::Runtime(error.to_string())
        }
    }
}

fn run() -> Result<ExitCode, Failure> {
    let started = Instant::now();
    let args = match CliArgs::try_parse() {
        Ok(args) => args,
        Err(error) => {
            // clap prints help/version to stdout and errors to stderr itself.
            let _ = error.print();
            return Ok(if error.use_stderr() {
                ExitCode::from(EXIT_USAGE)
            } else {
                ExitCode::SUCCESS
            });
        }
    };

    // Hidden generators used by packaging; they must work without a terminal.
    if let Some(shell) = args.generate_completions {
        let mut out = std::io::stdout();
        diple::cli::generate_completions(shell, &mut out);
        out.flush()?;
        return Ok(ExitCode::SUCCESS);
    }
    if args.generate_man {
        let mut out = std::io::stdout();
        diple::cli::generate_man(&mut out)?;
        out.flush()?;
        return Ok(ExitCode::SUCCESS);
    }

    let loaded = config::load(args.config.as_deref(), args.no_config)
        .map_err(|e| Failure::Usage(e.to_string()))?;
    let cfg = loaded
        .config
        .merged(&args)
        .map_err(|e| Failure::Usage(e.to_string()))?;
    // Keybindings are part of the configuration, so `--check-config` must
    // validate them too.
    let keymap = KeyMap::from_overrides(cfg.keys.iter().map(|(k, v)| (k.as_str(), v)))
        .map_err(|e| Failure::Usage(e.to_string()))?;
    if args.check_config {
        let mut out = std::io::stdout();
        match &loaded.path {
            Some(path) => writeln!(out, "configuration ok: {}", path.display())?,
            None => writeln!(out, "configuration ok: built-in defaults")?,
        }
        return Ok(ExitCode::SUCCESS);
    }

    let mut overrides = terminal::CapabilityOverrides::from_config(&cfg);
    overrides.width = args.width;
    let caps = terminal::detect(&overrides);

    if args.print_capabilities {
        let mut out = std::io::stdout();
        write!(out, "{}", caps.describe())?;
        out.flush()?;
        return Ok(ExitCode::SUCCESS);
    }

    let (name, source) = read_input(args.file.as_deref())?;
    let doc = diple::document::parse(&source);
    if args.debug {
        eprintln!(
            "diple: parsed {} nodes in {:?}",
            doc.node_count(),
            started.elapsed()
        );
    }

    let interactive = std::io::stdout().is_terminal()
        && !is_dumb_terminal()
        && (lifecycle::stdin_is_tty() || lifecycle::open_input_tty().is_ok());

    if !interactive {
        return print_plain(&doc, &cfg, &caps, args.width).map(|()| ExitCode::SUCCESS);
    }

    let color = app::color_level(cfg.color, &caps);
    let theme = app::resolve_theme(&cfg.theme, color);
    let diagrams = build_diagrams(&doc, &cfg, &caps, usize::from(caps.size.0));

    let term_size = caps.size;
    lifecycle::install_panic_hook();
    let options = lifecycle::TerminalOptions {
        alternate_screen: true,
        mouse: cfg.mouse && caps.mouse,
        hide_cursor: true,
        keyboard_enhancement: false,
    };
    let mut app = App::new(
        doc,
        cfg,
        keymap,
        AppEnv {
            caps,
            theme,
            color,
            diagrams,
        },
        AppOptions {
            filename: name,
            size: term_size,
            width_override: args.width,
            debug: args.debug,
        },
    );
    if args.debug {
        eprintln!("diple: first frame ready after {:?}", started.elapsed());
    }

    let mut guard = lifecycle::TerminalGuard::enter(options)
        .map_err(|e| Failure::Runtime(format!("cannot enter the terminal UI: {e}")))?;
    let result = app::events::run(&mut app);
    // Restore before reporting anything, on every exit path.
    let restored = guard.restore();
    result.map_err(Failure::from)?;
    restored.map_err(|e| Failure::Runtime(e.to_string()))?;
    Ok(ExitCode::SUCCESS)
}

/// `TERM=dumb` disables the interactive UI (graceful degradation).
fn is_dumb_terminal() -> bool {
    matches!(std::env::var("TERM").as_deref(), Ok("dumb") | Ok(""))
}

/// Read the document from a file or stdin.
///
/// Invalid UTF-8 is converted lossily with a warning rather than failing
/// (render as best effort).
fn read_input(file: Option<&Path>) -> Result<(String, String), Failure> {
    match file {
        Some(path) => {
            let bytes = std::fs::read(path)
                .map_err(|e| Failure::Runtime(format!("cannot read {}: {e}", path.display())))?;
            Ok((
                path.display().to_string(),
                decode(bytes, &path.display().to_string()),
            ))
        }
        None => {
            let mut bytes = Vec::new();
            std::io::stdin()
                .read_to_end(&mut bytes)
                .map_err(|e| Failure::Runtime(format!("cannot read from stdin: {e}")))?;
            Ok(("<stdin>".to_string(), decode(bytes, "<stdin>")))
        }
    }
}

fn decode(bytes: Vec<u8>, name: &str) -> String {
    match String::from_utf8(bytes) {
        Ok(text) => text,
        Err(error) => {
            eprintln!("diple: {name}: invalid UTF-8, decoding lossily");
            String::from_utf8_lossy(error.as_bytes()).into_owned()
        }
    }
}

/// Build the diagram provider, skipping all Mermaid work (including the `mmdc`
/// `PATH` probe) for documents without diagrams — the startup budget.
fn build_diagrams(
    doc: &Document,
    cfg: &Config,
    caps: &Capabilities,
    width: usize,
) -> DiagramProvider {
    let has_diagrams = doc
        .walk()
        .any(|node| matches!(node.kind, NodeKind::Mermaid(_)));
    if !has_diagrams {
        return DiagramProvider::source_only();
    }
    let env = app::render_environment(cfg, caps, width, true);
    DiagramProvider::new(select_backend(&cfg.mermaid, &env))
}

/// Non-interactive output: the plain rendered document on stdout.
///
/// This is what makes `diple file.md | head` and CI usage work. A closed
/// stdout is silently accepted.
fn print_plain(
    doc: &Document,
    cfg: &Config,
    caps: &Capabilities,
    width: Option<u16>,
) -> Result<(), Failure> {
    let color = app::color_level(cfg.color, caps);
    let theme = app::resolve_theme(&cfg.theme, color);
    let mut width = usize::from(width.unwrap_or(caps.size.0)).max(1);
    // `max_width` narrows this path too, so `diple doc.md | less -R` gets the
    // same measure as the interactive view.
    if cfg.max_width > 0 {
        width = width.min(usize::from(cfg.max_width));
    }
    let diagrams = build_diagrams(doc, cfg, caps, width);

    let mut opts = LayoutOptions::new(width, &theme);
    opts.apply_config(cfg);
    opts.diagrams = &diagrams;
    opts.images = false;
    opts.unicode = caps.unicode_box;
    // Spelled out rather than left to the default, because the interactive
    // path sets it explicitly and a silent disagreement between the two is
    // exactly what `apply_config` exists to prevent. `true` is the right value
    // here: this path has no `[n]`/jump affordance, so a footnote reference
    // whose definition section were dropped would be unreachable text.
    opts.footnotes = true;
    // Without colour the styling is dropped anyway (`to_ansi_text` returns
    // exactly `to_plain_text` at `ColorLevel::None`), so highlighting the
    // document would cost tens of milliseconds per language for output nobody
    // can tell apart. `--color always` still highlights everything.
    opts.lazy_code = color == diple::render::theme::ColorLevel::None;
    let tree = Layout::build(doc, &opts);

    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    // `--color always` must emit styling here too, so that `diple doc.md
    // --color always | less -R` works. `never` and `auto` on a non-terminal
    // stdout resolve to `ColorLevel::None`, for which `to_ansi_text` returns
    // exactly `to_plain_text` — no escape can leak .
    let rendered = diple::render::to_ansi_text(&tree, color);
    match out.write_all(rendered.as_bytes()) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => return Ok(()),
        Err(e) => return Err(Failure::from(e)),
    }
    match out.flush() {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
        Err(e) => Err(Failure::from(e)),
    }
}
