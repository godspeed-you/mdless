//! Moving through the document: folding, heading jumps, search and links
//!
//! These four groups share one file because they share one job — they all
//! change *where the reader is* — and because they call each other constantly.
//! A search jump reveals a match inside a collapsed section, which unfolds it;
//! following an internal link resolves an anchor to a node and then reveals
//! it exactly the way a search match is revealed; a heading jump and a link
//! jump both scroll with the same leading context. Splitting them into
//! `folds.rs`, `search.rs` and `links.rs` would be four short files tied
//! together by call edges crossing every boundary.
//!
//! Two invariants hold across the whole file. Position is always expressed as
//! a node, never a line number, so it survives the re-layout a fold or a
//! reveal may cause. And every path that changes fold state ends in
//! [`App::after_section_fold`] or [`App::after_fold_change`], the two entry
//! points into the [`layout_cache`](super::layout_cache) splice-or-rebuild
//! decision.
//!
//! Opening an external link spawns a detached child process; [`App::reap_children`]
//! is called from the event loop so those never become zombies.

use std::process::{Command, Stdio};

use super::{App, Mode};
use crate::document::{LinkId, LinkKind, NodeId, NodeKind, SectionId};

/// Which fold operation a key requested.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FoldOp {
    Toggle,
    Collapse,
    Expand,
}

impl App {
    // -- folding ----------------------------------------------------------

    /// A fold changed somewhere in the document: rebuild everything.
    ///
    /// Only [`Action::CollapseAll`]/[`Action::ExpandAll`] still need this;
    /// a single section takes [`App::after_section_fold`].
    pub(super) fn after_fold_change(&mut self) {
        self.invalidate();
        self.ensure_layout();
    }

    /// A single section was collapsed or expanded.
    ///
    /// Folding changes one contiguous run of top-level nodes and nothing else,
    /// so the cached tree is spliced instead of rebuilt, which avoids a
    /// full-document redraw. A full rebuild is still the fallback whenever the
    /// splice is not provably equivalent.
    pub(super) fn after_section_fold(&mut self, section: SectionId) {
        if self.splice_section(section) {
            // The tree now matches the new fold state, so the build key does
            // too; `ensure_layout` must not rebuild what was just spliced.
            self.built = Some(self.build_key());
            self.restore_anchor();
            self.prepare_frame();
        } else {
            self.after_fold_change();
        }
    }

    /// The outermost ancestor of `section` (itself when it has no parent).
    fn outermost_section(&self, section: SectionId) -> SectionId {
        let mut current = section;
        while let Some(parent) = self.doc.sections.get(current).and_then(|s| s.parent) {
            current = parent;
        }
        current
    }

    pub(super) fn fold_current(&mut self, op: FoldOp) {
        let Some(section) = self.current_section() else {
            self.set_message("no section here");
            return;
        };
        match op {
            FoldOp::Toggle => {
                self.folds.toggle(section);
            }
            FoldOp::Collapse => self.folds.collapse(section),
            FoldOp::Expand => self.folds.expand(section),
        }
        // Keep the section heading at the top so nested folds stay
        // predictable.
        if let Some(s) = self.doc.sections.get(section) {
            self.anchor = (s.heading, 0);
            self.cursor = s.heading;
        }
        self.after_section_fold(section);
    }

    // -- heading navigation ------------------------------------------------

    /// Index into [`RenderTree::heading_lines`] of the heading at or above the
    /// cursor.
    ///
    /// Comparing *nodes* rather than line numbers keeps this correct even
    /// though a node owns the blank spacing line in front of it.
    fn heading_index(&self) -> Option<usize> {
        let entries = self.tree.heading_lines();
        if let Some(node) = self.cursor_node() {
            if let Some(index) = entries.iter().position(|(_, n, _)| *n == node) {
                return Some(index);
            }
        }
        let line = self.cursor_line();
        entries.iter().rposition(|(l, _, _)| *l <= line)
    }

