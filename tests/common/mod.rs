//! Shared helpers for the integration tests.
//!
//! This is a plain `mod common;` include rather than the crate's `testing`
//! module: integration tests link `diple` as an *external* crate, built
//! without `cfg(test)`, so `crate::testing` is invisible from here. A
//! subdirectory `mod.rs` is not compiled as its own test target, so nothing
//! here needs a `#[test]`.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;

use assert_cmd::prelude::*;

/// Path to a fixture document.
pub fn fixture(name: &str) -> PathBuf {
    fixtures_dir().join(name)
}

/// The fixtures directory.
pub fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// The names (file stems) of every Markdown fixture, sorted.
pub fn fixture_names() -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(fixtures_dir())
        .expect("fixtures dir")
        .map(|e| e.expect("dir entry").path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("md"))
        .filter_map(|p| p.file_stem().and_then(|s| s.to_str()).map(str::to_string))
        .collect();
    names.sort();
    assert!(!names.is_empty(), "no fixtures");
    names
}

/// The `diple` binary with the developer's environment neutralised.
///
/// Without this, a developer (or a CI image) that exports `CLICOLOR_FORCE=1`
/// fails `color_never_emits_no_escape_sequences`, and one that exports
/// `DIPLE_THEME` changes what every test renders. `env_remove` rather than
/// `env_clear`: clearing everything would also take `PATH` and the loader
/// variables `Command::cargo_bin` itself resolves the binary through.
///
/// Use this only where a test must pass its own `--config`; everywhere else
/// use [`diple`], which also adds `--no-config`.
pub fn command() -> Command {
    let mut cmd = Command::cargo_bin("diple").expect("the diple binary");
    for var in ["DIPLE_THEME", "NO_COLOR", "CLICOLOR_FORCE"] {
        cmd.env_remove(var);
    }
    cmd
}

/// [`command`] plus `--no-config`: the developer's `config.toml` is ignored.
pub fn diple() -> Command {
    let mut cmd = command();
    cmd.arg("--no-config");
    cmd
}

/// The body of an insta snapshot file — everything after the YAML header.
///
/// The header is the block between the first two `---` lines; insta also
/// strips the value's trailing newline when it writes the file, so the body is
/// returned exactly as stored and callers compare against a trimmed value.
pub fn snapshot_body(name: &str) -> Option<String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/snapshots")
        .join(format!("{name}.snap"));
    let text = std::fs::read_to_string(path).ok()?;
    let rest = text.strip_prefix("---\n")?;
    let end = rest.find("\n---\n")?;
    Some(rest[end + "\n---\n".len()..].to_string())
}
