//! Internal graph representation for the supported Mermaid subset.
//!
//! The types here are deliberately independent of both the Markdown document
//! model and the terminal layer: [`crate::mermaid::parser`] produces a
//! [`Diagram`], and [`crate::mermaid::terminal`] turns it into character cells.

use std::fmt;

/// Flow direction of a flowchart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Orientation {
    /// Left to right (`graph LR`).
    Lr,
    /// Right to left (`graph RL`).
    Rl,
    /// Top down (`graph TD` / `graph TB`). Mermaid's default.
    #[default]
    Td,
    /// Bottom up (`graph BT`).
    Bt,
}

impl Orientation {
    /// Parses an orientation token such as `LR`, `rl`, `TD`, `TB` or `BT`.
    ///
    /// Returns `None` for anything else; the caller decides whether that is an
    /// error or simply means "no orientation given".
    #[must_use]
    pub fn parse(token: &str) -> Option<Self> {
        match token.trim().to_ascii_uppercase().as_str() {
            "LR" => Some(Self::Lr),
            "RL" => Some(Self::Rl),
            "TD" | "TB" => Some(Self::Td),
            "BT" => Some(Self::Bt),
            _ => None,
        }
    }

    /// `true` when layers are laid out along the x axis (`LR`, `RL`).
    #[must_use]
    pub const fn is_horizontal(self) -> bool {
        matches!(self, Self::Lr | Self::Rl)
    }

    /// `true` when the first layer is placed last (`RL`, `BT`).
    #[must_use]
    pub const fn is_reversed(self) -> bool {
        matches!(self, Self::Rl | Self::Bt)
    }

    /// Canonical Mermaid spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Lr => "LR",
            Self::Rl => "RL",
            Self::Td => "TD",
            Self::Bt => "BT",
        }
    }
}

impl fmt::Display for Orientation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Node outline. Mermaid shapes that diple does not draw natively degrade to
/// [`NodeShape::Rect`] at parse time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum NodeShape {
    /// `A[label]`.
    #[default]
    Rect,
    /// `A(label)`.
    Round,
    /// `A([label])`.
    Stadium,
    /// `A{label}`.
    Diamond,
    /// `A((label))`.
    Circle,
    /// `A{{label}}`.
    Hexagon,
    /// `A[[label]]`.
    Subroutine,
    /// `A[(label)]`.
    Cylinder,
}

/// A flowchart node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagramNode {
    /// Mermaid identifier as written in the source.
    pub id: String,
    /// Display label (defaults to the identifier when none was given).
    pub label: String,
    /// Outline shape.
    pub shape: NodeShape,
}

/// Line style of an edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum EdgeStyle {
    /// `-->`, `---`.
    #[default]
    Solid,
    /// `-.->`, `-.-`.
    Dotted,
    /// `==>`, `===`.
    Thick,
    /// `~~~` (drawn as empty space).
    Invisible,
}

/// Arrow decoration of an edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ArrowKind {
    /// One arrow head at the target end (`-->`).
    #[default]
    Arrow,
    /// No arrow head (`---`).
    None,
    /// Arrow heads at both ends (`<-->`).
    DoubleArrow,
}

/// A directed edge between two nodes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagramEdge {
    /// Index into [`Diagram::nodes`] of the source node.
    pub from: usize,
    /// Index into [`Diagram::nodes`] of the target node.
    pub to: usize,
    /// Optional edge label (`-->|text|` or `-- text -->`).
    pub label: Option<String>,
    /// Line style.
    pub style: EdgeStyle,
    /// Arrow decoration.
    pub arrow: ArrowKind,
}

/// A `subgraph ... end` group.
///
/// diple parses subgraphs so that the contained nodes are laid out, but the
/// native terminal renderer does **not** draw the grouping box; see
/// [`Diagram::has_unsupported_features`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subgraph {
    /// Title as written (identifier or quoted string), empty when unnamed.
    pub title: String,
    /// Identifiers of the nodes declared inside the subgraph, in source order.
    pub node_ids: Vec<String>,
}

/// A parsed flowchart.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Diagram {
    /// Flow direction from the header line.
    pub orientation: Orientation,
    /// Nodes in first-mention order. Edge endpoints index into this vector.
    pub nodes: Vec<DiagramNode>,
    /// Edges in source order.
    pub edges: Vec<DiagramEdge>,
    /// Subgraph groups in source order (nodes are laid out, boxes are not drawn).
    pub subgraphs: Vec<Subgraph>,
    /// `true` when the source used constructs the native renderer cannot show
    /// faithfully (currently: subgraph grouping boxes). The renderer appends a
    /// note line so the reader knows something was dropped.
    pub has_unsupported_features: bool,
    /// Human-readable notes matching [`Diagram::has_unsupported_features`].
    pub unsupported_notes: Vec<String>,
}

impl Diagram {
    /// Looks up a node index by identifier.
    #[must_use]
    pub fn node_index(&self, id: &str) -> Option<usize> {
        self.nodes.iter().position(|n| n.id == id)
    }

    /// Records a construct that is parsed but not rendered faithfully.
    pub(crate) fn note_unsupported(&mut self, note: impl Into<String>) {
        let note = note.into();
        if !self.unsupported_notes.contains(&note) {
            self.unsupported_notes.push(note);
        }
        self.has_unsupported_features = true;
    }
}
