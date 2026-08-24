//! Semantic document model — Workstream B.
//!
//! The pipeline is `Markdown text → [`parser::parse`] → [`Document`]`. The
//! document owns everything that is independent of terminal geometry: the
//! block/inline AST, the section hierarchy used for folding and navigation,
//! heading anchors, the ordered link list and the search index.
//!
//! Nothing in this module knows about widths, colours or terminals.

pub mod anchors;
pub mod ast;
pub mod links;
pub mod parser;
pub mod search;
pub mod sections;

pub use anchors::AnchorIndex;
pub use ast::{
    Alignment, CodeBlock, Document, Footnote, Heading, Image, Inline, Inlines, LinkId, List,
    ListItem, MermaidBlock, Node, NodeId, NodeKind, SourceSpan, Table,
};
pub use links::{Link, LinkKind};
pub use parser::parse;
pub use search::{Match, SearchIndex};
pub use sections::{FoldState, Section, SectionId, TocEntry};
