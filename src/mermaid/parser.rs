//! Parser for the supported Mermaid flowchart subset.
//!
//! # Supported syntax
//!
//! * Header: `graph <dir>` / `flowchart <dir>` with `LR`, `RL`, `TD`, `TB`, `BT`
//!   (missing or unknown direction ⇒ `TD`).
//! * Node shapes: `A`, `A[label]`, `A(label)`, `A([label])`, `A{label}`,
//!   `A((label))`, `A{{label}}`, `A[[label]]`, `A[(label)]`. `A>label]` and any
//!   other shape degrade to [`NodeShape::Rect`].
//! * Quoted labels with escapes: `A["a, b"]`, `A["say \"hi\""]`.
//! * Edges: `-->`, `---`, `-.->`, `-.-`, `==>`, `===`, `~~~`, longer dash runs
//!   (`--->`, `----`), and reversed/bidirectional heads `<--`, `<-->`.
//! * Edge labels: `-->|text|` and `-- text -->` (also `-. text .->`,
//!   `== text ==>`).
//! * Chains: `A --> B --> C`; multi-source/target `A & B --> C & D`.
//! * `subgraph <title> ... end` — nodes are collected, the grouping box is not
//!   drawn (see [`Diagram::has_unsupported_features`]).
//! * `%%` comments (whole line and trailing), `%%{init: ...}%%` directives.
//! * `click` / `style` / `classDef` / `linkStyle` / `class` / `direction`
//!   statements are ignored gracefully.
//! * Both `;` and newline act as statement separators; blank lines are ignored.
//!
//! # Guarantees
//!
//! Parsing is total: [`parse`] returns `Result` and **never panics** on
//! arbitrary input (fuzz target).

use super::ast::{ArrowKind, Diagram, DiagramEdge, DiagramNode, EdgeStyle, NodeShape};
use super::detect::{diagram_kind, DiagramKind};

/// A parse failure with a 1-based source position.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("mermaid: line {line}, column {column}: {message}")]
pub struct MermaidParseError {
    /// 1-based line number within the diagram source.
    pub line: usize,
    /// 1-based column (in characters) within that line.
    pub column: usize,
    /// Human-readable description.
    pub message: String,
}

impl MermaidParseError {
    fn new(line: usize, column: usize, message: impl Into<String>) -> Self {
        Self {
            line,
            column,
            message: message.into(),
        }
    }
}

type PResult<T> = Result<T, MermaidParseError>;

/// One logical statement (`;`- or newline-separated) with its source position.
struct Stmt {
    chars: Vec<char>,
    line: usize,
    /// 0-based character column of `chars[0]` within the source line.
    col: usize,
}

/// Splits the source into logical statements, stripping `%%` comments.
fn split_statements(source: &str) -> Vec<Stmt> {
    let mut out = Vec::new();
    for (line_idx, raw) in source.lines().enumerate() {
        let chars: Vec<char> = raw.chars().collect();
        let mut in_quote = false;
        let mut escaped = false;
        let mut start = 0usize;
        let mut i = 0usize;
        while i < chars.len() {
            let c = chars[i];
            if in_quote {
                if escaped {
                    escaped = false;
                } else if c == '\\' {
                    escaped = true;
                } else if c == '"' {
                    in_quote = false;
                }
                i += 1;
                continue;
            }
            if c == '"' {
                in_quote = true;
                i += 1;
                continue;
            }
            if c == '%' && chars.get(i + 1) == Some(&'%') {
                // Trailing (or whole-line) comment: everything up to EOL is dropped.
                break;
            }
            if c == ';' {
                push_stmt(&mut out, &chars[start..i], line_idx + 1, start);
                start = i + 1;
            }
            i += 1;
        }
        if start < chars.len() {
            push_stmt(
                &mut out,
                &chars[start..i.min(chars.len())],
                line_idx + 1,
                start,
            );
        }
    }
    out
}

