//! Key and mouse input, and the [`Action`] dispatcher they both feed
//!
//! The boundary here is *events in, one `Action` out, then dispatch*. Modal
//! keys are consumed first — the `/` prompt, the TOC pane and the help
//! overlay each read the raw event before the key map ever sees it — and only
//! in [`Mode::Normal`] does the key map get a chance to resolve a binding,
//! possibly after buffering a prefix.
//!
//! [`App::apply`] is the single place where an [`Action`] becomes an effect,
//! whatever produced it: a key, a mouse click on a hint row, or a caller. It
//! is deliberately a flat, exhaustive `match`, so the compiler reports a new
//! action variant that nobody handles. Most arms are one call into
//! [`navigate`](super::navigate) or [`sidebars`](super::sidebars), which is
//! the cost of this split: reading what an action *does* takes a hop into
//! another file.
//!
//! Mouse handling is here for the same reason — a click is resolved to a
//! screen region and then turned into the same actions a key produces, so
//! there is exactly one behaviour to keep consistent.

use super::navigate::FoldOp;
use super::{App, Mode};
use crate::config::actions::Action;
use crate::config::keys::KeyMatch;
use crate::render::primitives::LineKind;
use crate::render::terminal::{HintLine, KeyHintsSidebar};

impl App {
    // -- key handling -----------------------------------------------------

    /// Feed one key event. Sub-modes consume keys before the key map.
    pub(crate) fn handle_key(&mut self, event: &crossterm::event::KeyEvent) {
        use crossterm::event::{KeyCode, KeyEventKind, KeyModifiers};
        if event.kind == KeyEventKind::Release {
            return;
        }
        self.clear_message();

        // Ctrl-C always quits, whatever the mode.
        if event.code == KeyCode::Char('c') && event.modifiers.contains(KeyModifiers::CONTROL) {
            self.quit = true;
            return;
        }

        if self.mode == Mode::Search {
            self.handle_search_key(event);
            return;
        }
        if self.mode == Mode::Message {
            self.mode = Mode::Normal;
        }

        match self.keymap.feed(event) {
            KeyMatch::Action(action) => {
                self.pending.clear();
                self.apply(action);
            }
            KeyMatch::Pending => {
                self.pending = self
                    .keymap
                    .pending()
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("");
            }
            KeyMatch::None => {
                self.pending.clear();
                // Unhandled keys are ignored silently, except the sub-mode
                // conveniences below.
                if self.mode == Mode::Toc {
                    self.handle_toc_key(event);
                } else if self.mode == Mode::Help {
                    self.handle_help_key(event);
                }
            }
        }
    }

    fn handle_search_key(&mut self, event: &crossterm::event::KeyEvent) {
        use crossterm::event::KeyCode;
        match event.code {
            KeyCode::Esc => {
                self.search.query = self.search.saved.clone();
                self.search.refresh(&self.index);
                self.mode = Mode::Normal;
                self.prepare_frame();
            }
            KeyCode::Enter => {
                self.mode = Mode::Normal;
                if self.search.has_matches() {
                    self.goto_current_match();
                } else if !self.search.query.is_empty() {
                    self.set_message(format!("pattern not found: {}", self.search.query));
                }
            }
            KeyCode::Backspace => {
                self.search.query.pop();
                self.refresh_search_preview();
            }
            KeyCode::Char(c)
                if !event
                    .modifiers
                    .contains(crossterm::event::KeyModifiers::CONTROL) =>
            {
                self.search.query.push(c);
                self.refresh_search_preview();
            }
            _ => {}
        }
    }

    fn handle_toc_key(&mut self, event: &crossterm::event::KeyEvent) {
        use crossterm::event::KeyCode;
        let height = self.content_height();
        match event.code {
            KeyCode::Char('j') | KeyCode::Down => self.toc.move_selection(1, height),
            KeyCode::Char('k') | KeyCode::Up => self.toc.move_selection(-1, height),
            KeyCode::Home => self.toc.select(0, height),
            KeyCode::End => self.toc.select(usize::MAX, height),
            _ => {}
        }
    }

    fn handle_help_key(&mut self, event: &crossterm::event::KeyEvent) {
        use crossterm::event::KeyCode;
        match event.code {
            KeyCode::Char('j') | KeyCode::Down => self.help_scroll += 1,
            KeyCode::Char('k') | KeyCode::Up => {
                self.help_scroll = self.help_scroll.saturating_sub(1)
            }
            KeyCode::Char('q') | KeyCode::Esc => {
                self.mode = Mode::Normal;
                self.help_scroll = 0;
            }
            _ => {}
        }
    }

    // -- actions ----------------------------------------------------------

