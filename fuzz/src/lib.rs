//! Fuzz target bodies, factored out of `fuzz_targets/` so that they can also
//! be replayed by the corpus runner (`cargo run --bin smoke`) on a stable
//! toolchain, where libFuzzer is not available.
//!
//! Every function here takes raw bytes and must never panic.

use diple::document::{parse, Alignment, Inline, Inlines, Table};
use diple::layout::table::{layout_table, TableOptions};
use diple::layout::unicode;
use diple::layout::{Layout, LayoutOptions};
use diple::render::theme::Theme;

/// Largest terminal width a fuzzed input may ask for.
///
/// diple itself clamps to the real terminal size; the cap only keeps the
/// fuzzer from spending all its time allocating gigantic lines.
const MAX_WIDTH: usize = 500;

/// Split the leading control byte off the input.
fn split_control(data: &[u8]) -> (u8, &[u8]) {
    match data.split_first() {
        Some((first, rest)) => (*first, rest),
        None => (0, &[]),
    }
}

/// Interpret a control byte as a terminal width in `1..=MAX_WIDTH`.
fn width_from(byte: u8) -> usize {
    // 0 maps to 1, not to 0: a zero-width terminal is not a real input, and
    // the layout engine documents `width.max(1)` behaviour anyway.
    (usize::from(byte) * MAX_WIDTH / 256).max(1)
}

/// `document::parser::parse` on arbitrary UTF-8.
pub fn parse_markdown(data: &[u8]) {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    let doc = parse(text);
    // Touch the derived indices too: sections, anchors and links are built
    // from the same input and have their own edge cases.
    let _ = doc.node_count();
    for node in doc.walk() {
        let _ = &node.kind;
    }
}

/// Parse and lay out at a fuzzed width.
pub fn layout(data: &[u8]) {
    let (control, rest) = split_control(data);
    let Ok(text) = std::str::from_utf8(rest) else {
        return;
    };
    let doc = parse(text);
    let theme = if control & 1 == 0 {
        Theme::dark()
    } else {
        Theme::light()
    };
    let mut opts = LayoutOptions::new(width_from(control), &theme);
    opts.wrap = control & 2 == 0;
    opts.code_wrap = control & 4 == 0;
    opts.code_line_numbers = control & 8 != 0;
    opts.unicode = control & 16 == 0;
    opts.footnotes = control & 32 == 0;
    let tree = Layout::build(&doc, &opts);
    let _ = tree.to_plain_text();
}

/// Table layout with fuzzed cell content, alignments and width.
pub fn table(data: &[u8]) {
    let (control, rest) = split_control(data);
    let Ok(text) = std::str::from_utf8(rest) else {
        return;
    };
    // `\n` separates rows, `|` separates cells — a compact encoding that lets
    // the fuzzer reach ragged rows, empty cells and very wide cells quickly.
    let mut rows: Vec<Vec<Inlines>> = text
        .split('\n')
        .map(|row| {
            row.split('|')
                .map(|cell| vec![Inline::Text(cell.to_string())])
                .collect()
        })
        .collect();
    if rows.is_empty() {
        return;
    }
    let header = rows.remove(0);
    let alignments = (0..header.len())
        .map(|i| match (control as usize + i) % 4 {
            0 => Alignment::None,
            1 => Alignment::Left,
            2 => Alignment::Center,
            _ => Alignment::Right,
        })
        .collect();
    let table = Table {
        alignments,
        header,
        rows,
    };
    let theme = Theme::dark();
    let opts = TableOptions {
        unicode: control & 1 == 0,
        ..TableOptions::default()
    };
    let _ = layout_table(&table, &theme, &opts, width_from(control), &[]);
}

/// The Unicode width / wrap / slice helpers.
pub fn unicode_helpers(data: &[u8]) {
    let (control, rest) = split_control(data);
    let Ok(text) = std::str::from_utf8(rest) else {
        return;
    };
    let n = usize::from(control);
    let (head, tail) = unicode::split_at_width(text, n);
    // Holds for every input, control characters included.
    assert_eq!(
        head.len() + tail.len(),
        text.len(),
        "split_at_width must partition the input"
    );
    assert!(unicode::truncate_to_width(text, n).len() <= text.len());
    assert!(unicode::pad_to_width(text, n).len() >= text.len());
    assert!(unicode::pad_left_to_width(text, n).len() >= text.len());
    let _ = unicode::width(text);
    let _ = unicode::center_to_width(text, n);
    let _ = unicode::truncate_with_ellipsis(text, n, "…");
    let _ = unicode::slice_columns(text, n / 2, n);
    let _ = unicode::expand_tabs(text, (n % 16) + 1);
    let _ = unicode::tokenize(text);
    let wrapped = unicode::wrap(text, n.max(1), n.max(1));

    // The width invariants hold for every input, control characters included:
    // `unicode::width`'s ASCII fast path is restricted to printable ASCII, so
    // it agrees with `unicode::grapheme_width` — the measure `split_at_width`
    // and `wrap` use — that a control character occupies zero cells.
    assert!(
        head.is_empty() || unicode::width(head) <= n,
        "split_at_width must not exceed the requested width"
    );
    for line in wrapped {
        // A single grapheme cluster that is wider than the limit cannot be
        // broken any further and is emitted as-is — the same documented
        // escape hatch `split_at_width` has. Everything else must fit.
        // (Note that "unbreakable" is not the same as "contains no space":
        // `" \u{fe0f}"` is one cluster whose base character is a space.)
        let unbreakable = unicode::graphemes(&line).count() <= 1;
        assert!(
            unbreakable || unicode::width(&line) <= n.max(1),
            "wrap must respect the width for breakable text"
        );
    }
}

/// `config::loader` TOML parsing, through the real file entry point.
pub fn config(data: &[u8]) {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    let mut path = std::env::temp_dir();
    path.push(format!("diple-fuzz-config-{}.toml", std::process::id()));
    if std::fs::write(&path, text).is_err() {
        return;
    }
    // Both outcomes are valid: a well-formed config loads, a malformed one
    // must produce a `ConfigError` rather than a panic.
    let _ = diple::config::loader::load_file(&path);
}

/// The Mermaid subset parser and the native renderer behind it.
pub fn mermaid(data: &[u8]) {
    let (control, rest) = split_control(data);
    let Ok(text) = std::str::from_utf8(rest) else {
        return;
    };
    let _ = diple::mermaid::diagram_kind(text);
    if let Ok(diagram) = diple::mermaid::parse(text) {
        let opts = diple::mermaid::RenderOptions {
            width_cells: width_from(control),
            unicode_box: control & 1 == 0,
            ..Default::default()
        };
        let _ = diple::mermaid::terminal::render(&diagram, &opts);
    }
}