fn push_stmt(out: &mut Vec<Stmt>, slice: &[char], line: usize, col: usize) {
    // Trim leading whitespace, keeping the column in sync.
    let lead = slice.iter().take_while(|c| c.is_whitespace()).count();
    let trimmed: Vec<char> = slice[lead..].to_vec();
    let end = trimmed
        .iter()
        .rposition(|c| !c.is_whitespace())
        .map_or(0, |p| p + 1);
    if end == 0 {
        return;
    }
    out.push(Stmt {
        chars: trimmed[..end].to_vec(),
        line,
        col: col + lead,
    });
}

/// Statement keywords that are accepted and ignored.
const IGNORED_KEYWORDS: [&str; 7] = [
    "click",
    "style",
    "classdef",
    "linkstyle",
    "class",
    "direction",
    "accTitle",
];

/// Parses a Mermaid flowchart block into a [`Diagram`].
///
/// # Errors
///
/// Returns [`MermaidParseError`] when the block is not a flowchart, when a
/// statement cannot be understood, or when a label/shape is left unclosed.
pub fn parse(source: &str) -> PResult<Diagram> {
    let kind = diagram_kind(source);
    let orientation = match kind {
        DiagramKind::Flowchart { orientation } => orientation,
        other => {
            return Err(MermaidParseError::new(
                1,
                1,
                format!(
                    "native renderer supports flowcharts only, found {}",
                    if other.name().is_empty() {
                        "an empty diagram"
                    } else {
                        other.name()
                    }
                ),
            ))
        }
    };

    let statements = split_statements(source);
    let mut diagram = Diagram {
        orientation,
        ..Diagram::default()
    };

    let mut subgraph_stack: Vec<usize> = Vec::new();
    let mut header_seen = false;

    for stmt in &statements {
        let text: String = stmt.chars.iter().collect();
        let first_word = text
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .to_string();

        if !header_seen {
            // The header statement itself was already consumed by diagram_kind.
            if first_word == "graph" || first_word == "flowchart" {
                header_seen = true;
                continue;
            }
            header_seen = true;
        }

        if first_word == "subgraph" {
            let title = parse_subgraph_title(&text);
            diagram.subgraphs.push(super::ast::Subgraph {
                title,
                node_ids: Vec::new(),
            });
            let idx = diagram.subgraphs.len() - 1;
            subgraph_stack.push(idx);
            diagram.note_unsupported("subgraph grouping is not drawn in the terminal renderer");
            continue;
        }
        if first_word == "end" && text.trim() == "end" {
            subgraph_stack.pop();
            continue;
        }
        if IGNORED_KEYWORDS
            .iter()
            .any(|k| k.eq_ignore_ascii_case(&first_word))
        {
            continue;
        }

        parse_flow_statement(stmt, &mut diagram, subgraph_stack.last().copied())?;
    }

    Ok(diagram)
}

/// `subgraph one`, `subgraph one [Title]`, `subgraph "A title"`.
fn parse_subgraph_title(text: &str) -> String {
    let rest = text.trim_start_matches("subgraph").trim();
    if let Some(open) = rest.find('[') {
        let inner = &rest[open + 1..];
        let inner = inner.strip_suffix(']').unwrap_or(inner);
        return unquote(inner.trim());
    }
    unquote(rest)
}