    /// Execute one [`Action`]. Every variant does something real.
    pub(crate) fn apply(&mut self, action: Action) {
        let height = self.content_height();
        match action {
            Action::Quit => match self.mode {
                Mode::Help | Mode::Toc => {
                    self.close_overlays();
                }
                _ => self.quit = true,
            },
            Action::Cancel => self.close_overlays(),
            Action::ScrollDown => match self.mode {
                Mode::Toc => self.toc.move_selection(1, height),
                Mode::Help => self.help_scroll += 1,
                _ => self.scroll_by(1),
            },
            Action::ScrollUp => match self.mode {
                Mode::Toc => self.toc.move_selection(-1, height),
                Mode::Help => self.help_scroll = self.help_scroll.saturating_sub(1),
                _ => self.scroll_by(-1),
            },
            Action::PageDown => match self.mode {
                Mode::Toc => self.toc.move_selection(height as isize, height),
                Mode::Help => self.help_scroll += height,
                _ => self.scroll_by(height as isize),
            },
            Action::PageUp => match self.mode {
                Mode::Toc => self.toc.move_selection(-(height as isize), height),
                Mode::Help => self.help_scroll = self.help_scroll.saturating_sub(height),
                _ => self.scroll_by(-(height as isize)),
            },
            Action::HalfPageDown => self.scroll_by((height / 2).max(1) as isize),
            Action::HalfPageUp => self.scroll_by(-((height / 2).max(1) as isize)),
            Action::ScrollLeft => self.scroll_h(-8),
            Action::ScrollRight => self.scroll_h(8),
            Action::Top => match self.mode {
                Mode::Toc => self.toc.select(0, height),
                Mode::Help => self.help_scroll = 0,
                _ => self.scroll_to(0),
            },
            Action::Bottom => match self.mode {
                Mode::Toc => self.toc.select(usize::MAX, height),
                _ => self.scroll_to(usize::MAX),
            },
            Action::Search => self.open_search(),
            Action::NextSearch => self.cycle_search(true),
            Action::PreviousSearch => self.cycle_search(false),
            Action::NextHeading => self.jump_heading(true),
            Action::PreviousHeading => self.jump_heading(false),
            Action::NextHeadingSameLevel => self.jump_heading_same_level(true),
            Action::PreviousHeadingSameLevel => self.jump_heading_same_level(false),
            Action::ToggleToc => self.toggle_toc(),
            Action::ToggleKeyHints => self.toggle_key_hints(),
            Action::Activate => self.activate(),
            Action::OpenLink => self.open_selected_link(),
            Action::NextLink => self.cycle_link(true),
            Action::PreviousLink => self.cycle_link(false),
            Action::ToggleFold => self.fold_current(FoldOp::Toggle),
            Action::CollapseFold => self.fold_current(FoldOp::Collapse),
            Action::ExpandFold => self.fold_current(FoldOp::Expand),
            Action::CollapseAll => {
                self.folds.collapse_all();
                self.after_fold_change();
                self.set_message("all sections collapsed");
            }
            Action::ExpandAll => {
                self.folds.expand_all();
                self.after_fold_change();
                self.set_message("all sections expanded");
            }
            Action::Help => {
                self.mode = if self.mode == Mode::Help {
                    Mode::Normal
                } else {
                    Mode::Help
                };
                self.help_scroll = 0;
            }
            Action::ToggleMermaidSource => self.toggle_mermaid_source(),
        }
        self.ensure_layout();
    }

    fn close_overlays(&mut self) {
        match self.mode {
            Mode::Help => {
                self.mode = Mode::Normal;
                self.help_scroll = 0;
            }
            Mode::Toc => {
                self.mode = Mode::Normal;
                self.toc.open = false;
                self.invalidate();
            }
            Mode::Search => {
                self.mode = Mode::Normal;
            }
            _ => {
                if self.search.has_matches() {
                    self.search.clear();
                    self.prepare_frame();
                } else {
                    self.selected_link = None;
                }
            }
        }
        self.clear_message();
    }

    // -- mouse ------------------------------------------------------------

    /// Handle a mouse event; ignored unless `mouse` is configured and the
    /// terminal supports it.
    pub(crate) fn handle_mouse(&mut self, event: &crossterm::event::MouseEvent) {
        use crossterm::event::MouseEventKind;
        if !self.config.mouse || !self.caps.mouse {
            return;
        }
        match event.kind {
            MouseEventKind::ScrollDown => self.scroll_by(3),
            MouseEventKind::ScrollUp => self.scroll_by(-3),
            MouseEventKind::ScrollLeft => self.scroll_h(-4),
            MouseEventKind::ScrollRight => self.scroll_h(4),
            MouseEventKind::Down(crossterm::event::MouseButton::Left) => {
                self.click(event.column, event.row)
            }
            _ => return,
        }
        self.ensure_layout();
    }

