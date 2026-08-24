//! Terminal capability detection.
//!
//! # Why environment variables only
//!
//! Detection is **purely** based on environment variables, the `isatty` state of
//! the standard streams and the terminal size reported by `ioctl`. mdless
//! deliberately never writes a query escape sequence (`DA1`, `XTGETTCAP`,
//! `CSI 16 t`, …) to the terminal during startup:
//!
//! * A query must be followed by a *read* with a timeout. Terminals that do not
//!   answer cost us the full timeout, which alone would blow the sub-30 ms
//!   startup budget.
//! * If the reply arrives late (slow SSH link, tmux without passthrough) the
//!   response bytes are delivered to the shell after mdless exits, or are mixed
//!   into the document output — visible corruption.
//! * Queries are unsafe while stdin is a pipe (`cat x.md | mdless`), which is an
//!   explicitly supported mode.
//!
//! The cost of that decision is a slightly coarser result; we therefore
//! **always fail toward the conservative mode** and let the user override
//! anything through configuration, `MDLESS_*` variables or CLI flags, and
//! inspect the outcome with `mdless --print-capabilities`.

use std::collections::HashMap;
use std::fmt;

use crate::config::schema::{ColorMode, ImageMode, Osc8Mode};
use crate::util::DEFAULT_CELL_PIXELS;

/// How much colour the terminal can display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum ColorLevel {
    /// Monochrome: no SGR colour sequences at all.
    #[default]
    None,
    /// The 8/16 basic ANSI colours.
    Ansi16,
    /// The 256-colour palette (`38;5;n`).
    Ansi256,
    /// 24-bit RGB (`38;2;r;g;b`).
    TrueColor,
}

impl ColorLevel {
    /// Canonical lower-case name used in reports and `MDLESS_COLOR`.
    pub fn as_str(self) -> &'static str {
        match self {
            ColorLevel::None => "none",
            ColorLevel::Ansi16 => "ansi16",
            ColorLevel::Ansi256 => "ansi256",
            ColorLevel::TrueColor => "truecolor",
        }
    }

    /// Parse a level name (`none`, `16`, `ansi16`, `256`, `ansi256`,
    /// `truecolor`, `24bit`). Returns [`None`] for anything else.
    pub fn parse(s: &str) -> Option<ColorLevel> {
        match s.trim().to_ascii_lowercase().as_str() {
            "none" | "off" | "never" | "0" => Some(ColorLevel::None),
            "16" | "ansi16" | "ansi" | "basic" => Some(ColorLevel::Ansi16),
            "256" | "ansi256" | "256color" => Some(ColorLevel::Ansi256),
            "truecolor" | "24bit" | "rgb" => Some(ColorLevel::TrueColor),
            _ => None,
        }
    }
}

impl fmt::Display for ColorLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Which inline-image protocol the terminal understands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ImageSupport {
    /// No inline images; callers must fall back to text.
    #[default]
    None,
    /// Kitty graphics protocol (APC `_G…`).
    Kitty,
    /// DEC Sixel graphics.
    Sixel,
    /// iTerm2 inline images (OSC 1337).
    Iterm2,
}

impl ImageSupport {
    /// Canonical lower-case name used in reports and `MDLESS_IMAGES`.
    pub fn as_str(self) -> &'static str {
        match self {
            ImageSupport::None => "none",
            ImageSupport::Kitty => "kitty",
            ImageSupport::Sixel => "sixel",
            ImageSupport::Iterm2 => "iterm2",
        }
    }

    /// Parse a protocol name; [`None`] for unknown values.
    pub fn parse(s: &str) -> Option<ImageSupport> {
        match s.trim().to_ascii_lowercase().as_str() {
            "none" | "off" | "never" | "0" => Some(ImageSupport::None),
            "kitty" => Some(ImageSupport::Kitty),
            "sixel" => Some(ImageSupport::Sixel),
            "iterm2" | "iterm" => Some(ImageSupport::Iterm2),
            _ => None,
        }
    }

    /// Whether any inline-image protocol is available.
    pub fn is_some(self) -> bool {
        self != ImageSupport::None
    }
}

impl fmt::Display for ImageSupport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Terminal geometry passed into the pure detection core.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalSize {
    /// Width in cells.
    pub cols: u16,
    /// Height in cells.
    pub rows: u16,
    /// Size of one cell in pixels, if the terminal reported it.
    pub cell_pixels: Option<(u16, u16)>,
    /// Whether the output stream is a terminal.
    pub is_tty: bool,
}

impl Default for TerminalSize {
    fn default() -> Self {
        Self {
            cols: 80,
            rows: 24,
            cell_pixels: None,
            is_tty: true,
        }
    }
}

impl TerminalSize {
    /// A non-interactive fallback: 80×24, not a tty.
    pub fn non_tty() -> Self {
        Self {
            is_tty: false,
            ..Self::default()
        }
    }

    /// Query the real terminal via crossterm, falling back to 80×24.
    ///
    /// Never fails: an unavailable `ioctl` yields the conservative default.
    pub fn query(is_tty: bool) -> Self {
        let mut size = TerminalSize {
            cols: 80,
            rows: 24,
            cell_pixels: None,
            is_tty,
        };
        if let Ok((cols, rows)) = crossterm::terminal::size() {
            if cols > 0 && rows > 0 {
                size.cols = cols;
                size.rows = rows;
            }
        }
        if let Ok(ws) = crossterm::terminal::window_size() {
            if ws.width > 0 && ws.height > 0 && ws.columns > 0 && ws.rows > 0 {
                size.cell_pixels = Some((ws.width / ws.columns, ws.height / ws.rows));
            }
        }
        size
    }
}