fn unquote(s: &str) -> String {
    let t = s.trim();
    if t.len() >= 2 && t.starts_with('"') && t.ends_with('"') {
        let inner = &t[1..t.len() - 1];
        let mut out = String::with_capacity(inner.len());
        let mut escaped = false;
        for c in inner.chars() {
            if escaped {
                out.push(c);
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else {
                out.push(c);
            }
        }
        out
    } else {
        t.to_string()
    }
}

// ---------------------------------------------------------------------------
// Statement-level parsing
// ---------------------------------------------------------------------------

struct NodeSpec {
    id: String,
    label: Option<String>,
    shape: NodeShape,
}

struct Connector {
    style: EdgeStyle,
    arrow: ArrowKind,
    label: Option<String>,
    /// `true` for `<--`-style heads: the edge points at the *left* operand.
    reversed: bool,
}

fn parse_flow_statement(
    stmt: &Stmt,
    diagram: &mut Diagram,
    subgraph: Option<usize>,
) -> PResult<()> {
    let s = &stmt.chars;
    let mut i = 0usize;

    let mut prev = parse_node_group(stmt, s, &mut i)?;
    register_group(diagram, &prev, subgraph);

    loop {
        skip_ws(s, &mut i);
        if i >= s.len() {
            break;
        }
        let conn = parse_connector(stmt, s, &mut i)?;
        skip_ws(s, &mut i);
        let next = parse_node_group(stmt, s, &mut i)?;
        register_group(diagram, &next, subgraph);

        for a in &prev {
            for b in &next {
                let (from_id, to_id) = if conn.reversed {
                    (b.id.as_str(), a.id.as_str())
                } else {
                    (a.id.as_str(), b.id.as_str())
                };
                let (Some(from), Some(to)) =
                    (diagram.node_index(from_id), diagram.node_index(to_id))
                else {
                    continue;
                };
                diagram.edges.push(DiagramEdge {
                    from,
                    to,
                    label: conn.label.clone(),
                    style: conn.style,
                    arrow: conn.arrow,
                });
            }
        }
        prev = next;
    }
    Ok(())
}

fn register_group(diagram: &mut Diagram, group: &[NodeSpec], subgraph: Option<usize>) {
    for spec in group {
        match diagram.node_index(&spec.id) {
            Some(idx) => {
                if let Some(label) = &spec.label {
                    if let Some(node) = diagram.nodes.get_mut(idx) {
                        node.label = label.clone();
                        node.shape = spec.shape;
                    }
                }
            }
            None => diagram.nodes.push(DiagramNode {
                id: spec.id.clone(),
                label: spec.label.clone().unwrap_or_else(|| spec.id.clone()),
                shape: spec.shape,
            }),
        }
        if let Some(sg) = subgraph.and_then(|s| diagram.subgraphs.get_mut(s)) {
            if !sg.node_ids.contains(&spec.id) {
                sg.node_ids.push(spec.id.clone());
            }
        }
    }
}

fn skip_ws(s: &[char], i: &mut usize) {
    while *i < s.len() && s[*i].is_whitespace() {
        *i += 1;
    }
}

fn pos(stmt: &Stmt, i: usize) -> (usize, usize) {
    (stmt.line, stmt.col + i + 1)
}

fn parse_node_group(stmt: &Stmt, s: &[char], i: &mut usize) -> PResult<Vec<NodeSpec>> {
    let mut group = vec![parse_node(stmt, s, i)?];
    loop {
        let save = *i;
        skip_ws(s, i);
        if *i < s.len() && s[*i] == '&' {
            *i += 1;
            skip_ws(s, i);
            group.push(parse_node(stmt, s, i)?);
        } else {
            *i = save;
            break;
        }
    }
    Ok(group)
}

fn is_id_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '.'
}

fn parse_node(stmt: &Stmt, s: &[char], i: &mut usize) -> PResult<NodeSpec> {
    skip_ws(s, i);
    let start = *i;
    while *i < s.len() && is_id_char(s[*i]) {
        *i += 1;
    }
    if *i == start {
        let (line, col) = pos(stmt, *i);
        return Err(MermaidParseError::new(
            line,
            col,
            format!(
                "expected a node identifier, found {}",
                s.get(*i)
                    .map_or("end of statement".to_string(), |c| format!("{c:?}"))
            ),
        ));
    }
    let id: String = s[start..*i].iter().collect();

    // Optional shape + label.
    const SHAPES: [(&str, &str, NodeShape); 9] = [
        ("([", "])", NodeShape::Stadium),
        ("[[", "]]", NodeShape::Subroutine),
        ("[(", ")]", NodeShape::Cylinder),
        ("((", "))", NodeShape::Circle),
        ("{{", "}}", NodeShape::Hexagon),
        ("[", "]", NodeShape::Rect),
        ("(", ")", NodeShape::Round),
        ("{", "}", NodeShape::Diamond),
        (">", "]", NodeShape::Rect), // asymmetric shape degrades to Rect
    ];

    for (open, close, shape) in SHAPES {
        if starts_with(s, *i, open) {
            let open_at = *i;
            *i += open.chars().count();
            let label = parse_label(stmt, s, i, close, open_at)?;
            return Ok(NodeSpec {
                id,
                label: Some(label),
                shape,
            });
        }
    }

    Ok(NodeSpec {
        id,
        label: None,
        shape: NodeShape::Rect,
    })
}

