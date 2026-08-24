//! Configuration loading: explicit `--config` path, `DIPLE_CONFIG`, XDG
//! default location, or built-in defaults with `--no-config`. Invalid values
//! are reported with path, key, value and expected form.

use std::path::{Path, PathBuf};

use directories::ProjectDirs;

use super::keys::KeyOverrideError;
use super::schema::{Config, ConfigError};

/// A loaded configuration plus its origin (None: built-in defaults).
#[derive(Debug, Clone)]
pub struct LoadedConfig {
    /// The parsed configuration.
    pub config: Config,
    /// The file it came from, if any.
    pub path: Option<PathBuf>,
}

/// Default XDG config file path (`~/.config/diple/config.toml`).
pub fn default_path() -> Option<PathBuf> {
    ProjectDirs::from("", "", "diple").map(|d| d.config_dir().join("config.toml"))
}

/// Load configuration.
///
/// Precedence for the file location: `no_config` (skip everything) >
/// `explicit` (`--config`, must exist) > `$DIPLE_CONFIG` (must exist) >
/// XDG default (silently skipped when absent).
pub fn load(explicit: Option<&Path>, no_config: bool) -> Result<LoadedConfig, ConfigError> {
    load_with_env(explicit, no_config, |k| std::env::var(k).ok())
}

/// [`load`] with an injectable environment (for tests).
pub fn load_with_env(
    explicit: Option<&Path>,
    no_config: bool,
    env: impl Fn(&str) -> Option<String>,
) -> Result<LoadedConfig, ConfigError> {
    if no_config {
        return Ok(LoadedConfig {
            config: Config::default(),
            path: None,
        });
    }
    let env_path = env("DIPLE_CONFIG").map(PathBuf::from);
    let (path, required) = match (explicit, env_path) {
        (Some(p), _) => (p.to_path_buf(), true),
        (None, Some(p)) => (p, true),
        (None, None) => match default_path() {
            Some(p) => (p, false),
            None => {
                return Ok(LoadedConfig {
                    config: Config::default(),
                    path: None,
                })
            }
        },
    };
    if !required && !path.exists() {
        return Ok(LoadedConfig {
            config: Config::default(),
            path: None,
        });
    }
    let config = load_file(&path)?;
    Ok(LoadedConfig {
        config,
        path: Some(path),
    })
}

/// Parse and validate one config file.
pub fn load_file(path: &Path) -> Result<Config, ConfigError> {
    let source = std::fs::read_to_string(path).map_err(|source| ConfigError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let config: Config =
        toml::from_str(&source).map_err(|e| invalid_from_toml(path, &source, &e))?;
    validate(path, &source, &config)?;
    Ok(config)
}

/// Semantic validation beyond deserialization.
///
/// `source` is the file's text; it is only used to report the line a rejected
/// value was written on (these used to report line `0`).
fn validate(path: &Path, source: &str, config: &Config) -> Result<(), ConfigError> {
    if config.code.tab_width == 0 {
        return Err(ConfigError::Invalid {
            path: path.to_path_buf(),
            key: "code.tab_width".to_string(),
            value: "0".to_string(),
            expected: "an integer >= 1".to_string(),
            line: line_of(source, "code.tab_width"),
        });
    }
    if config.table.max_column_width < 3 {
        return Err(ConfigError::Invalid {
            path: path.to_path_buf(),
            key: "table.max_column_width".to_string(),
            value: config.table.max_column_width.to_string(),
            expected: "an integer >= 3".to_string(),
            line: line_of(source, "table.max_column_width"),
        });
    }
    // Keybinding overrides must resolve.
    super::keys::KeyMap::from_overrides(config.keys.iter().map(|(k, v)| (k.as_str(), v)))
        .map_err(|e| key_override_error_in(path, source, e))?;
    Ok(())
}

/// Convert a keybinding override error into a full [`ConfigError`].
///
/// The line number is unknown without the file's text; prefer
/// [`key_override_error_in`] when it is available.
pub fn key_override_error(path: &Path, e: KeyOverrideError) -> ConfigError {
    key_override_error_in(path, "", e)
}

/// [`key_override_error`] with the file's text, so the real line is reported.
pub fn key_override_error_in(path: &Path, source: &str, e: KeyOverrideError) -> ConfigError {
    let key = format!("keys.{}", e.action);
    let line = line_of(source, &key);
    ConfigError::Invalid {
        path: path.to_path_buf(),
        key,
        value: e.value,
        expected: key_expected_phrase(&e.reason),
        line,
    }
}

/// Noun phrase for a rejected `[keys]` entry.
///
/// `KeyOverrideError::reason` is a full sentence; spliced in verbatim it read
/// `expected unknown action ...` / `expected invalid key spec ...`.
fn key_expected_phrase(reason: &str) -> String {
    if let Some(idx) = reason.find("(expected ") {
        let list = reason[idx + "(expected ".len()..].trim_end_matches(')');
        return format!("a known action name ({list})");
    }
    match reason.split_once(": ") {
        Some((_, detail)) => format!("a valid key specification ({detail})"),
        None => reason.to_string(),
    }
}

/// 1-based line of the dotted key `dotted` (`keys.quit`, `code.tab_width`) in
/// `source`, or `0` when it cannot be located.
fn line_of(source: &str, dotted: &str) -> usize {
    let (want_section, want_key) = dotted.rsplit_once('.').unwrap_or(("", dotted));
    let mut section = String::new();
    for (index, raw) in source.lines().enumerate() {
        let line = raw.trim();
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            section = name.trim().to_string();
            continue;
        }
        let Some((key, _)) = line.split_once('=') else {
            continue;
        };
        if section == want_section && key.trim().trim_matches(['"', '\'']) == want_key {
            return index + 1;
        }
    }
    0
}

