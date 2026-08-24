//! Table of contents state.
//!
//! The entry list mirrors the document's section hierarchy exactly — it is
//! derived from [`crate::document::Section`], never from rendered lines — and
//! carries its own selection and scroll offset so that long documents stay
//! navigable.

use crate::document::{Document, SectionId, TocEntry};

/// Default sidebar width in columns, capped to a third of the screen.
pub(crate) const DEFAULT_WIDTH: u16 = 28;

/// Sidebar visibility, selection and scroll offset.
#[derive(Debug, Clone, Default)]
pub(crate) struct TocState {
    /// Whether the sidebar is drawn.
    pub(crate) open: bool,
    /// Index of the selected entry.
    pub(crate) selected: usize,
    /// Index of the first drawn entry.
    pub(crate) scroll: usize,
    /// Entries in document order.
    pub(crate) entries: Vec<TocEntry>,
}

/// Build the entry list from the document's sections.
pub(crate) fn entries(doc: &Document) -> Vec<TocEntry> {
    let mut out = Vec::with_capacity(doc.sections.len());
    for section in &doc.sections {
        let Some(heading) = doc.heading_of(section.id) else {
            continue;
        };
        out.push(TocEntry {
            section: section.id,
            depth: depth_of(doc, section.id),
            text: heading.text.clone(),
        });
    }
    out
}

fn depth_of(doc: &Document, section: SectionId) -> usize {
    let mut depth = 0;
    let mut current = doc.sections.get(section).and_then(|s| s.parent);
    while let Some(parent) = current {
        depth += 1;
        current = doc.sections.get(parent).and_then(|s| s.parent);
        if depth > doc.sections.len() {
            break; // defensive: never loop on a malformed hierarchy
        }
    }
    depth
}

impl TocState {
    /// Build the state for a document (closed).
    pub(crate) fn new(doc: &Document) -> Self {
        Self {
            open: false,
            selected: 0,
            scroll: 0,
            entries: entries(doc),
        }
    }

    /// Number of entries.
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the document has no headings.
    pub(crate) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Sidebar width for a screen of `total` columns.
    pub(crate) fn width(&self, total: u16) -> u16 {
        DEFAULT_WIDTH
            .min(total / 3)
            .max(if total >= 12 { 12 } else { total })
    }

    /// The section the selected entry refers to.
    pub(crate) fn selected_section(&self) -> Option<SectionId> {
        self.entries.get(self.selected).map(|e| e.section)
    }

    /// The entry index for a section id.
    pub(crate) fn index_of(&self, section: SectionId) -> Option<usize> {
        self.entries.iter().position(|e| e.section == section)
    }

    /// Move the selection by `delta` entries, clamped.
    pub(crate) fn move_selection(&mut self, delta: isize, height: usize) {
        if self.entries.is_empty() {
            return;
        }
        let last = self.entries.len() - 1;
        let next = self.selected as isize + delta;
        self.selected = next.clamp(0, last as isize) as usize;
        self.scroll_into_view(height);
    }

    /// Select an entry by index (mouse click), clamped.
    pub(crate) fn select(&mut self, index: usize, height: usize) {
        if self.entries.is_empty() {
            return;
        }
        self.selected = index.min(self.entries.len() - 1);
        self.scroll_into_view(height);
    }

    /// Keep the selection inside a window of `height` rows.
    pub(crate) fn scroll_into_view(&mut self, height: usize) {
        let height = height.max(1);
        if self.selected < self.scroll {
            self.scroll = self.selected;
        } else if self.selected >= self.scroll + height {
            self.scroll = self.selected + 1 - height;
        }
        let max_scroll = self.entries.len().saturating_sub(height);
        self.scroll = self.scroll.min(max_scroll);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::parse;

    const DOC: &str = "# One\n\ntext\n\n## Two\n\ntext\n\n### Three\n\ntext\n\n# Four\n\ntext\n";

    #[test]
    fn entries_mirror_the_hierarchy() {
        let doc = parse(DOC);
        let toc = TocState::new(&doc);
        let shape: Vec<(usize, &str)> = toc
            .entries
            .iter()
            .map(|e| (e.depth, e.text.as_str()))
            .collect();
        assert_eq!(
            shape,
            vec![(0, "One"), (1, "Two"), (2, "Three"), (0, "Four")]
        );
    }

    #[test]
    fn selection_clamps_and_scrolls() {
        let doc = parse(DOC);
        let mut toc = TocState::new(&doc);
        toc.move_selection(-5, 2);
        assert_eq!(toc.selected, 0);
        toc.move_selection(99, 2);
        assert_eq!(toc.selected, 3);
        assert_eq!(toc.scroll, 2, "scrolled to keep the selection visible");
        toc.move_selection(-99, 2);
        assert_eq!(toc.selected, 0);
        assert_eq!(toc.scroll, 0);
    }

    #[test]
    fn selected_section_maps_back() {
        let doc = parse(DOC);
        let mut toc = TocState::new(&doc);
        toc.select(2, 10);
        let section = toc.selected_section().expect("section");
        assert_eq!(
            doc.heading_of(section).map(|h| h.text.as_str()),
            Some("Three")
        );
        assert_eq!(toc.index_of(section), Some(2));
    }

    #[test]
    fn width_is_bounded() {
        let doc = parse(DOC);
        let toc = TocState::new(&doc);
        assert_eq!(toc.width(120), DEFAULT_WIDTH);
        assert_eq!(toc.width(60), 20);
        assert!(toc.width(10) <= 10);
    }
}