fn starts_with(s: &[char], i: usize, pat: &str) -> bool {
    pat.chars()
        .enumerate()
        .all(|(k, c)| s.get(i + k).copied() == Some(c))
}

/// Reads a label up to `close`, honouring `"quoted, labels"` with `\"` escapes.
fn parse_label(
    stmt: &Stmt,
    s: &[char],
    i: &mut usize,
    close: &str,
    open_at: usize,
) -> PResult<String> {
    skip_ws(s, i);
    if s.get(*i) == Some(&'"') {
        *i += 1;
        let mut out = String::new();
        let mut escaped = false;
        while *i < s.len() {
            let c = s[*i];
            *i += 1;
            if escaped {
                out.push(c);
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                skip_ws(s, i);
                if !starts_with(s, *i, close) {
                    let (line, col) = pos(stmt, *i);
                    return Err(MermaidParseError::new(
                        line,
                        col,
                        format!("expected {close:?} after quoted label"),
                    ));
                }
                *i += close.chars().count();
                return Ok(out);
            } else {
                out.push(c);
            }
        }
        let (line, col) = pos(stmt, open_at);
        return Err(MermaidParseError::new(
            line,
            col,
            "unterminated quoted label",
        ));
    }

    let start = *i;
    while *i < s.len() && !starts_with(s, *i, close) {
        *i += 1;
    }
    if *i >= s.len() {
        let (line, col) = pos(stmt, open_at);
        return Err(MermaidParseError::new(
            line,
            col,
            format!("unterminated node label, expected {close:?}"),
        ));
    }
    let label: String = s[start..*i].iter().collect();
    *i += close.chars().count();
    Ok(label.trim().to_string())
}

// ---------------------------------------------------------------------------
// Connectors
// ---------------------------------------------------------------------------

fn run_len(s: &[char], i: usize, c: char) -> usize {
    let mut n = 0;
    while s.get(i + n).copied() == Some(c) {
        n += 1;
    }
    n
}

/// Finds the next occurrence of `pat` at or after `from`.
fn find_from(s: &[char], from: usize, pat: &str) -> Option<usize> {
    (from..s.len()).find(|&k| starts_with(s, k, pat))
}