/// Turn a `toml::de::Error` into [`ConfigError::Invalid`] using its span to
/// recover the offending key, value and line number.
fn invalid_from_toml(path: &Path, source: &str, err: &toml::de::Error) -> ConfigError {
    let mut key = String::new();
    let mut value = String::new();
    let mut line = 0usize;
    if let Some(span) = err.span() {
        let start = span.start.min(source.len());
        let end = span.end.min(source.len()).max(start);
        value = source[start..end].trim().trim_matches('"').to_string();
        line = source[..start].matches('\n').count() + 1;
        // Key on the offending line, prefixed with the enclosing [section].
        let line_start = source[..start].rfind('\n').map_or(0, |i| i + 1);
        let line_end = source[start..]
            .find('\n')
            .map_or(source.len(), |i| start + i);
        let line_text = &source[line_start..line_end];
        let local_key = line_text.split('=').next().unwrap_or("").trim();
        let section = source[..line_start]
            .lines()
            .rev()
            .map(str::trim)
            .find(|l| l.starts_with('[') && l.ends_with(']'))
            .map(|l| l.trim_matches(['[', ']']).to_string());
        key = match (section, local_key.is_empty()) {
            (Some(s), false) => format!("{s}.{local_key}"),
            (Some(s), true) => s,
            (None, false) => local_key.to_string(),
            (None, true) => String::new(),
        };
    }
    ConfigError::Invalid {
        path: path.to_path_buf(),
        key,
        value,
        expected: expected_phrase(err.message()),
        line,
    }
}