    /// Whether the cursor sits on a heading (or on a collapsed marker).
    pub(super) fn cursor_on_heading(&self) -> bool {
        self.cursor_node()
            .and_then(|n| self.doc.node(n))
            .map(|n| matches!(n.kind, NodeKind::Heading(_)))
            .unwrap_or(false)
    }

    pub(super) fn jump_heading(&mut self, forward: bool) {
        let entries: Vec<(usize, NodeId, u8)> = self.tree.heading_lines().to_vec();
        if entries.is_empty() {
            self.set_message("document has no headings");
            return;
        }
        let current = self.heading_index();
        let target = if forward {
            match current {
                Some(i) => entries.get(i + 1).copied(),
                None => entries.first().copied(),
            }
        } else {
            match current {
                Some(i) if self.cursor_on_heading() => {
                    i.checked_sub(1).and_then(|p| entries.get(p).copied())
                }
                Some(i) => entries.get(i).copied(),
                None => None,
            }
        };
        match target {
            Some((line, _, _)) => self.scroll_with_context(line),
            None => self.set_message(if forward {
                "no further heading"
            } else {
                "no previous heading"
            }),
        }
    }

    pub(super) fn jump_heading_same_level(&mut self, forward: bool) {
        let Some(section) = self.section_at_cursor_for_navigation() else {
            self.jump_heading(forward);
            return;
        };
        let level = self.doc.sections.get(section).map(|s| s.level).unwrap_or(1);
        let mut current = section;
        loop {
            let next = if forward {
                self.doc.next_section_at_or_above(current, level)
            } else {
                self.doc.previous_section_at_or_above(current, level)
            };
            let Some(id) = next else {
                self.set_message(if forward {
                    "no further heading at this level"
                } else {
                    "no previous heading at this level"
                });
                return;
            };
            let heading = self.doc.sections.get(id).map(|s| s.heading);
            if let Some(node) = heading {
                if let Some(line) = self.tree.first_line_of(node) {
                    self.scroll_with_context(line);
                    return;
                }
            }
            current = id;
        }
    }

    /// The section used as the origin of a same-level jump: the section whose
    /// heading is at or above the cursor.
    fn section_at_cursor_for_navigation(&self) -> Option<SectionId> {
        let node = self.cursor_node()?;
        self.doc.section_of(node)
    }

    // -- search -----------------------------------------------------------

    pub(super) fn open_search(&mut self) {
        self.search.saved = self.search.committed.clone();
        self.search.query.clear();
        self.mode = Mode::Search;
        self.refresh_search_preview();
    }

    /// Incremental search: refresh matches and preview the first one at or
    /// after the current position.
    pub(super) fn refresh_search_preview(&mut self) {
        self.search.refresh(&self.index);
        // No `invalidate()`: the query is not a layout input any more, so an
        // incremental search never rebuilds the document.
        self.prepare_frame();
        let from = self.cursor_node().unwrap_or(0);
        if self.search.select_near(from).is_some() {
            self.goto_current_match();
        }
    }

    pub(super) fn cycle_search(&mut self, forward: bool) {
        if !self.search.has_matches() {
            self.set_message(if self.search.committed.is_empty() {
                "no active search".to_string()
            } else {
                format!("pattern not found: {}", self.search.committed)
            });
            return;
        }
        let wrapped = if forward {
            self.search.next_match()
        } else {
            self.search.previous_match()
        };
        self.goto_current_match();
        if wrapped {
            self.set_message(if forward {
                "search wrapped to the top"
            } else {
                "search wrapped to the bottom"
            });
        }
    }

    /// Scroll to the current match, revealing a collapsed section if the match
    /// is hidden inside one.
    pub(super) fn goto_current_match(&mut self) {
        let Some(m) = self.search.current_match() else {
            return;
        };
        self.reveal_node(m.node);
        match self.match_line(m.node) {
            Some(line) => self.scroll_with_context(line),
            None => self.set_message("match is not visible"),
        }
    }