fn parse_connector(stmt: &Stmt, s: &[char], i: &mut usize) -> PResult<Connector> {
    let start = *i;
    let err = |at: usize| {
        let (line, col) = pos(stmt, at);
        MermaidParseError::new(line, col, "expected an edge connector (e.g. `-->`)")
    };

    let mut head_left = false;
    if s.get(*i) == Some(&'<') {
        head_left = true;
        *i += 1;
    }

    let Some(&c) = s.get(*i) else {
        return Err(err(start));
    };

    let mut label: Option<String> = None;
    let is_ws = |s: &[char], i: usize| s.get(i).is_some_and(|c| c.is_whitespace());

    let (style, head_right) = match c {
        '~' => {
            let n = run_len(s, *i, '~');
            if n < 2 {
                return Err(err(start));
            }
            *i += n;
            (EdgeStyle::Invisible, false)
        }
        '=' => {
            let n = run_len(s, *i, '=');
            if n < 2 {
                return Err(err(start));
            }
            *i += n;
            if s.get(*i) == Some(&'>') {
                *i += 1;
                (EdgeStyle::Thick, true)
            } else if n >= 3 || head_left {
                (EdgeStyle::Thick, false)
            } else if is_ws(s, *i) {
                let (head_right, text) =
                    parse_inline_label(s, i, "==").ok_or_else(|| err(start))?;
                label = Some(text);
                (EdgeStyle::Thick, head_right)
            } else {
                return Err(err(start));
            }
        }
        '-' if s.get(*i + 1) == Some(&'.') => {
            *i += 1;
            let dots = run_len(s, *i, '.');
            *i += dots;
            if is_ws(s, *i) {
                let (head_right, text) =
                    parse_inline_label(s, i, ".-").ok_or_else(|| err(start))?;
                label = Some(text);
                (EdgeStyle::Dotted, head_right)
            } else if s.get(*i) == Some(&'-') {
                *i += 1;
                if s.get(*i) == Some(&'>') {
                    *i += 1;
                    (EdgeStyle::Dotted, true)
                } else {
                    (EdgeStyle::Dotted, false)
                }
            } else {
                return Err(err(start));
            }
        }
        '-' => {
            let n = run_len(s, *i, '-');
            if n < 2 {
                return Err(err(start));
            }
            *i += n;
            if s.get(*i) == Some(&'>') {
                *i += 1;
                (EdgeStyle::Solid, true)
            } else if n >= 3 || head_left {
                (EdgeStyle::Solid, false)
            } else if is_ws(s, *i) {
                let (head_right, text) =
                    parse_inline_label(s, i, "--").ok_or_else(|| err(start))?;
                label = Some(text);
                (EdgeStyle::Solid, head_right)
            } else {
                return Err(err(start));
            }
        }
        _ => return Err(err(start)),
    };

    let arrow = match (head_left, head_right) {
        (false, false) => ArrowKind::None,
        (false, true) | (true, false) => ArrowKind::Arrow,
        (true, true) => ArrowKind::DoubleArrow,
    };
    let arrow = if style == EdgeStyle::Invisible {
        ArrowKind::None
    } else {
        arrow
    };
    let reversed = head_left && !head_right;

    // Optional `|label|` suffix (only when no inline label was given).
    if label.is_none() {
        let save = *i;
        skip_ws(s, i);
        if s.get(*i) == Some(&'|') {
            let open_at = *i;
            *i += 1;
            label = Some(parse_label(stmt, s, i, "|", open_at)?);
        } else {
            *i = save;
        }
    }

    Ok(Connector {
        style,
        arrow,
        label,
        reversed,
    })
}

