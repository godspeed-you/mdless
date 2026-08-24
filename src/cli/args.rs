//! `mdless [OPTIONS] [FILE]` — argument definitions.

use std::path::PathBuf;

use clap::Parser;
use clap_complete::Shell;

use crate::config::schema::{ColorMode, ImageMode, MermaidBackend};

/// An interactive terminal Markdown reader.
#[derive(Debug, Clone, Parser, Default, PartialEq)]
#[command(
    name = "mdless",
    version,
    about = "An interactive terminal Markdown reader",
    long_about = "mdless is `less` for Markdown: an interactive terminal document viewer \
                  with semantic navigation, folding, rich tables and Mermaid diagrams. \
                  If FILE is omitted, mdless reads from stdin."
)]
pub struct CliArgs {
    /// Markdown file to read (stdin when omitted).
    pub file: Option<PathBuf>,

    // Appearance
    /// Colour theme.
    #[arg(long, value_name = "auto|dark|light|NAME", help_heading = "Appearance")]
    pub theme: Option<String>,
    /// Colour output.
    #[arg(long, value_enum, value_name = "WHEN", help_heading = "Appearance")]
    pub color: Option<ColorMode>,
    /// Override the detected terminal width.
    #[arg(long, value_name = "COLUMNS", help_heading = "Appearance")]
    pub width: Option<u16>,
    /// Enable mouse support.
    #[arg(long, overrides_with = "no_mouse", help_heading = "Appearance")]
    pub mouse: bool,
    /// Disable mouse support.
    #[arg(long, overrides_with = "mouse", help_heading = "Appearance")]
    pub no_mouse: bool,

    // Navigation and UI
    /// Show the table of contents on startup.
    #[arg(long, overrides_with = "no_toc", help_heading = "Navigation and UI")]
    pub toc: bool,
    /// Hide the table of contents on startup.
    #[arg(long, overrides_with = "toc", help_heading = "Navigation and UI")]
    pub no_toc: bool,
    /// Show the key hints sidebar on startup.
    #[arg(
        long,
        overrides_with = "no_key_hints",
        help_heading = "Navigation and UI"
    )]
    pub key_hints: bool,
    /// Hide the key hints sidebar on startup.
    #[arg(long, overrides_with = "key_hints", help_heading = "Navigation and UI")]
    pub no_key_hints: bool,
    /// Show line numbers.
    #[arg(
        long,
        overrides_with = "no_line_numbers",
        help_heading = "Navigation and UI"
    )]
    pub line_numbers: bool,
    /// Hide line numbers.
    #[arg(
        long,
        overrides_with = "line_numbers",
        help_heading = "Navigation and UI"
    )]
    pub no_line_numbers: bool,
    /// Wrap long lines.
    #[arg(long, overrides_with = "no_wrap", help_heading = "Navigation and UI")]
    pub wrap: bool,
    /// Do not wrap long lines.
    #[arg(long, overrides_with = "wrap", help_heading = "Navigation and UI")]
    pub no_wrap: bool,

    // Mermaid
    /// Mermaid rendering backend.
    #[arg(long, value_enum, value_name = "BACKEND", help_heading = "Mermaid")]
    pub mermaid: Option<MermaidBackend>,
    /// Terminal image usage for Mermaid diagrams.
    #[arg(long, value_enum, value_name = "WHEN", help_heading = "Mermaid")]
    pub mermaid_images: Option<ImageMode>,

    // Configuration
    /// Use this configuration file.
    #[arg(
        long,
        value_name = "PATH",
        conflicts_with = "no_config",
        help_heading = "Configuration options"
    )]
    pub config: Option<PathBuf>,
    /// Ignore all configuration files.
    #[arg(long, help_heading = "Configuration options")]
    pub no_config: bool,

    // Diagnostics
    /// Print detected terminal capabilities and exit.
    #[arg(long, help_heading = "Diagnostics")]
    pub print_capabilities: bool,
    /// Validate the configuration and exit.
    #[arg(long, help_heading = "Diagnostics")]
    pub check_config: bool,
    /// Log debug information to stderr.
    #[arg(long, help_heading = "Diagnostics")]
    pub debug: bool,

    // Hidden (used by packaging)
    /// Generate a shell completion script on stdout and exit.
    #[arg(long, hide = true, value_enum, value_name = "SHELL")]
    pub generate_completions: Option<Shell>,
    /// Generate the man page on stdout and exit.
    #[arg(long, hide = true)]
    pub generate_man: bool,
}