/// One decision made by [`detect_from`], for `--print-capabilities`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Evidence {
    /// Capability name, e.g. `color`.
    pub capability: &'static str,
    /// The resulting value, e.g. `truecolor`.
    pub value: String,
    /// What decided it, e.g. `COLORTERM=truecolor`.
    pub reason: String,
}

impl Evidence {
    fn new(capability: &'static str, value: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            capability,
            value: value.into(),
            reason: reason.into(),
        }
    }
}

/// Explicit overrides applied on top of detection.
///
/// Values come from the configuration file (`color`, `links.osc8`,
/// `mermaid.images`) and from the CLI; `MDLESS_*` variables are handled inside
/// [`detect_from`] because they are part of the environment.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CapabilityOverrides {
    /// `color` / `--color`.
    pub color: Option<ColorMode>,
    /// Force an exact colour level (used by `MDLESS_COLOR=256`).
    pub color_level: Option<ColorLevel>,
    /// `links.osc8`.
    pub osc8: Option<Osc8Mode>,
    /// `mermaid.images` / `--mermaid-images`.
    pub images: Option<ImageMode>,
    /// Force a specific image protocol.
    pub image_protocol: Option<ImageSupport>,
    /// `mouse` / `--mouse` / `--no-mouse`.
    pub mouse: Option<bool>,
    /// `--width`.
    pub width: Option<u16>,
    /// Force Unicode box drawing on or off.
    pub unicode_box: Option<bool>,
    /// Assume tmux passthrough is enabled (`set -g allow-passthrough on`).
    pub allow_passthrough: Option<bool>,
}

impl CapabilityOverrides {
    /// Build the overrides implied by a loaded [`Config`](crate::config::Config).
    pub fn from_config(cfg: &crate::config::Config) -> Self {
        Self {
            color: Some(cfg.color),
            osc8: Some(cfg.links.osc8),
            images: Some(cfg.mermaid.images),
            mouse: Some(cfg.mouse),
            ..Self::default()
        }
    }
}

/// The detected terminal capabilities.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Capabilities {
    /// Colour depth.
    pub color: ColorLevel,
    /// Whether Unicode box-drawing characters are safe to emit.
    pub unicode_box: bool,
    /// Whether OSC 8 hyperlinks are supported.
    pub osc8: bool,
    /// Inline-image protocol.
    pub images: ImageSupport,
    /// Whether mouse reporting may be enabled.
    pub mouse: bool,
    /// Running inside tmux (or GNU screen).
    pub tmux: bool,
    /// Running over SSH.
    pub ssh: bool,
    /// Terminal size in cells, `(cols, rows)`.
    pub size: (u16, u16),
    /// Size of a single cell in pixels, when known.
    pub cell_pixels: Option<(u16, u16)>,
    /// Whether output goes to a terminal at all.
    pub is_tty: bool,
    /// Whether tmux passthrough (`allow-passthrough`) may be assumed.
    pub tmux_passthrough: bool,
    /// The terminal identity used for the decisions (`TERM_PROGRAM` or `TERM`).
    pub terminal: String,
    /// Per-capability evidence, in report order.
    pub evidence: Vec<Evidence>,
}

impl Default for Capabilities {
    fn default() -> Self {
        Self {
            color: ColorLevel::None,
            unicode_box: false,
            osc8: false,
            images: ImageSupport::None,
            mouse: false,
            tmux: false,
            ssh: false,
            size: (80, 24),
            cell_pixels: None,
            is_tty: false,
            tmux_passthrough: false,
            terminal: "unknown".to_string(),
            evidence: Vec::new(),
        }
    }
}

/// Detect capabilities from the real process environment and terminal.
///
/// Thin wrapper over [`detect_from`]; all logic lives there so it can be
/// exercised with synthetic environments in unit tests.
pub fn detect(overrides: &CapabilityOverrides) -> Capabilities {
    let env: HashMap<String, String> = std::env::vars().collect();
    let is_tty = crate::terminal::lifecycle::stdout_is_tty();
    detect_from(&env, TerminalSize::query(is_tty), overrides)
}