/// Parses ` text <terminator>[>]`, e.g. ` text -->` after `--` was consumed.
///
/// Returns `(head_right, label)`, or `None` when no terminator follows so the
/// caller can report a parse error.
fn parse_inline_label(s: &[char], i: &mut usize, terminator: &str) -> Option<(bool, String)> {
    let text_start = *i;
    let end = find_from(s, text_start, terminator)?;
    let label: String = s[text_start..end].iter().collect();
    let label = label.trim().to_string();
    if label.is_empty() {
        return None;
    }
    let term_char = terminator.chars().last()?;
    let mut j = end + terminator.chars().count();
    // Consume a longer terminator run, e.g. `---->`.
    j += run_len(s, j, term_char);
    let head_right = s.get(j) == Some(&'>');
    if head_right {
        j += 1;
    }
    *i = j;
    Some((head_right, label))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mermaid::ast::Orientation;

    fn dg(src: &str) -> Diagram {
        parse(src).unwrap()
    }

    fn ids(d: &Diagram) -> Vec<&str> {
        d.nodes.iter().map(|n| n.id.as_str()).collect()
    }

    fn edge_pairs(d: &Diagram) -> Vec<(&str, &str)> {
        d.edges
            .iter()
            .filter_map(|e| {
                Some((
                    d.nodes.get(e.from)?.id.as_str(),
                    d.nodes.get(e.to)?.id.as_str(),
                ))
            })
            .collect()
    }

    #[test]
    fn spec_11_1_example() {
        let d = dg("graph LR\n    A --> B\n    B --> C\n");
        assert_eq!(d.orientation, Orientation::Lr);
        assert_eq!(ids(&d), ["A", "B", "C"]);
        assert_eq!(edge_pairs(&d), [("A", "B"), ("B", "C")]);
        assert!(!d.has_unsupported_features);
    }

    #[test]
    fn all_node_shapes() {
        let d = dg("flowchart TD\n\
             A[rect]\n\
             B(round)\n\
             C([stadium])\n\
             D{diamond}\n\
             E((circle))\n\
             F{{hex}}\n\
             G[[sub]]\n\
             H[(db)]\n\
             I>asym]\n\
             J\n");
        let shapes: Vec<NodeShape> = d.nodes.iter().map(|n| n.shape).collect();
        assert_eq!(
            shapes,
            [
                NodeShape::Rect,
                NodeShape::Round,
                NodeShape::Stadium,
                NodeShape::Diamond,
                NodeShape::Circle,
                NodeShape::Hexagon,
                NodeShape::Subroutine,
                NodeShape::Cylinder,
                NodeShape::Rect,
                NodeShape::Rect,
            ]
        );
        assert_eq!(d.nodes[0].label, "rect");
        assert_eq!(d.nodes[9].label, "J", "bare node labels default to the id");
    }

    #[test]
    fn quoted_labels_with_escapes() {
        let d = dg("graph LR\nA[\"a, b\"] --> B[\"say \\\"hi\\\"\"]\n");
        assert_eq!(d.nodes[0].label, "a, b");
        assert_eq!(d.nodes[1].label, "say \"hi\"");
    }

    #[test]
    fn edge_styles_and_arrows() {
        let cases = [
            ("A --> B", EdgeStyle::Solid, ArrowKind::Arrow),
            ("A --- B", EdgeStyle::Solid, ArrowKind::None),
            ("A ---> B", EdgeStyle::Solid, ArrowKind::Arrow),
            ("A -.-> B", EdgeStyle::Dotted, ArrowKind::Arrow),
            ("A -.- B", EdgeStyle::Dotted, ArrowKind::None),
            ("A -..-> B", EdgeStyle::Dotted, ArrowKind::Arrow),
            ("A ==> B", EdgeStyle::Thick, ArrowKind::Arrow),
            ("A === B", EdgeStyle::Thick, ArrowKind::None),
            ("A ~~~ B", EdgeStyle::Invisible, ArrowKind::None),
            ("A <--> B", EdgeStyle::Solid, ArrowKind::DoubleArrow),
        ];
        for (stmt, style, arrow) in cases {
            let d = dg(&format!("graph LR\n{stmt}\n"));
            assert_eq!(d.edges.len(), 1, "{stmt}");
            assert_eq!(d.edges[0].style, style, "{stmt}");
            assert_eq!(d.edges[0].arrow, arrow, "{stmt}");
        }
    }

    #[test]
    fn reversed_arrow_swaps_endpoints() {
        let d = dg("graph LR\nA <-- B\n");
        assert_eq!(edge_pairs(&d), [("B", "A")]);
    }

    #[test]
    fn edge_labels_both_forms() {
        let d = dg("graph LR\nA -->|yes| B\nB -- no --> C\nC -. maybe .-> D\nD == go ==> E\n");
        let labels: Vec<Option<&str>> = d.edges.iter().map(|e| e.label.as_deref()).collect();
        assert_eq!(labels, [Some("yes"), Some("no"), Some("maybe"), Some("go")]);
        assert_eq!(d.edges[2].style, EdgeStyle::Dotted);
        assert_eq!(d.edges[3].style, EdgeStyle::Thick);
        assert!(d.edges.iter().all(|e| e.arrow == ArrowKind::Arrow));
    }

    #[test]
    fn quoted_edge_label() {
        let d = dg("graph LR\nA -->|\"a, b\"| B\n");
        assert_eq!(d.edges[0].label.as_deref(), Some("a, b"));
    }

    #[test]
    fn chains_and_multi_endpoints() {
        let d = dg("graph LR\nA --> B --> C\n");
        assert_eq!(edge_pairs(&d), [("A", "B"), ("B", "C")]);

        let d = dg("graph LR\nA & B --> C & D\n");
        assert_eq!(
            edge_pairs(&d),
            [("A", "C"), ("A", "D"), ("B", "C"), ("B", "D")]
        );

        let d = dg("graph LR\nA & B --> C --> D & E\n");
        assert_eq!(
            edge_pairs(&d),
            [("A", "C"), ("B", "C"), ("C", "D"), ("C", "E")]
        );
    }

    #[test]
    fn semicolon_and_newline_separators() {
        let a = dg("graph LR;A-->B;B-->C;");
        let b = dg("graph LR\nA-->B\nB-->C\n");
        assert_eq!(a, b);
    }

    #[test]
    fn comments_directives_and_ignored_statements() {
        let d = dg("%%{init: {'theme':'dark'}}%%\n\
             graph LR\n\
             %% a comment\n\
             A --> B %% trailing comment\n\
             \n\
             click A \"https://example.org\"\n\
             style A fill:#f9f\n\
             classDef big font-size:20px\n\
             class A big\n\
             linkStyle 0 stroke:#333\n\
             direction LR\n");
        assert_eq!(edge_pairs(&d), [("A", "B")]);
        assert_eq!(ids(&d), ["A", "B"]);
    }

    #[test]
    fn subgraphs_are_parsed_and_flagged() {
        let d = dg("graph TD\n\
             subgraph one [Group One]\n\
             A --> B\n\
             end\n\
             subgraph \"two\"\n\
             C\n\
             end\n\
             B --> C\n");
        assert_eq!(d.subgraphs.len(), 2);
        assert_eq!(d.subgraphs[0].title, "Group One");
        assert_eq!(d.subgraphs[0].node_ids, ["A", "B"]);
        assert_eq!(d.subgraphs[1].title, "two");
        assert_eq!(d.subgraphs[1].node_ids, ["C"]);
        assert!(d.has_unsupported_features);
        assert_eq!(d.unsupported_notes.len(), 1);
        assert_eq!(edge_pairs(&d), [("A", "B"), ("B", "C")]);
    }

    #[test]
    fn label_defined_later_wins() {
        let d = dg("graph LR\nA --> B\nA[Start]\n");
        assert_eq!(d.nodes[0].label, "Start");
    }

    #[test]
    fn non_flowchart_is_rejected() {
        let e = parse("sequenceDiagram\nA->>B: hi").unwrap_err();
        assert!(e.message.contains("flowchart"), "{}", e.message);
        assert_eq!(e.line, 1);
    }

    #[test]
    fn errors_carry_line_and_column() {
        let e = parse("graph LR\nA[unterminated\n").unwrap_err();
        assert_eq!(e.line, 2);
        assert_eq!(e.column, 2);

        let e = parse("graph LR\nA ?? B\n").unwrap_err();
        assert_eq!(e.line, 2);
        assert!(e.message.contains("connector"));
    }

    #[test]
    fn never_panics_on_garbage() {
        let mut inputs: Vec<String> = vec![
            String::new(),
            "graph".into(),
            "graph LR".into(),
            "graph LR\n-->".into(),
            "graph LR\nA[".into(),
            "graph LR\nA[\"".into(),
            "graph LR\nA[\"\\".into(),
            "graph LR\nA --".into(),
            "graph LR\nA -- ".into(),
            "graph LR\nA -- x".into(),
            "graph LR\nA -.".into(),
            "graph LR\nA <".into(),
            "graph LR\nA -->|".into(),
            "graph LR\nA & ".into(),
            "graph LR\n&&&&&&".into(),
            "graph LR\n||||||".into(),
            "graph LR\n;;;;;;".into(),
            "graph LR\nsubgraph\nend\nend\nend".into(),
            "graph LR\n\u{0}\u{1}\u{2}\u{7f}".into(),
            "\u{0}graph\u{0}LR\u{0}A-->B".into(),
            "graph LR\nA[\u{202e}\u{200b}] --> B[🙂🇩🇪]".into(),
            "graph LR\nA[[[[[[[[[[".into(),
            "graph LR\n".to_string() + &"A --> ".repeat(500) + "B",
            "graph LR\n".to_string() + &"subgraph s\n".repeat(200) + &"end\n".repeat(200),
            "graph LR\n".to_string() + &"(".repeat(2000),
            "graph LR\n".to_string() + &"-".repeat(2000),
            "graph LR\n".to_string() + &"\"".repeat(500),
        ];
        // Binary-ish noise.
        inputs.push((0u8..=255).map(|b| b as char).collect());
        for src in &inputs {
            let _ = parse(src);
        }
    }

    #[test]
    fn parsing_is_deterministic() {
        let src = "graph TD\nA --> B & C\nC --> D\nB --> D\n";
        assert_eq!(dg(src), dg(src));
    }
}