    /// Expand every collapsed ancestor of `node` and re-layout.
    pub(super) fn reveal_node(&mut self, node: NodeId) {
        if !self.doc.is_hidden(node, &self.folds) {
            return;
        }
        if let Some(section) = self.doc.section_of(node) {
            self.folds.reveal(section);
            self.folds.expand(section);
            // `reveal` expands every collapsed ancestor, so the outermost one
            // delimits the range that changed.
            self.after_section_fold(self.outermost_section(section));
        }
    }

    /// The line inside `node` that carries the highlighted match, or the
    /// node's first line.
    fn match_line(&self, node: NodeId) -> Option<usize> {
        let first = self.tree.first_line_of(node)?;
        let mut end = first;
        while self.tree.lines.get(end).map(|l| l.node) == Some(node) {
            end += 1;
        }
        // The node's lines may not be painted yet (painting follows the
        // viewport), so the text is searched directly.
        self.tree
            .find_line_with(
                first,
                end,
                &self.search.committed,
                self.search.case_sensitive,
            )
            .or(Some(first))
    }

    // -- links ------------------------------------------------------------

    /// Link ids that are visible in the current render tree, in document
    /// order. Links inside collapsed sections are absent by construction.
    pub(crate) fn visible_links(&self) -> Vec<LinkId> {
        let mut out: Vec<LinkId> = Vec::new();
        for line in &self.tree.lines {
            for span in &line.spans {
                if let Some(id) = span.link {
                    if !out.contains(&id) {
                        out.push(id);
                    }
                }
            }
        }
        out
    }

    pub(super) fn cycle_link(&mut self, forward: bool) {
        let links = self.visible_links();
        if links.is_empty() {
            self.set_message("no links in view");
            return;
        }
        let next = match self
            .selected_link
            .and_then(|id| links.iter().position(|l| *l == id))
        {
            Some(pos) if forward => (pos + 1) % links.len(),
            Some(pos) => (pos + links.len() - 1) % links.len(),
            None if forward => 0,
            None => links.len() - 1,
        };
        let id = links[next];
        self.selected_link = Some(id);
        if let Some(line) = self.link_line(id) {
            self.reveal_line(line);
        }
        if let Some(link) = self.doc.links.get(id) {
            self.set_message(format!("link: {}", link.url));
        }
    }

    fn link_line(&self, id: LinkId) -> Option<usize> {
        self.tree
            .lines
            .iter()
            .position(|l| l.spans.iter().any(|s| s.link == Some(id)))
    }

    /// `Enter`: toggle the section when the cursor is on a heading, otherwise
    /// open the selected link.
    pub(super) fn activate(&mut self) {
        if self.mode == Mode::Toc {
            self.toc_jump();
            return;
        }
        if self.mode == Mode::Help {
            self.mode = Mode::Normal;
            return;
        }
        if self.cursor_on_heading() {
            self.fold_current(FoldOp::Toggle);
        } else {
            self.open_selected_link();
        }
    }

    pub(super) fn open_selected_link(&mut self) {
        let Some(id) = self
            .selected_link
            .or_else(|| self.visible_links().first().copied())
        else {
            self.set_message("no link selected");
            return;
        };
        self.selected_link = Some(id);
        let Some(link) = self.doc.links.get(id).cloned() else {
            self.set_message("no link selected");
            return;
        };
        match link.kind {
            LinkKind::Internal(anchor) => self.jump_to_anchor(&anchor),
            LinkKind::External | LinkKind::Relative => self.spawn_opener(&link.url),
        }
    }

    /// Follow an internal `#anchor` link.
    pub(crate) fn jump_to_anchor(&mut self, target: &str) {
        let anchor = target.trim_start_matches('#');
        let Some(node) = self.doc.anchors.resolve(anchor) else {
            self.set_message(format!("unknown anchor: #{anchor}"));
            return;
        };
        self.reveal_node(node);
        self.ensure_layout();
        match self.tree.first_line_of(node) {
            Some(line) => self.scroll_with_context(line),
            None => self.set_message(format!("#{anchor} is not visible")),
        }
    }