/// The pure detection core: environment map + geometry + overrides → result.
pub fn detect_from(
    env: &HashMap<String, String>,
    size: TerminalSize,
    overrides: &CapabilityOverrides,
) -> Capabilities {
    let get = |k: &str| env.get(k).map(String::as_str);
    let term = get("TERM").unwrap_or("").to_ascii_lowercase();
    let colorterm = get("COLORTERM").unwrap_or("").to_ascii_lowercase();
    let term_program = get("TERM_PROGRAM").unwrap_or("").to_string();
    let program = term_program.to_ascii_lowercase();

    let mut ev: Vec<Evidence> = Vec::new();

    // ---- multiplexer / remote ------------------------------------------------
    let (tmux, mux_reason) = if env.contains_key("TMUX") {
        (true, "TMUX is set".to_string())
    } else if term.starts_with("tmux") {
        (true, format!("TERM={term}"))
    } else if env.contains_key("STY") || term.starts_with("screen") {
        (true, "GNU screen (STY/TERM=screen*)".to_string())
    } else {
        (false, "no TMUX/STY, TERM is not a multiplexer".to_string())
    };
    ev.push(Evidence::new("multiplexer", tmux.to_string(), mux_reason));

    let (ssh, ssh_reason) = if env.contains_key("SSH_TTY") {
        (true, "SSH_TTY is set".to_string())
    } else if env.contains_key("SSH_CONNECTION") {
        (true, "SSH_CONNECTION is set".to_string())
    } else {
        (false, "no SSH_TTY/SSH_CONNECTION".to_string())
    };
    ev.push(Evidence::new("ssh", ssh.to_string(), ssh_reason));

    let passthrough = match overrides.allow_passthrough {
        Some(v) => v,
        None => truthy(get("MDLESS_TMUX_PASSTHROUGH")) || truthy(get("MDLESS_FORCE_IMAGES")),
    };
    if tmux {
        ev.push(Evidence::new(
            "tmux-passthrough",
            passthrough.to_string(),
            if passthrough {
                "forced (MDLESS_TMUX_PASSTHROUGH/MDLESS_FORCE_IMAGES or config)"
            } else {
                "not assumed; tmux needs `set -g allow-passthrough on`"
            },
        ));
    }

    // ---- terminal identity ---------------------------------------------------
    let ident = TerminalIdent::detect(env, &term, &program);
    ev.push(Evidence::new("terminal", ident.name, ident.reason.clone()));

    // ---- tty -----------------------------------------------------------------
    ev.push(Evidence::new(
        "tty",
        size.is_tty.to_string(),
        if size.is_tty {
            "stdout is a terminal"
        } else {
            "stdout is not a terminal (piped/redirected)"
        },
    ));

    // ---- colour --------------------------------------------------------------
    let (mut color, mut color_reason) = detect_color(&term, &colorterm, &ident, size.is_tty, env);

    // Overrides, most specific last.
    if let Some(level) = ColorLevel::parse(get("MDLESS_COLOR").unwrap_or("")) {
        color = level;
        color_reason = format!("MDLESS_COLOR={}", get("MDLESS_COLOR").unwrap_or(""));
    }
    if let Some(level) = overrides.color_level {
        color = level;
        color_reason = "explicit override".to_string();
    }
    match overrides.color {
        Some(ColorMode::Never) => {
            color = ColorLevel::None;
            color_reason = "configuration/CLI: color = never".to_string();
        }
        Some(ColorMode::Always) if color == ColorLevel::None => {
            color = ColorLevel::Ansi16;
            color_reason = "configuration/CLI: color = always (conservative ansi16)".to_string();
        }
        _ => {}
    }
    ev.push(Evidence::new("color", color.as_str(), color_reason));

    // ---- Unicode -------------------------------------------------------------
    let (mut unicode_box, mut unicode_reason) = detect_unicode(env, &term);
    if let Some(v) = parse_bool(get("MDLESS_UNICODE")) {
        unicode_box = v;
        unicode_reason = "MDLESS_UNICODE".to_string();
    }
    if let Some(v) = overrides.unicode_box {
        unicode_box = v;
        unicode_reason = "explicit override".to_string();
    }
    ev.push(Evidence::new(
        "unicode-box",
        unicode_box.to_string(),
        unicode_reason,
    ));

    // ---- OSC 8 ---------------------------------------------------------------
    let (mut osc8, mut osc8_reason) = detect_osc8(&ident, env, tmux, passthrough, size.is_tty);
    if let Some(v) = parse_bool(get("MDLESS_OSC8")) {
        osc8 = v;
        osc8_reason = "MDLESS_OSC8".to_string();
    }
    match overrides.osc8 {
        Some(Osc8Mode::Never) => {
            osc8 = false;
            osc8_reason = "configuration/CLI: links.osc8 = never".to_string();
        }
        Some(Osc8Mode::Always) => {
            osc8 = true;
            osc8_reason = "configuration/CLI: links.osc8 = always".to_string();
        }
        _ => {}
    }
    if color == ColorLevel::None && !size.is_tty {
        osc8 = false;
        osc8_reason = "not a terminal".to_string();
    }
    ev.push(Evidence::new("osc8", osc8.to_string(), osc8_reason));

    // ---- images --------------------------------------------------------------
    let (mut images, mut images_reason) = detect_images(&ident, env, &term, size.is_tty);
    if tmux && images.is_some() && !passthrough {
        images = ImageSupport::None;
        images_reason =
            "inside tmux without passthrough; enable with `set -g allow-passthrough on` \
             or MDLESS_FORCE_IMAGES=1"
                .to_string();
    }
    if let Some(forced) = ImageSupport::parse(get("MDLESS_IMAGES").unwrap_or("")) {
        images = forced;
        images_reason = format!("MDLESS_IMAGES={}", get("MDLESS_IMAGES").unwrap_or(""));
    }
    if let Some(forced) = overrides.image_protocol {
        images = forced;
        images_reason = "explicit override".to_string();
    }
    match overrides.images {
        Some(ImageMode::Never) => {
            images = ImageSupport::None;
            images_reason = "configuration/CLI: mermaid.images = never".to_string();
        }
        Some(ImageMode::Always) if images == ImageSupport::None => {
            // "always" cannot invent a protocol; keep the conservative result
            // but record why nothing happened (fail conservative).
            images_reason = format!("{images_reason} (mermaid.images = always: no protocol found)");
        }
        _ => {}
    }
    if !size.is_tty && !truthy(get("MDLESS_FORCE_IMAGES")) {
        images = ImageSupport::None;
        images_reason = "stdout is not a terminal".to_string();
    }
    ev.push(Evidence::new("images", images.as_str(), images_reason));

    // ---- mouse ---------------------------------------------------------------
    // A request to enable the mouse can never override the absence of a
    // terminal, so overrides only ever narrow the detected value.
    let mouse_capable = size.is_tty && term != "dumb" && !term.is_empty();
    let mut mouse = mouse_capable;
    let mut mouse_reason = if mouse {
        "terminal supports X10/SGR mouse reporting".to_string()
    } else if !size.is_tty {
        "stdout is not a terminal".to_string()
    } else {
        format!("TERM={term} cannot report mouse events")
    };
    for (requested, source) in [
        (parse_bool(get("MDLESS_MOUSE")), "MDLESS_MOUSE"),
        (overrides.mouse, "configuration/CLI: mouse"),
    ] {
        let Some(requested) = requested else { continue };
        mouse = requested && mouse_capable;
        mouse_reason = if requested && !mouse_capable {
            format!("{source} requested it, but the terminal cannot report mouse events")
        } else {
            source.to_string()
        };
    }
    ev.push(Evidence::new("mouse", mouse.to_string(), mouse_reason));

    // ---- geometry ------------------------------------------------------------
    let cols = overrides.width.filter(|w| *w > 0).unwrap_or(size.cols);
    ev.push(Evidence::new(
        "size",
        format!("{}x{}", cols, size.rows),
        match overrides.width {
            Some(w) if w > 0 => format!("--width={w}"),
            _ => "ioctl(TIOCGWINSZ) via crossterm".to_string(),
        },
    ));
    ev.push(Evidence::new(
        "cell-pixels",
        match size.cell_pixels {
            Some((w, h)) => format!("{w}x{h}"),
            None => "unknown (assuming 8x16)".to_string(),
        },
        "crossterm window_size()",
    ));

    Capabilities {
        color,
        unicode_box,
        osc8,
        images,
        mouse,
        tmux,
        ssh,
        size: (cols, size.rows),
        cell_pixels: size.cell_pixels,
        is_tty: size.is_tty,
        tmux_passthrough: tmux && passthrough,
        terminal: ident.name.to_string(),
        evidence: ev,
    }
}