impl CliArgs {
    /// `--mouse` / `--no-mouse` as an override (None: unset).
    pub fn mouse_override(&self) -> Option<bool> {
        flag_pair(self.mouse, self.no_mouse)
    }

    /// `--toc` / `--no-toc` as an override.
    pub fn toc_override(&self) -> Option<bool> {
        flag_pair(self.toc, self.no_toc)
    }

    /// `--key-hints` / `--no-key-hints` as an override.
    pub fn key_hints_override(&self) -> Option<bool> {
        flag_pair(self.key_hints, self.no_key_hints)
    }

    /// `--line-numbers` / `--no-line-numbers` as an override.
    pub fn line_numbers_override(&self) -> Option<bool> {
        flag_pair(self.line_numbers, self.no_line_numbers)
    }

    /// `--wrap` / `--no-wrap` as an override.
    pub fn wrap_override(&self) -> Option<bool> {
        flag_pair(self.wrap, self.no_wrap)
    }
}

fn flag_pair(yes: bool, no: bool) -> Option<bool> {
    match (yes, no) {
        (true, _) => Some(true),
        (_, true) => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    fn parse(args: &[&str]) -> CliArgs {
        CliArgs::try_parse_from(std::iter::once("mdless").chain(args.iter().copied())).unwrap()
    }

    #[test]
    fn verify_command() {
        CliArgs::command().debug_assert();
    }

    #[test]
    fn spec_29_flags_parse() {
        let a = parse(&[
            "--theme",
            "dark",
            "--color",
            "never",
            "--width",
            "100",
            "--no-mouse",
            "--toc",
            "--line-numbers",
            "--no-wrap",
            "--mermaid",
            "mmdc",
            "--mermaid-images",
            "always",
            "--no-config",
            "--debug",
            "README.md",
        ]);
        assert_eq!(a.theme.as_deref(), Some("dark"));
        assert_eq!(a.color, Some(ColorMode::Never));
        assert_eq!(a.width, Some(100));
        assert_eq!(a.mouse_override(), Some(false));
        assert_eq!(a.toc_override(), Some(true));
        assert_eq!(a.line_numbers_override(), Some(true));
        assert_eq!(a.wrap_override(), Some(false));
        assert_eq!(a.mermaid, Some(MermaidBackend::Mmdc));
        assert_eq!(a.mermaid_images, Some(ImageMode::Always));
        assert!(a.no_config);
        assert!(a.debug);
        assert_eq!(a.file.as_deref(), Some(std::path::Path::new("README.md")));
    }

    #[test]
    fn defaults_leave_overrides_unset() {
        let a = parse(&[]);
        assert_eq!(a.mouse_override(), None);
        assert_eq!(a.toc_override(), None);
        assert_eq!(a.wrap_override(), None);
        assert_eq!(a.line_numbers_override(), None);
        assert!(a.file.is_none());
        assert!(!a.check_config && !a.print_capabilities);
    }

    #[test]
    fn key_hints_flags_parse_like_the_toc_flags() {
        assert_eq!(parse(&["--key-hints"]).key_hints_override(), Some(true));
        assert_eq!(parse(&["--no-key-hints"]).key_hints_override(), Some(false));
        assert_eq!(parse(&[]).key_hints_override(), None);
        assert_eq!(
            parse(&["--key-hints", "--no-key-hints"]).key_hints_override(),
            Some(false)
        );
    }

    #[test]
    fn later_flag_wins_for_pairs() {
        let a = parse(&["--mouse", "--no-mouse"]);
        assert_eq!(a.mouse_override(), Some(false));
        let a = parse(&["--no-toc", "--toc"]);
        assert_eq!(a.toc_override(), Some(true));
    }

    #[test]
    fn invalid_enum_value_rejected() {
        let r = CliArgs::try_parse_from(["mdless", "--mermaid", "opengl"]);
        assert!(r.is_err());
        let r = CliArgs::try_parse_from(["mdless", "--color", "sometimes"]);
        assert!(r.is_err());
    }

    #[test]
    fn config_conflicts_with_no_config() {
        let r = CliArgs::try_parse_from(["mdless", "--config", "x.toml", "--no-config"]);
        assert!(r.is_err());
    }

    #[test]
    fn hidden_generators_parse() {
        let a = parse(&["--generate-completions", "bash"]);
        assert_eq!(a.generate_completions, Some(clap_complete::Shell::Bash));
        let a = parse(&["--generate-man"]);
        assert!(a.generate_man);
    }
}
