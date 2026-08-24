//! Diagram-kind detection.
//!
//! Only flowcharts are natively renderable in 1.0. Every other diagram type is
//! reported as "native unsupported" so that
//! [`select_backend`](crate::mermaid::select_backend) can apply the fallback
//! matrix.

use super::ast::Orientation;

/// The Mermaid diagram type of a source block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagramKind {
    /// `graph`/`flowchart` with the given orientation (defaults to `TD`).
    Flowchart {
        /// Flow direction from the header line.
        orientation: Orientation,
    },
    /// `sequenceDiagram`.
    Sequence,
    /// `classDiagram` / `classDiagram-v2`.
    Class,
    /// `stateDiagram` / `stateDiagram-v2`.
    State,
    /// `erDiagram`.
    Er,
    /// `gantt`.
    Gantt,
    /// `pie`.
    Pie,
    /// `journey`.
    Journey,
    /// Anything else (including an empty block); carries the first keyword.
    Unknown(String),
}

impl DiagramKind {
    /// `true` when the native terminal renderer can attempt this diagram.
    ///
    /// In 1.0 that is exactly [`DiagramKind::Flowchart`]. A successful parse is
    /// still required — see [`crate::mermaid::parser::parse`].
    #[must_use]
    pub const fn natively_supported(&self) -> bool {
        matches!(self, Self::Flowchart { .. })
    }

    /// Short human-readable name, used in fallback warnings.
    #[must_use]
    pub fn name(&self) -> &str {
        match self {
            Self::Flowchart { .. } => "flowchart",
            Self::Sequence => "sequence diagram",
            Self::Class => "class diagram",
            Self::State => "state diagram",
            Self::Er => "entity relationship diagram",
            Self::Gantt => "gantt chart",
            Self::Pie => "pie chart",
            Self::Journey => "user journey",
            Self::Unknown(word) => word.as_str(),
        }
    }
}

/// Returns `true` for lines that carry no diagram type information:
/// blank lines, `%% comments` and `%%{init: ...}%%` directives.
fn is_skippable(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.is_empty() || trimmed.starts_with("%%")
}

/// Returns the first line of `source` that can carry the diagram keyword.
pub(crate) fn header_line(source: &str) -> Option<&str> {
    source.lines().find(|line| !is_skippable(line))
}

/// Determines the diagram type by inspecting the first non-comment,
/// non-directive line of `source`.
///
/// Leading `%%{init: ...}%%` directives and `%%` comment lines are skipped.
/// For `graph`/`flowchart` the following token is parsed as the orientation
/// (`LR`, `RL`, `TD`, `TB`, `BT`); an absent or unrecognised token yields
/// [`Orientation::Td`], matching Mermaid's default.
#[must_use]
pub fn diagram_kind(source: &str) -> DiagramKind {
    let Some(line) = header_line(source) else {
        return DiagramKind::Unknown(String::new());
    };
    let line = line.trim();

    // The keyword ends at whitespace or at the orientation separator used by
    // `flowchart-elk` style declarations (`graph LR;`).
    let keyword_end = line
        .find(|c: char| c.is_whitespace() || c == ';')
        .unwrap_or(line.len());
    let (keyword, rest) = line.split_at(keyword_end);
    let rest = rest.trim_start_matches([' ', '\t', ';']);

    match keyword.trim() {
        "graph" | "flowchart" => {
            let token = rest
                .split(|c: char| c.is_whitespace() || c == ';')
                .find(|t| !t.is_empty())
                .unwrap_or("");
            DiagramKind::Flowchart {
                orientation: Orientation::parse(token).unwrap_or_default(),
            }
        }
        "sequenceDiagram" => DiagramKind::Sequence,
        "classDiagram" | "classDiagram-v2" => DiagramKind::Class,
        "stateDiagram" | "stateDiagram-v2" => DiagramKind::State,
        "erDiagram" => DiagramKind::Er,
        "gantt" => DiagramKind::Gantt,
        "pie" => DiagramKind::Pie,
        "journey" => DiagramKind::Journey,
        other => DiagramKind::Unknown(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_flowchart_orientations() {
        for (src, want) in [
            ("graph LR\nA-->B", Orientation::Lr),
            ("graph RL", Orientation::Rl),
            ("flowchart TD", Orientation::Td),
            ("flowchart TB", Orientation::Td),
            ("graph BT", Orientation::Bt),
            ("graph", Orientation::Td),
            ("graph XX", Orientation::Td),
            ("graph LR;", Orientation::Lr),
            ("flowchart   lr  ", Orientation::Lr),
        ] {
            assert_eq!(
                diagram_kind(src),
                DiagramKind::Flowchart { orientation: want },
                "source: {src:?}"
            );
        }
    }

    #[test]
    fn skips_comments_and_init_directives() {
        let src = "%%{init: {'theme':'dark'}}%%\n%% a comment\n\n   \nflowchart LR\nA-->B";
        assert_eq!(
            diagram_kind(src),
            DiagramKind::Flowchart {
                orientation: Orientation::Lr
            }
        );
    }

    #[test]
    fn detects_other_kinds() {
        assert_eq!(
            diagram_kind("sequenceDiagram\nA->>B: hi"),
            DiagramKind::Sequence
        );
        assert_eq!(diagram_kind("classDiagram-v2"), DiagramKind::Class);
        assert_eq!(diagram_kind("stateDiagram-v2"), DiagramKind::State);
        assert_eq!(diagram_kind("erDiagram"), DiagramKind::Er);
        assert_eq!(diagram_kind("gantt\ntitle x"), DiagramKind::Gantt);
        assert_eq!(diagram_kind("pie showData"), DiagramKind::Pie);
        assert_eq!(diagram_kind("journey"), DiagramKind::Journey);
        assert_eq!(
            diagram_kind("mindmap\n  root"),
            DiagramKind::Unknown("mindmap".to_string())
        );
        assert_eq!(diagram_kind(""), DiagramKind::Unknown(String::new()));
        assert_eq!(
            diagram_kind("%% only a comment"),
            DiagramKind::Unknown(String::new())
        );
    }

    #[test]
    fn only_flowcharts_are_native() {
        assert!(diagram_kind("graph LR").natively_supported());
        assert!(!diagram_kind("sequenceDiagram").natively_supported());
        assert!(!diagram_kind("gantt").natively_supported());
    }

    #[test]
    fn never_panics_on_garbage() {
        for src in [
            "\u{0}\u{1}\u{2}",
            "graph\u{0}LR",
            "%%%%%%%%",
            "\u{feff}graph LR",
            "🙂🙂🙂",
            &"%%\n".repeat(1000),
        ] {
            let _ = diagram_kind(src);
        }
    }
}