/// A recognised terminal emulator family.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Family {
    Kitty,
    WezTerm,
    Iterm2,
    AppleTerminal,
    Konsole,
    Foot,
    Mlterm,
    Contour,
    Vte,
    WindowsTerminal,
    Alacritty,
    Xterm,
    Dumb,
    Unknown,
}

#[derive(Debug, Clone)]
struct TerminalIdent {
    family: Family,
    name: &'static str,
    reason: String,
    /// Parsed emulator version, when the environment exposes one.
    version: Option<(u32, u32)>,
}

impl TerminalIdent {
    fn detect(env: &HashMap<String, String>, term: &str, program: &str) -> Self {
        let get = |k: &str| env.get(k).map(String::as_str);
        let mk =
            |family: Family, name: &'static str, reason: String, version: Option<(u32, u32)>| {
                TerminalIdent {
                    family,
                    name,
                    reason,
                    version,
                }
            };
        if term == "dumb" {
            return mk(Family::Dumb, "dumb", "TERM=dumb".to_string(), None);
        }
        if env.contains_key("KITTY_WINDOW_ID") {
            return mk(
                Family::Kitty,
                "kitty",
                "KITTY_WINDOW_ID is set".to_string(),
                None,
            );
        }
        if term.contains("kitty") {
            return mk(Family::Kitty, "kitty", format!("TERM={term}"), None);
        }
        if env.contains_key("WEZTERM_EXECUTABLE") || program == "wezterm" {
            return mk(
                Family::WezTerm,
                "wezterm",
                "WEZTERM_EXECUTABLE/TERM_PROGRAM=WezTerm".to_string(),
                None,
            );
        }
        if program == "iterm.app" {
            let version = get("TERM_PROGRAM_VERSION").and_then(parse_version);
            return mk(
                Family::Iterm2,
                "iterm2",
                "TERM_PROGRAM=iTerm.app".to_string(),
                version,
            );
        }
        if program == "apple_terminal" {
            return mk(
                Family::AppleTerminal,
                "apple-terminal",
                "TERM_PROGRAM=Apple_Terminal".to_string(),
                None,
            );
        }
        if let Some(v) = get("KONSOLE_VERSION") {
            return mk(
                Family::Konsole,
                "konsole",
                format!("KONSOLE_VERSION={v}"),
                parse_konsole_version(v),
            );
        }
        if term.starts_with("foot") {
            return mk(Family::Foot, "foot", format!("TERM={term}"), None);
        }
        if term.starts_with("mlterm") {
            return mk(Family::Mlterm, "mlterm", format!("TERM={term}"), None);
        }
        if term.starts_with("contour") || program == "contour" {
            return mk(Family::Contour, "contour", "contour".to_string(), None);
        }
        if env.contains_key("WT_SESSION") {
            return mk(
                Family::WindowsTerminal,
                "windows-terminal",
                "WT_SESSION is set".to_string(),
                None,
            );
        }
        if term.starts_with("alacritty") || env.contains_key("ALACRITTY_WINDOW_ID") {
            return mk(
                Family::Alacritty,
                "alacritty",
                "TERM=alacritty*/ALACRITTY_WINDOW_ID".to_string(),
                None,
            );
        }
        if let Some(v) = get("VTE_VERSION") {
            return mk(
                Family::Vte,
                "vte",
                format!("VTE_VERSION={v}"),
                parse_vte_version(v),
            );
        }
        if term.starts_with("xterm") {
            return mk(Family::Xterm, "xterm", format!("TERM={term}"), None);
        }
        if term.is_empty() {
            return mk(
                Family::Unknown,
                "unknown",
                "TERM is unset".to_string(),
                None,
            );
        }
        mk(Family::Unknown, "unknown", format!("TERM={term}"), None)
    }
}