    fn click(&mut self, column: u16, row: u16) {
        let (sidebar, hints) = self.sidebar_widths();
        if usize::from(row) >= self.content_height() {
            return; // status/prompt row
        }
        if hints > 0 && column >= self.size.0.max(1).saturating_sub(hints) {
            // The hints sidebar is a column of buttons: the row the reader
            // clicked names its own action, so run it. Titles and the blank
            // separators are inert.
            if let Some(action) = self.hint_row_action(row) {
                self.apply(action);
            }
            return;
        }
        if sidebar > 0 && column < sidebar {
            let index = self.toc.scroll + usize::from(row);
            if index < self.toc.len() {
                let height = self.content_height();
                self.toc.select(index, height);
                self.mode = Mode::Toc;
                self.toc_jump();
            }
            return;
        }
        // With a width limit the document ends before the hints sidebar
        // does; clicks in between belong to no line.
        let column = usize::from(column).saturating_sub(usize::from(sidebar));
        if column >= self.content_width() {
            return;
        }
        let line_index = self.top_line + usize::from(row);
        let Some(line) = self.tree.lines.get(line_index) else {
            return;
        };
        let node = line.node;
        let kind = line.kind.clone();
        // Column inside the document, accounting for the sidebar and h-scroll.
        let target_col = self.h_offset + column;
        let mut col = 0usize;
        let mut clicked_link = None;
        for span in &line.spans {
            let width = span.width();
            if target_col < col + width {
                clicked_link = span.link;
                break;
            }
            col += width;
        }
        if let Some(id) = clicked_link {
            self.selected_link = Some(id);
            if let Some(link) = self.doc.links.get(id) {
                self.set_message(format!("link: {}", link.url));
            }
            return;
        }
        if matches!(kind, LineKind::Heading(_) | LineKind::FoldedMarker) {
            if let Some(section) = self.doc.section_of(node) {
                self.folds.toggle(section);
                if let Some(s) = self.doc.sections.get(section) {
                    self.anchor = (s.heading, 0);
                    self.cursor = s.heading;
                }
                self.after_section_fold(section);
            }
        }
    }

    /// The action a click on hints-sidebar row `row` triggers, if any.
    ///
    /// Recomputed from the current state rather than remembered from the last
    /// frame: the sidebar's content is a pure function of the state, so the
    /// two can never disagree.
    fn hint_row_action(&self, row: u16) -> Option<Action> {
        let (_, width) = self.sidebar_widths();
        if width == 0 {
            return None;
        }
        let groups = self.hint_groups();
        let inner = usize::from(width.saturating_sub(1)).max(1);
        let rows = KeyHintsSidebar {
            groups: &groups,
            theme: &self.theme,
            level: self.color,
            unicode: self.caps.unicode_box,
        }
        .rows(inner, self.content_height());
        match rows.get(usize::from(row)).map(|(_, kind)| *kind) {
            Some(HintLine::Row(action)) => Some(action),
            _ => None,
        }
    }

    // -- help -------------------------------------------------------------