    /// Spawn the configured opener for an external link, fully detached so a
    /// misbehaving child can never corrupt the terminal.
    fn spawn_opener(&mut self, url: &str) {
        let opener = self.config.links.opener.clone();
        let result = Command::new(&opener)
            .arg(url)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        match result {
            Ok(child) => {
                self.children.push(child);
                self.set_message(format!("opening {url}"));
            }
            Err(e) => self.set_message(format!("cannot run `{opener}`: {e}")),
        }
    }

    /// Reap finished opener processes (called from the event loop).
    pub(crate) fn reap_children(&mut self) {
        self.children
            .retain_mut(|c| !matches!(c.try_wait(), Ok(Some(_))));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::state::test_support::*;
    use crate::app::state::HEADING_CONTEXT;
    use crate::config::actions::Action;
    use crossterm::event::KeyCode;

    #[test]
    fn heading_navigation_is_semantic_and_ordered() {
        let mut a = app();
        let mut visited = Vec::new();
        for _ in 0..4 {
            a.apply(Action::NextHeading);
            if let Some(node) = a.tree().node_at(a.cursor_line()) {
                if let Some(crate::document::Node {
                    kind: NodeKind::Heading(h),
                    ..
                }) = a.doc.node(node)
                {
                    visited.push(h.text.clone());
                }
            }
        }
        assert_eq!(visited, vec!["Title", "Alpha", "Alpha Child", "Beta"]);
        a.apply(Action::PreviousHeading);
        let node = a.cursor_node().and_then(|n| a.doc.node(n));
        assert!(
            matches!(node.map(|n| &n.kind), Some(NodeKind::Heading(h)) if h.text == "Alpha Child")
        );
    }

    #[test]
    fn same_level_jumps_skip_nested_headings() {
        let mut a = app();
        a.apply(Action::NextHeading); // Title (H1)
        a.apply(Action::NextHeading); // Alpha (H2)
        a.apply(Action::NextHeadingSameLevel);
        let node = a.cursor_node().and_then(|n| a.doc.node(n));
        assert!(
            matches!(node.map(|n| &n.kind), Some(NodeKind::Heading(h)) if h.text == "Beta"),
            "H2 → H2 skipped the H3, got {node:?}"
        );
        a.apply(Action::PreviousHeadingSameLevel);
        let node = a.cursor_node().and_then(|n| a.doc.node(n));
        assert!(matches!(node.map(|n| &n.kind), Some(NodeKind::Heading(h)) if h.text == "Alpha"));
    }

    #[test]
    fn folding_hides_nested_children() {
        let mut a = app();
        a.apply(Action::NextHeading); // Title
        a.apply(Action::NextHeading); // Alpha
        let before = a.tree().len();
        a.apply(Action::ToggleFold);
        let after = a.tree().len();
        assert!(after < before, "collapsing removed lines");
        let text = a.tree().to_plain_text();
        assert!(!text.contains("Alpha Child"), "nested heading hidden");
        assert!(!text.contains("Alpha body"), "body hidden");
        assert!(text.contains("Beta"), "the sibling stays visible");
        a.apply(Action::ToggleFold);
        assert_eq!(a.tree().len(), before, "expanding restored the lines");
    }

    #[test]
    fn collapse_all_and_expand_all() {
        let mut a = app();
        let expanded = a.tree().len();
        a.apply(Action::CollapseAll);
        let collapsed = a.tree().len();
        assert!(collapsed < expanded);
        assert!(!a.tree().to_plain_text().contains("Alpha body"));
        a.apply(Action::ExpandAll);
        assert_eq!(a.tree().len(), expanded);
    }

    #[test]
    fn enter_on_a_heading_toggles_the_fold() {
        let mut a = app();
        a.apply(Action::NextHeading);
        a.apply(Action::NextHeading);
        let before = a.tree().len();
        a.apply(Action::Activate);
        assert!(a.tree().len() < before);
    }

    #[test]
    fn search_finds_cycles_and_wraps() {
        let mut a = app();
        key(&mut a, '/');
        for c in "needle".chars() {
            key(&mut a, c);
        }
        assert_eq!(a.mode(), Mode::Search);
        assert_eq!(a.search.matches.len(), 2, "incremental search found both");
        code(&mut a, KeyCode::Enter);
        assert_eq!(a.mode(), Mode::Normal);
        assert_eq!(a.search.current, 0);
        a.apply(Action::NextSearch);
        assert_eq!(a.search.current, 1);
        a.apply(Action::NextSearch);
        assert_eq!(a.search.current, 0, "wrapped");
        assert!(a.message().unwrap_or_default().contains("wrapped"));
    }

    #[test]
    fn search_cancel_restores_the_previous_query() {
        let mut a = app();
        key(&mut a, '/');
        for c in "needle".chars() {
            key(&mut a, c);
        }
        code(&mut a, KeyCode::Enter);
        key(&mut a, '/');
        for c in "zzz".chars() {
            key(&mut a, c);
        }
        code(&mut a, KeyCode::Esc);
        assert_eq!(a.search.committed, "needle");
        assert_eq!(a.search.matches.len(), 2);
    }

    #[test]
    fn search_reveals_a_match_inside_a_collapsed_section() {
        let mut a = app();
        a.apply(Action::CollapseAll);
        assert!(!a.tree().to_plain_text().contains("needle"));
        key(&mut a, '/');
        for c in "needle".chars() {
            key(&mut a, c);
        }
        code(&mut a, KeyCode::Enter);
        let text = a.tree().to_plain_text();
        assert!(text.contains("needle"), "the section was revealed");
        let m = a.search.current_match().expect("a current match");
        assert!(!a.doc.is_hidden(m.node, &a.folds));
    }

    #[test]
    fn backspace_narrows_and_widens_the_result_set() {
        let mut a = app();
        key(&mut a, '/');
        for c in "needlez".chars() {
            key(&mut a, c);
        }
        assert!(a.search.matches.is_empty());
        code(&mut a, KeyCode::Backspace);
        assert_eq!(a.search.matches.len(), 2);
    }

    #[test]
    fn link_cycling_skips_hidden_links() {
        let mut a = app();
        let all = a.visible_links();
        assert_eq!(all.len(), 3, "lead, intro and internal anchor links");
        a.apply(Action::CollapseAll);
        let visible = a.visible_links();
        assert!(
            visible.len() < all.len(),
            "links inside collapsed sections are gone: {visible:?}"
        );
        a.apply(Action::NextLink);
        let selected = a.selected_link().expect("a selection");
        assert!(visible.contains(&selected));
    }

    #[test]
    fn internal_anchor_links_jump_to_the_right_node() {
        let mut a = app();
        a.jump_to_anchor("#alpha");
        let node = a.doc.anchors.resolve("alpha").expect("anchor");
        let line = a.tree().first_line_of(node).expect("visible");
        assert_eq!(a.top_line(), line.saturating_sub(HEADING_CONTEXT));
        a.jump_to_anchor("#does-not-exist");
        assert!(a.message().unwrap_or_default().contains("unknown anchor"));
    }

    #[test]
    fn an_unknown_opener_reports_instead_of_crashing() {
        let mut a = app();
        a.config.links.opener = "definitely-not-a-real-opener".to_string();
        a.apply(Action::NextLink);
        a.apply(Action::OpenLink);
        let message = a.message().unwrap_or_default();
        assert!(
            message.contains("definitely-not-a-real-opener"),
            "{message}"
        );
        assert!(!a.should_quit());
    }
}