fn detect_color(
    term: &str,
    colorterm: &str,
    ident: &TerminalIdent,
    is_tty: bool,
    env: &HashMap<String, String>,
) -> (ColorLevel, String) {
    // NO_COLOR wins over everything detected (https://no-color.org).
    if env.contains_key("NO_COLOR") {
        return (ColorLevel::None, "NO_COLOR is set".to_string());
    }
    if env
        .get("CLICOLOR_FORCE")
        .is_some_and(|v| !v.is_empty() && v != "0")
    {
        // Forced even when not a tty; depth still comes from the usual sources.
        let (level, why) = color_depth(term, colorterm, ident);
        return (
            level.max(ColorLevel::Ansi16),
            format!("CLICOLOR_FORCE ({why})"),
        );
    }
    if !is_tty {
        return (ColorLevel::None, "stdout is not a terminal".to_string());
    }
    if env.get("CLICOLOR").is_some_and(|v| v == "0") {
        return (ColorLevel::None, "CLICOLOR=0".to_string());
    }
    if ident.family == Family::Dumb {
        return (ColorLevel::None, "TERM=dumb".to_string());
    }
    let (level, why) = color_depth(term, colorterm, ident);
    (level, why)
}

fn color_depth(term: &str, colorterm: &str, ident: &TerminalIdent) -> (ColorLevel, String) {
    if colorterm == "truecolor" || colorterm == "24bit" {
        return (ColorLevel::TrueColor, format!("COLORTERM={colorterm}"));
    }
    if term.contains("truecolor") || term.contains("direct") {
        return (ColorLevel::TrueColor, format!("TERM={term}"));
    }
    if matches!(
        ident.family,
        Family::Kitty | Family::WezTerm | Family::Iterm2 | Family::Foot | Family::Contour
    ) {
        return (
            ColorLevel::TrueColor,
            format!("known true-colour terminal: {}", ident.name),
        );
    }
    if term.contains("256color") {
        return (ColorLevel::Ansi256, format!("TERM={term}"));
    }
    if !colorterm.is_empty() {
        return (ColorLevel::Ansi16, format!("COLORTERM={colorterm}"));
    }
    if term.is_empty() {
        return (ColorLevel::None, "TERM is unset".to_string());
    }
    (ColorLevel::Ansi16, format!("TERM={term}"))
}

fn detect_unicode(env: &HashMap<String, String>, term: &str) -> (bool, String) {
    for var in ["LC_ALL", "LC_CTYPE", "LANG"] {
        if let Some(value) = env.get(var) {
            if value.is_empty() {
                continue;
            }
            let up = value.to_ascii_uppercase();
            if up.contains("UTF-8") || up.contains("UTF8") {
                return (true, format!("{var}={value}"));
            }
            if up == "C" || up == "POSIX" {
                return (false, format!("{var}={value} (not UTF-8)"));
            }
            return (false, format!("{var}={value} (not UTF-8)"));
        }
    }
    if term == "dumb" {
        return (false, "TERM=dumb".to_string());
    }
    (
        false,
        "no LC_ALL/LC_CTYPE/LANG announcing UTF-8 (conservative default)".to_string(),
    )
}

fn detect_osc8(
    ident: &TerminalIdent,
    env: &HashMap<String, String>,
    tmux: bool,
    passthrough: bool,
    is_tty: bool,
) -> (bool, String) {
    if !is_tty {
        return (false, "stdout is not a terminal".to_string());
    }
    let supported = match ident.family {
        Family::Kitty | Family::WezTerm | Family::Iterm2 | Family::Foot | Family::Contour => {
            (true, format!("{} supports OSC 8", ident.name))
        }
        Family::Vte => match ident.version {
            // VTE >= 0.50 (VTE_VERSION 5000) implements OSC 8.
            Some((major, minor)) if (major, minor) >= (0, 50) => (
                true,
                format!(
                    "VTE_VERSION={} (>= 0.50)",
                    env.get("VTE_VERSION").map(String::as_str).unwrap_or("?")
                ),
            ),
            _ => (false, "VTE older than 0.50".to_string()),
        },
        Family::Konsole => match ident.version {
            Some((major, _)) if major >= 20 => {
                (true, format!("Konsole {major} (>= 20) supports OSC 8"))
            }
            _ => (false, "Konsole older than 20.x".to_string()),
        },
        Family::WindowsTerminal => (true, "Windows Terminal supports OSC 8".to_string()),
        Family::AppleTerminal => (false, "Apple Terminal has no OSC 8".to_string()),
        Family::Alacritty => (false, "Alacritty support is version dependent".to_string()),
        Family::Xterm | Family::Mlterm | Family::Dumb | Family::Unknown => (
            false,
            format!("no evidence of OSC 8 support for {}", ident.name),
        ),
    };
    if tmux && !passthrough {
        return (
            false,
            "inside tmux without `allow-passthrough on`".to_string(),
        );
    }
    supported
}