    /// Help entries with the *actual* current bindings, so custom keybindings
    /// are reflected.
    pub(crate) fn help_entries(&self) -> Vec<(String, String)> {
        Action::ALL
            .iter()
            .map(|action| {
                let keys = self.keymap.bindings_for(*action);
                let keys = if keys.is_empty() {
                    "(unbound)".to_string()
                } else {
                    keys.join(", ")
                };
                (keys, action.description().to_string())
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::state::test_support::*;
    use crate::config::keys::KeyMap;
    use crate::config::schema::KeyBinding;
    use crossterm::event::KeyCode;
    use crossterm::event::KeyEvent;
    use crossterm::event::KeyModifiers;

    #[test]
    fn clicking_a_hint_row_runs_its_action() {
        let mut a = app_with(DOC, (120, 24));
        a.caps.mouse = true; // the default capabilities have no mouse
        a.apply(Action::ToggleKeyHints);
        let (_, hints) = a.sidebar_widths();
        assert!(hints > 0);
        let column = a.size().0 - hints + 2; // past the left border
        let groups = a.hint_groups();
        let rows = KeyHintsSidebar {
            groups: &groups,
            theme: &a.theme,
            level: a.color,
            unicode: a.caps.unicode_box,
        }
        .rows(usize::from(hints - 1), a.content_height());
        let toc_row = rows
            .iter()
            .position(|(_, k)| *k == HintLine::Row(Action::ToggleToc))
            .expect("the `contents` row is on screen");

        a.handle_mouse(&crossterm::event::MouseEvent {
            kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
            column,
            row: toc_row as u16,
            modifiers: crossterm::event::KeyModifiers::NONE,
        });
        assert!(a.toc.open, "clicking the `contents` row opened the TOC");

        // A group title is inert.
        a.handle_mouse(&crossterm::event::MouseEvent {
            kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
            column,
            row: 0,
            modifiers: crossterm::event::KeyModifiers::NONE,
        });
        assert_eq!(a.mode(), Mode::Toc, "the title row did nothing");
    }

    #[test]
    fn help_lists_custom_bindings() {
        let binding = KeyBinding::One("x".to_string());
        let keymap = KeyMap::from_overrides([("quit", &binding)]).expect("valid override");
        let mut a = crate::testing::AppBuilder::new(DOC).keymap(keymap).build();
        let entries = a.help_entries();
        let quit = entries
            .iter()
            .find(|(_, d)| d == Action::Quit.description())
            .expect("quit entry");
        assert_eq!(quit.0, "x", "help shows the override, not the default");
        a.apply(Action::Help);
        assert_eq!(a.mode(), Mode::Help);
        // `q` closes the overlay rather than quitting when it is bound away.
        key(&mut a, 'x');
        assert!(!a.should_quit(), "quit closes the overlay first");
        assert_eq!(a.mode(), Mode::Normal);
    }

    #[test]
    fn unknown_keys_are_ignored_and_prefixes_are_shown() {
        let mut a = app();
        let before = a.top_line();
        key(&mut a, 'Z');
        assert_eq!(a.top_line(), before);
        assert_eq!(a.pending(), "");
        key(&mut a, 'z');
        assert_eq!(a.pending(), "z", "pending prefix is displayed");
        key(&mut a, 'M');
        assert_eq!(a.pending(), "");
        assert!(a.tree().to_plain_text().contains('▶') || a.tree().len() < 20);
    }

    #[test]
    fn ctrl_c_quits_from_any_mode() {
        let mut a = app();
        a.apply(Action::Help);
        a.handle_key(&KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(a.should_quit());
    }

    #[test]
    fn mouse_wheel_scrolls_when_enabled() {
        use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
        let mut a = app();
        a.caps.mouse = true;
        let wheel = MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 1,
            row: 1,
            modifiers: KeyModifiers::NONE,
        };
        a.handle_mouse(&wheel);
        assert_eq!(a.top_line(), 3);
        a.config.mouse = false;
        a.handle_mouse(&wheel);
        assert_eq!(a.top_line(), 3, "disabled mouse is ignored");

        a.config.mouse = true;
        a.apply(Action::Top);
        let before = a.tree().len();
        let heading_row = a
            .tree()
            .heading_lines()
            .first()
            .map(|(line, _, _)| *line as u16)
            .expect("a heading");
        let click = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 1,
            row: heading_row,
            modifiers: KeyModifiers::NONE,
        };
        a.handle_mouse(&click);
        assert!(a.tree().len() < before, "clicking the H1 collapsed it");
    }

    #[test]
    fn clicks_past_a_width_limited_document_hit_nothing() {
        use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};

        let cfg = crate::config::Config {
            max_width: 60,
            mouse: true,
            ..crate::config::Config::default()
        };
        let mut a = crate::testing::AppBuilder::new(DOC)
            .size((100, 24))
            .config(cfg)
            .build();
        a.caps.mouse = true;
        assert_eq!(a.content_width(), 60);

        let heading_row = a
            .tree()
            .heading_lines()
            .first()
            .map(|(line, _, _)| *line as u16)
            .expect("a heading");
        let click = |column: u16| MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row: heading_row,
            modifiers: KeyModifiers::NONE,
        };

        // Past the limit there is no document, even though the terminal has
        // 40 more columns.
        let before = a.tree().len();
        a.handle_mouse(&click(60));
        assert_eq!(a.tree().len(), before, "the unused columns swallowed it");

        // Inside it the heading still folds.
        a.handle_mouse(&click(0));
        assert!(a.tree().len() < before, "clicking the heading collapsed it");
    }

    #[test]
    fn help_overlay_scrolls_and_closes() {
        let mut a = app();
        a.apply(Action::Help);
        assert_eq!(a.mode(), Mode::Help);
        a.apply(Action::ScrollDown);
        a.apply(Action::ScrollDown);
        assert_eq!(a.help_scroll(), 2);
        assert_eq!(a.top_line(), 0, "the document did not scroll");
        a.apply(Action::ScrollUp);
        assert_eq!(a.help_scroll(), 1);
        a.apply(Action::Cancel);
        assert_eq!(a.mode(), Mode::Normal);
        assert_eq!(a.help_scroll(), 0);
    }
}