/// Turn a serde/toml diagnostic into the noun phrase that reads naturally
/// after `— expected ` in [`ConfigError::Invalid`].
///
/// serde's messages already restate the offending value and then say what it
/// wanted (`invalid type: integer `5`, expected a string`, `unknown variant
/// `sideways`, expected one of …`). Splicing the whole message in produced
/// `expected invalid type: integer `5`, expected a string`, which reads like a
/// bug.
fn expected_phrase(message: &str) -> String {
    const NEEDLE: &str = "expected ";
    if message.contains("untagged enum KeyBinding") {
        return "a key specification, or an array of them".to_string();
    }
    match message.rfind(NEEDLE) {
        Some(idx) => message[idx + NEEDLE.len()..]
            .trim()
            .trim_end_matches(')')
            .to_string(),
        None => message.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::schema::{ColorMode, MermaidBackend, TableMode, Theme};
    use std::io::Write;

    fn write_config(content: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
        (dir, path)
    }

    #[test]
    fn defaults_when_no_config() {
        let loaded = load_with_env(None, true, |_| None).unwrap();
        assert_eq!(loaded.config, Config::default());
        assert!(loaded.path.is_none());
        let c = Config::default();
        assert_eq!(c.theme, Theme::Auto);
        assert_eq!(c.color, ColorMode::Auto);
        assert!(c.mouse);
        assert!(!c.toc);
        assert!(!c.line_numbers);
        assert!(c.wrap);
        assert_eq!(c.table.mode, TableMode::Auto);
        assert_eq!(c.table.max_column_width, 60);
        assert!(!c.code.wrap);
        assert_eq!(c.code.tab_width, 4);
        assert_eq!(c.links.opener, "xdg-open");
        assert_eq!(c.mermaid.backend, MermaidBackend::Auto);
        assert_eq!(c.mermaid.mmdc_command, "mmdc");
    }

    #[test]
    fn parses_full_spec_example() {
        let (_d, path) = write_config(
            r#"
theme = "auto"
color = "auto"
mouse = true
toc = false
line_numbers = false
wrap = true

[table]
mode = "auto"
max_column_width = 60

[code]
wrap = false
line_numbers = false
tab_width = 4

[links]
opener = "xdg-open"
osc8 = "auto"

[mermaid]
backend = "auto"
images = "auto"
mmdc_command = "mmdc"

[keys]
quit = "q"
search = "/"
next_search = "n"
previous_search = "N"
next_heading = "]"
previous_heading = "["
toggle_toc = "t"
toggle_fold = "za"
"#,
        );
        let loaded = load_with_env(Some(&path), false, |_| None).unwrap();
        assert_eq!(loaded.config.keys.len(), 8);
        assert_eq!(loaded.path.as_deref(), Some(path.as_path()));
    }

    #[test]
    fn partial_config_keeps_defaults() {
        let (_d, path) = write_config("theme = \"light\"\n\n[table]\nmode = \"scroll\"\n");
        let c = load_with_env(Some(&path), false, |_| None).unwrap().config;
        assert_eq!(c.theme, Theme::Light);
        assert_eq!(c.table.mode, TableMode::Scroll);
        assert_eq!(c.table.max_column_width, 60);
        assert!(c.wrap);
    }

    #[test]
    fn custom_theme_name_is_allowed() {
        let (_d, path) = write_config("theme = \"gruvbox\"\n");
        let c = load_with_env(Some(&path), false, |_| None).unwrap().config;
        assert_eq!(c.theme, Theme::Named("gruvbox".into()));
    }

    #[test]
    fn invalid_enum_value_reports_path_key_value_expected() {
        let (_d, path) = write_config("theme = \"auto\"\n\n[table]\nmode = \"sideways\"\n");
        let err = load_with_env(Some(&path), false, |_| None).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains(&path.display().to_string()), "path in: {msg}");
        assert!(msg.contains("table.mode"), "key in: {msg}");
        assert!(msg.contains("sideways"), "value in: {msg}");
        assert!(
            msg.to_lowercase().contains("wrap"),
            "expected values in: {msg}"
        );
        match err {
            ConfigError::Invalid {
                key, value, line, ..
            } => {
                assert_eq!(key, "table.mode");
                assert_eq!(value, "sideways");
                assert_eq!(line, 4);
            }
            other => panic!("{other}"),
        }
    }

    #[test]
    fn invalid_type_reports_key() {
        let (_d, path) = write_config("mouse = \"yes\"\n");
        let err = load_with_env(Some(&path), false, |_| None).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("mouse"), "{msg}");
        assert!(msg.contains("yes"), "{msg}");
        assert!(msg.to_lowercase().contains("boolean"), "{msg}");
    }

    #[test]
    fn the_expected_phrase_reads_naturally() {
        // serde's own message restates the value and then says what it wanted,
        // so splicing it in whole produced "expected invalid type: integer
        // `5`, expected a string".
        let (_d, path) = write_config("theme = 5\n");
        let err = load_with_env(Some(&path), false, |_| None).unwrap_err();
        let msg = err.to_string();
        assert!(msg.ends_with("expected a string"), "{msg}");
        assert!(!msg.contains("expected invalid type"), "{msg}");
        assert_eq!(msg.matches("expected").count(), 1, "{msg}");
    }

    #[test]
    fn an_invalid_key_binding_reports_its_real_line() {
        // This used to report line 0.
        let (_d, path) = write_config("theme = \"dark\"\n\n[keys]\nquit = \"ctrl-\"\n");
        let err = load_with_env(Some(&path), false, |_| None).unwrap_err();
        match err {
            ConfigError::Invalid {
                key,
                value,
                line,
                expected,
                ..
            } => {
                assert_eq!(key, "keys.quit");
                assert_eq!(value, "ctrl-");
                assert_eq!(line, 4, "the real line of `quit = …`");
                assert!(
                    expected.starts_with("a valid key specification"),
                    "{expected}"
                );
            }
            other => panic!("{other}"),
        }
    }

    #[test]
    fn an_unknown_action_reports_its_line_and_the_allowed_names() {
        let (_d, path) = write_config("[keys]\nnope = \"x\"\n");
        let err = load_with_env(Some(&path), false, |_| None).unwrap_err();
        match err {
            ConfigError::Invalid {
                key,
                line,
                expected,
                ..
            } => {
                assert_eq!(key, "keys.nope");
                assert_eq!(line, 2);
                assert!(
                    expected.starts_with("a known action name (one of: quit"),
                    "{expected}"
                );
                assert!(!expected.contains("unknown action"), "{expected}");
            }
            other => panic!("{other}"),
        }
    }

    #[test]
    fn a_semantic_range_error_reports_its_line() {
        let (_d, path) = write_config("theme = \"dark\"\n\n[code]\ntab_width = 0\n");
        let err = load_with_env(Some(&path), false, |_| None).unwrap_err();
        match err {
            ConfigError::Invalid { key, line, .. } => {
                assert_eq!(key, "code.tab_width");
                assert_eq!(line, 4);
            }
            other => panic!("{other}"),
        }
    }

    #[test]
    fn unknown_key_is_rejected() {
        let (_d, path) = write_config("them = \"dark\"\n");
        let err = load_with_env(Some(&path), false, |_| None).unwrap_err();
        assert!(err.to_string().contains("them"), "{err}");
    }

    #[test]
    fn invalid_tab_width_and_column_width() {
        let (_d, path) = write_config("[code]\ntab_width = 0\n");
        let err = load_with_env(Some(&path), false, |_| None).unwrap_err();
        assert!(err.to_string().contains("code.tab_width"), "{err}");
        let (_d, path) = write_config("[table]\nmax_column_width = 1\n");
        let err = load_with_env(Some(&path), false, |_| None).unwrap_err();
        assert!(err.to_string().contains("table.max_column_width"), "{err}");
    }

    #[test]
    fn invalid_key_binding_reports_action() {
        let (_d, path) = write_config("[keys]\nwarp_speed = \"w\"\n");
        let err = load_with_env(Some(&path), false, |_| None).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("keys.warp_speed"), "{msg}");
        assert!(msg.contains("a known action name"), "{msg}");

        let (_d, path) = write_config("[keys]\nquit = \"hyper-q\"\n");
        let err = load_with_env(Some(&path), false, |_| None).unwrap_err();
        assert!(err.to_string().contains("keys.quit"), "{err}");
        assert!(err.to_string().contains("hyper-q"), "{err}");
    }

    #[test]
    fn missing_explicit_path_errors() {
        let err =
            load_with_env(Some(Path::new("/nonexistent/diple.toml")), false, |_| None).unwrap_err();
        assert!(matches!(err, ConfigError::Io { .. }));
        assert!(err.to_string().contains("/nonexistent/diple.toml"));
    }

    #[test]
    fn env_config_path_is_used() {
        let (_d, path) = write_config("toc = true\n");
        let p = path.display().to_string();
        let loaded = load_with_env(None, false, move |k| {
            (k == "DIPLE_CONFIG").then(|| p.clone())
        })
        .unwrap();
        assert!(loaded.config.toc);
    }

    #[test]
    fn key_binding_list_form() {
        let (_d, path) = write_config("[keys]\nquit = [\"q\", \"ctrl-c\", \"esc\"]\n");
        let c = load_with_env(Some(&path), false, |_| None).unwrap().config;
        assert_eq!(c.keys["quit"].specs().len(), 3);
    }
}