fn detect_images(
    ident: &TerminalIdent,
    env: &HashMap<String, String>,
    term: &str,
    is_tty: bool,
) -> (ImageSupport, String) {
    if !is_tty {
        return (ImageSupport::None, "stdout is not a terminal".to_string());
    }
    if env.contains_key("KITTY_WINDOW_ID") || term.contains("kitty") {
        return (
            ImageSupport::Kitty,
            "KITTY_WINDOW_ID / TERM=*kitty*".to_string(),
        );
    }
    match ident.family {
        // WezTerm implements both; the iTerm2 protocol is the better tested one.
        Family::WezTerm => (
            ImageSupport::Iterm2,
            "WezTerm implements the iTerm2 inline-image protocol".to_string(),
        ),
        Family::Iterm2 => (ImageSupport::Iterm2, "TERM_PROGRAM=iTerm.app".to_string()),
        Family::Foot => (ImageSupport::Sixel, "foot supports Sixel".to_string()),
        Family::Mlterm => (ImageSupport::Sixel, "mlterm supports Sixel".to_string()),
        Family::Contour => (ImageSupport::Sixel, "contour supports Sixel".to_string()),
        _ if term.contains("sixel") => {
            (ImageSupport::Sixel, format!("TERM={term} announces sixel"))
        }
        _ => (
            ImageSupport::None,
            format!("no known image protocol for {}", ident.name),
        ),
    }
}

fn truthy(value: Option<&str>) -> bool {
    matches!(
        value.map(|v| v.trim().to_ascii_lowercase()),
        Some(ref v) if v == "1" || v == "true" || v == "yes" || v == "on"
    )
}

fn parse_bool(value: Option<&str>) -> Option<bool> {
    match value?.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" | "always" => Some(true),
        "0" | "false" | "no" | "off" | "never" => Some(false),
        _ => None,
    }
}

/// `"3.4.19"` → `(3, 4)`.
fn parse_version(value: &str) -> Option<(u32, u32)> {
    let mut parts = value.trim().split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
    Some((major, minor))
}

/// `VTE_VERSION` is `MMmmpp` scaled, e.g. `6003` → 0.60.3, `5202` → 0.52.2.
fn parse_vte_version(value: &str) -> Option<(u32, u32)> {
    let n: u32 = value.trim().parse().ok()?;
    Some((0, n / 100))
}

/// `KONSOLE_VERSION` is e.g. `211200` (21.12.0) or `201204`.
fn parse_konsole_version(value: &str) -> Option<(u32, u32)> {
    let n: u32 = value.trim().parse().ok()?;
    if n >= 100_000 {
        Some((n / 10_000, (n / 100) % 100))
    } else {
        // Older Konsole reported e.g. `18.12.2` style dotted versions.
        parse_version(value)
    }
}

impl Capabilities {
    /// Cell size in pixels, falling back to [`DEFAULT_CELL_PIXELS`].
    pub fn cell_size(&self) -> (u16, u16) {
        self.cell_pixels
            .filter(|(w, h)| *w > 0 && *h > 0)
            .unwrap_or(DEFAULT_CELL_PIXELS)
    }

    /// The human-readable `--print-capabilities` report.
    ///
    /// Every capability is listed with its value and the evidence that decided
    /// it, so users can diagnose why a feature is off.
    pub fn describe(&self) -> String {
        let mut out = String::new();
        out.push_str("mdless terminal capabilities\n");
        out.push_str(
            "(detection is environment-based only; mdless never queries the terminal)\n\n",
        );
        let width = self
            .evidence
            .iter()
            .map(|e| e.capability.len())
            .max()
            .unwrap_or(10)
            .max(10);
        let value_width = self
            .evidence
            .iter()
            .map(|e| e.value.len())
            .max()
            .unwrap_or(8)
            .min(24);
        for e in &self.evidence {
            out.push_str(&format!(
                "  {:<cap$}  {:<val$}  {}\n",
                e.capability,
                e.value,
                e.reason,
                cap = width,
                val = value_width
            ));
        }
        out.push('\n');
        out.push_str("summary\n");
        out.push_str(&format!("  color        {}\n", self.color));
        out.push_str(&format!("  unicode box  {}\n", yes_no(self.unicode_box)));
        out.push_str(&format!("  osc 8 links  {}\n", yes_no(self.osc8)));
        out.push_str(&format!("  images       {}\n", self.images));
        out.push_str(&format!("  mouse        {}\n", yes_no(self.mouse)));
        out.push_str(&format!("  tmux         {}\n", yes_no(self.tmux)));
        out.push_str(&format!("  ssh          {}\n", yes_no(self.ssh)));
        out.push_str(&format!("  size         {}x{}\n", self.size.0, self.size.1));
        let (cw, ch) = self.cell_size();
        out.push_str(&format!(
            "  cell size    {cw}x{ch} px{}\n",
            if self.cell_pixels.is_some() {
                ""
            } else {
                " (assumed)"
            }
        ));
        if self.tmux && !self.tmux_passthrough {
            out.push_str(
                "\nnote: inside tmux, images and OSC 8 require `set -g allow-passthrough on`\n",
            );
        }
        out
    }
}

