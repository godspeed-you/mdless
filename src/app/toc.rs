//! Table of contents state.
//!
//! The entry list mirrors the document's section hierarchy exactly — it is
//! derived from [`crate::document::Section`], never from rendered lines — and
//! carries its own selection and scroll offset so that long documents stay
//! navigable.

use crate::document::{Document, SectionId, TocEntry};

/// Widest the sidebar may grow, however long the headings are.
///
/// The same number as [`crate::app::hints::MIN_DOCUMENT_WIDTH`], and for the
/// same reason: 40 columns is the narrowest thing this program still calls
/// readable, so it is also the most a navigation aid may take from the text.
/// A heading that does not fit is scrolled to, not accommodated.
pub(crate) const MAX_WIDTH: u16 = 40;

/// Narrowest the sidebar may shrink on a screen with room for it.
pub(crate) const MIN_WIDTH: u16 = 12;

/// Sidebar visibility, selection and scroll offset.
#[derive(Debug, Clone, Default)]
pub(crate) struct TocState {
    /// Whether the sidebar is drawn.
    pub(crate) open: bool,
    /// Index of the selected entry.
    pub(crate) selected: usize,
    /// Index of the first drawn entry.
    pub(crate) scroll: usize,
    /// First visible column, for headings wider than [`MAX_WIDTH`].
    pub(crate) h_scroll: usize,
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
            h_scroll: 0,
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

    /// Widest entry row in columns, borders excluded.
    ///
    /// One column for the current-section marker, two per nesting level for
    /// the tree connectors, and the heading itself. Both the marker and the
    /// connectors are one cell wide in either glyph set, so this does not
    /// depend on whether the terminal draws them in Unicode or ASCII.
    pub(crate) fn content_width(&self) -> usize {
        self.entries
            .iter()
            .map(|e| 1 + 2 * e.depth + crate::util::unicode::width(&e.text))
            .max()
            .unwrap_or(0)
    }

    /// Sidebar width for a screen of `total` columns, including the border.
    ///
    /// The sidebar is as wide as its widest entry and no wider, so a document
    /// of short headings gives the columns it does not need back to the text.
    /// Two ceilings bound it: [`MAX_WIDTH`], and a third of the screen so it
    /// can never dominate a narrow terminal. Whatever the ceilings cut off is
    /// reachable by scrolling the sidebar sideways.
    pub(crate) fn width(&self, total: u16) -> u16 {
        let wanted = u16::try_from(self.content_width().saturating_add(1)).unwrap_or(u16::MAX);
        wanted
            .min(MAX_WIDTH)
            .min(total / 3)
            .max(if total >= MIN_WIDTH { MIN_WIDTH } else { total })
            .min(total)
    }

    /// Columns the entries extend past an inner width of `inner`.
    pub(crate) fn max_h_scroll(&self, inner: usize) -> usize {
        self.content_width().saturating_sub(inner)
    }

    /// Scroll the entries sideways by `delta` columns, clamped.
    pub(crate) fn scroll_h(&mut self, delta: isize, inner: usize) {
        let max = self.max_h_scroll(inner) as isize;
        self.h_scroll = (self.h_scroll as isize + delta).clamp(0, max.max(0)) as usize;
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
    fn width_follows_the_widest_entry_within_its_ceilings() {
        let doc = parse(DOC);
        let toc = TocState::new(&doc);
        // "  └ Three" — marker, two levels of connector, five letters.
        assert_eq!(toc.content_width(), 10);
        // Short headings do not claim the full ceiling: content plus border,
        // lifted to the floor a usable sidebar needs.
        assert_eq!(toc.width(120), MIN_WIDTH);

        let long = parse(&format!("# {}\n\ntext\n", "a".repeat(80)));
        let wide = TocState::new(&long);
        assert_eq!(wide.width(200), MAX_WIDTH, "capped by MAX_WIDTH");
        assert_eq!(wide.width(60), 20, "capped by a third of the screen");
        assert_eq!(wide.width(30), MIN_WIDTH, "the floor beats a small third");
        assert!(wide.width(10) <= 10, "never wider than the screen itself");
    }

    #[test]
    fn horizontal_scrolling_is_clamped_to_the_overflow() {
        let long = parse(&format!("# {}\n\ntext\n", "a".repeat(80)));
        let mut toc = TocState::new(&long);
        // 1 marker + 80 letters, shown through the 39 inner columns of a
        // sidebar at MAX_WIDTH.
        assert_eq!(toc.content_width(), 81);
        assert_eq!(toc.max_h_scroll(39), 42);

        toc.scroll_h(8, 39);
        assert_eq!(toc.h_scroll, 8);
        toc.scroll_h(999, 39);
        assert_eq!(toc.h_scroll, 42, "never past the last column of the text");
        toc.scroll_h(-999, 39);
        assert_eq!(toc.h_scroll, 0, "and never before the first");

        // Nothing to scroll when everything already fits.
        let doc = parse(DOC);
        let mut fits = TocState::new(&doc);
        assert_eq!(fits.max_h_scroll(39), 0);
        fits.scroll_h(8, 39);
        assert_eq!(fits.h_scroll, 0);
    }
}
