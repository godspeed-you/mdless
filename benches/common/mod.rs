//! Shared fixture helpers for the mdless benchmarks.
//!
//! Everything here is deterministic: the synthetic documents are generated
//! from the checked-in fixtures with a fixed recipe, so two runs on the same
//! commit benchmark exactly the same input (benchmarks run in CI and
//! meaningful regressions should flag the build).

#![allow(dead_code)]

use std::path::PathBuf;

/// Absolute path of a checked-in fixture.
pub fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// Read a checked-in fixture.
pub fn fixture(name: &str) -> String {
    match std::fs::read_to_string(fixture_path(name)) {
        Ok(text) => text,
        Err(error) => panic!("cannot read fixture {name}: {error}"),
    }
}

/// The README-sized document used for the startup benchmarks.
pub fn readme() -> String {
    fixture("readme.md")
}

/// A "typical" documentation page: every fixture once, which exercises
/// headings, lists, tables, code blocks, footnotes and Mermaid in one pass.
pub fn mixed() -> String {
    const FIXTURES: [&str; 8] = [
        "readme.md",
        "code-blocks.md",
        "nested-lists.md",
        "wide-table.md",
        "narrow-table.md",
        "mixed-formatting.md",
        "unicode-cjk-emoji.md",
        "footnotes.md",
    ];
    let mut out = String::new();
    for name in FIXTURES {
        out.push_str(&fixture(name));
        out.push_str("\n\n");
    }
    out
}

/// A synthetic document of at least `target_bytes` bytes.
///
/// Built by repeating [`mixed`] with a per-repetition heading so that headings
/// stay unique (anchor de-duplication is part of what we measure) and the
/// section tree keeps growing.
pub fn large(target_bytes: usize) -> String {
    let unit = mixed();
    let mut out = String::with_capacity(target_bytes + unit.len());
    let mut chapter = 0usize;
    while out.len() < target_bytes {
        chapter += 1;
        out.push_str(&format!("\n\n# Chapter {chapter}\n\n"));
        out.push_str(&unit);
    }
    out
}

/// A large single code block (Rust), used for the highlighting benchmark.
pub fn big_code_block(lines: usize) -> String {
    let mut code = String::with_capacity(lines * 48);
    for i in 0..lines {
        code.push_str(&format!(
            "pub fn item_{i}(value: &str, count: usize) -> Option<String> {{\n    \
             let mut out = String::from(\"item {i}\"); // note\n    \
             if count > {i} {{ out.push_str(value); }}\n    Some(out)\n}}\n"
        ));
    }
    format!("```rust\n{code}```\n")
}