fn yes_no(v: bool) -> &'static str {
    if v {
        "yes"
    } else {
        "no"
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    use crate::testing::{caps, env_map as env};

    #[test]
    fn color_detection_matrix() {
        let cases: &[(&[(&str, &str)], ColorLevel)] = &[
            (&[("TERM", "xterm-256color")], ColorLevel::Ansi256),
            (
                &[("TERM", "xterm-256color"), ("COLORTERM", "truecolor")],
                ColorLevel::TrueColor,
            ),
            (
                &[("TERM", "xterm"), ("COLORTERM", "24bit")],
                ColorLevel::TrueColor,
            ),
            (&[("TERM", "xterm")], ColorLevel::Ansi16),
            (&[("TERM", "dumb")], ColorLevel::None),
            (&[], ColorLevel::None),
            (&[("TERM", "xterm-kitty")], ColorLevel::TrueColor),
            (&[("TERM", "screen-256color")], ColorLevel::Ansi256),
        ];
        for (pairs, expected) in cases {
            assert_eq!(caps(pairs).color, *expected, "case {pairs:?}");
        }
    }

    #[test]
    fn no_color_beats_colorterm() {
        let c = caps(&[
            ("TERM", "xterm-256color"),
            ("COLORTERM", "truecolor"),
            ("NO_COLOR", ""),
        ]);
        assert_eq!(c.color, ColorLevel::None);
        let c = caps(&[("COLORTERM", "truecolor"), ("NO_COLOR", "1")]);
        assert_eq!(c.color, ColorLevel::None);
        assert!(c
            .evidence
            .iter()
            .any(|e| e.capability == "color" && e.reason.contains("NO_COLOR")));
    }

    #[test]
    fn not_a_tty_disables_everything_optional() {
        let c = detect_from(
            &env(&[("TERM", "xterm-kitty"), ("KITTY_WINDOW_ID", "1")]),
            TerminalSize::non_tty(),
            &CapabilityOverrides::default(),
        );
        assert_eq!(c.color, ColorLevel::None);
        assert_eq!(c.images, ImageSupport::None);
        assert!(!c.osc8);
        assert!(!c.mouse);
    }

    #[test]
    fn clicolor_force_enables_color_without_tty() {
        let c = detect_from(
            &env(&[("TERM", "xterm-256color"), ("CLICOLOR_FORCE", "1")]),
            TerminalSize::non_tty(),
            &CapabilityOverrides::default(),
        );
        assert_eq!(c.color, ColorLevel::Ansi256);
    }

    #[test]
    fn image_protocol_matrix() {
        let cases: &[(&[(&str, &str)], ImageSupport)] = &[
            (&[("TERM", "xterm-kitty")], ImageSupport::Kitty),
            (
                &[("TERM", "xterm-256color"), ("KITTY_WINDOW_ID", "3")],
                ImageSupport::Kitty,
            ),
            (
                &[("TERM", "xterm-256color"), ("TERM_PROGRAM", "iTerm.app")],
                ImageSupport::Iterm2,
            ),
            (
                &[("TERM", "xterm-256color"), ("WEZTERM_EXECUTABLE", "/x")],
                ImageSupport::Iterm2,
            ),
            (&[("TERM", "foot")], ImageSupport::Sixel),
            (&[("TERM", "mlterm")], ImageSupport::Sixel),
            (&[("TERM", "xterm-sixel")], ImageSupport::Sixel),
            (&[("TERM", "xterm-256color")], ImageSupport::None),
            (&[("TERM", "alacritty")], ImageSupport::None),
            (
                &[
                    ("TERM", "xterm-256color"),
                    ("TERM_PROGRAM", "Apple_Terminal"),
                ],
                ImageSupport::None,
            ),
        ];
        for (pairs, expected) in cases {
            assert_eq!(caps(pairs).images, *expected, "case {pairs:?}");
        }
    }

    #[test]
    fn tmux_disables_images_by_default() {
        let c = caps(&[
            ("TERM", "xterm-kitty"),
            ("TMUX", "/tmp/tmux-1000/default,1,0"),
        ]);
        assert!(c.tmux);
        assert_eq!(c.images, ImageSupport::None);
        assert!(!c.osc8);

        // ... unless passthrough is asserted.
        let c = caps(&[
            ("TERM", "xterm-kitty"),
            ("TMUX", "/tmp/x,1,0"),
            ("MDLESS_FORCE_IMAGES", "1"),
        ]);
        assert_eq!(c.images, ImageSupport::Kitty);
        assert!(c.osc8);
    }

    #[test]
    fn screen_counts_as_multiplexer() {
        let c = caps(&[("TERM", "screen-256color"), ("STY", "1234.pts-0")]);
        assert!(c.tmux);
    }

    #[test]
    fn osc8_matrix() {
        let cases: &[(&[(&str, &str)], bool)] = &[
            (&[("TERM", "xterm-kitty")], true),
            (&[("TERM", "foot")], true),
            (&[("TERM", "xterm-256color"), ("VTE_VERSION", "6003")], true),
            (
                &[("TERM", "xterm-256color"), ("VTE_VERSION", "4600")],
                false,
            ),
            (
                &[("TERM", "xterm-256color"), ("KONSOLE_VERSION", "211200")],
                true,
            ),
            (
                &[("TERM", "xterm-256color"), ("KONSOLE_VERSION", "180804")],
                false,
            ),
            (&[("TERM", "xterm-256color")], false),
            (
                &[("TERM", "xterm-256color"), ("TERM_PROGRAM", "iTerm.app")],
                true,
            ),
        ];
        for (pairs, expected) in cases {
            assert_eq!(caps(pairs).osc8, *expected, "case {pairs:?}");
        }
    }

    #[test]
    fn unicode_from_locale() {
        assert!(caps(&[("TERM", "xterm"), ("LANG", "en_US.UTF-8")]).unicode_box);
        assert!(caps(&[("TERM", "xterm"), ("LC_ALL", "de_DE.utf8")]).unicode_box);
        assert!(!caps(&[("TERM", "xterm"), ("LANG", "C")]).unicode_box);
        assert!(!caps(&[("TERM", "xterm")]).unicode_box);
        // LC_ALL wins over LANG.
        assert!(!caps(&[("TERM", "xterm"), ("LC_ALL", "C"), ("LANG", "en_US.UTF-8")]).unicode_box);
    }

    #[test]
    fn ssh_detection() {
        assert!(caps(&[("SSH_TTY", "/dev/pts/3")]).ssh);
        assert!(caps(&[("SSH_CONNECTION", "1.2.3.4 5 6.7.8.9 22")]).ssh);
        assert!(!caps(&[("TERM", "xterm")]).ssh);
    }

    #[test]
    fn config_overrides_win() {
        let base = env(&[("TERM", "xterm-kitty"), ("KITTY_WINDOW_ID", "1")]);
        let o = CapabilityOverrides {
            color: Some(ColorMode::Never),
            osc8: Some(Osc8Mode::Never),
            images: Some(ImageMode::Never),
            mouse: Some(false),
            width: Some(120),
            ..CapabilityOverrides::default()
        };
        let c = detect_from(&base, TerminalSize::default(), &o);
        assert_eq!(c.color, ColorLevel::None);
        assert!(!c.osc8);
        assert_eq!(c.images, ImageSupport::None);
        assert!(!c.mouse);
        assert_eq!(c.size, (120, 24));

        let o = CapabilityOverrides {
            osc8: Some(Osc8Mode::Always),
            ..CapabilityOverrides::default()
        };
        let c = detect_from(&env(&[("TERM", "vt100")]), TerminalSize::default(), &o);
        assert!(c.osc8);
    }

    #[test]
    fn mdless_env_overrides() {
        let c = caps(&[("TERM", "xterm-256color"), ("MDLESS_COLOR", "truecolor")]);
        assert_eq!(c.color, ColorLevel::TrueColor);
        let c = caps(&[("TERM", "xterm-kitty"), ("MDLESS_IMAGES", "none")]);
        assert_eq!(c.images, ImageSupport::None);
        let c = caps(&[("TERM", "vt100"), ("MDLESS_OSC8", "1")]);
        assert!(c.osc8);
        let c = caps(&[("TERM", "vt100"), ("MDLESS_UNICODE", "1")]);
        assert!(c.unicode_box);
    }

    #[test]
    fn mouse_override_cannot_enable_an_incapable_terminal() {
        let o = CapabilityOverrides {
            mouse: Some(true),
            ..CapabilityOverrides::default()
        };
        let c = detect_from(&env(&[("TERM", "dumb")]), TerminalSize::default(), &o);
        assert!(!c.mouse);
        let c = detect_from(&env(&[("TERM", "xterm")]), TerminalSize::non_tty(), &o);
        assert!(!c.mouse);
        let c = detect_from(&env(&[("TERM", "xterm")]), TerminalSize::default(), &o);
        assert!(c.mouse);
    }

    #[test]
    fn describe_lists_every_capability() {
        let text = caps(&[("TERM", "xterm-256color"), ("LANG", "en_US.UTF-8")]).describe();
        for key in [
            "color",
            "unicode-box",
            "osc8",
            "images",
            "mouse",
            "ssh",
            "size",
            "cell-pixels",
            "terminal",
            "tty",
        ] {
            assert!(text.contains(key), "missing {key} in:\n{text}");
        }
        // Evidence must be present, not just values.
        assert!(text.contains("TERM=xterm-256color"));
    }

    #[test]
    fn version_parsers() {
        assert_eq!(parse_vte_version("6003"), Some((0, 60)));
        assert_eq!(parse_vte_version("5000"), Some((0, 50)));
        assert_eq!(parse_konsole_version("211200"), Some((21, 12)));
        assert_eq!(parse_version("3.4.19"), Some((3, 4)));
        assert_eq!(parse_version("nonsense"), None);
    }

    #[test]
    fn cell_size_falls_back() {
        let c = caps(&[("TERM", "xterm")]);
        assert_eq!(c.cell_size(), (8, 16));
        let c = detect_from(
            &env(&[("TERM", "xterm")]),
            TerminalSize {
                cell_pixels: Some((10, 20)),
                ..TerminalSize::default()
            },
            &CapabilityOverrides::default(),
        );
        assert_eq!(c.cell_size(), (10, 20));
    }
}
