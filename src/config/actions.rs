//! Every bindable action.
//!
//! Action names used in the `[keys]` config section are the snake_case form
//! of the variant name, e.g. `NextHeading` → `next_heading`.

/// A user-triggerable action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Action {
    /// Quit diple (`q`, `Ctrl-C`).
    Quit,
    /// Cancel the current sub-mode (`Esc`).
    Cancel,
    /// Scroll one line down (`j`, `↓`).
    ScrollDown,
    /// Scroll one line up (`k`, `↑`).
    ScrollUp,
    /// Page down (`PgDn`, `Space`).
    PageDown,
    /// Page up (`PgUp`, `b`).
    PageUp,
    /// Half page down (`Ctrl-D`).
    HalfPageDown,
    /// Half page up (`Ctrl-U`).
    HalfPageUp,
    /// Horizontal scroll left (`h`, `←`).
    ScrollLeft,
    /// Horizontal scroll right (`l`, `→`).
    ScrollRight,
    /// Jump to the top (`g`).
    Top,
    /// Jump to the bottom (`G`).
    Bottom,
    /// Open the search prompt (`/`).
    Search,
    /// Next search result (`n`).
    NextSearch,
    /// Previous search result (`N`).
    PreviousSearch,
    /// Next heading (`]`).
    NextHeading,
    /// Previous heading (`[`).
    PreviousHeading,
    /// Next heading at same or higher level (`}`).
    NextHeadingSameLevel,
    /// Previous heading at same or higher level (`{`).
    PreviousHeadingSameLevel,
    /// Toggle the table of contents (`t`).
    ToggleToc,
    /// Toggle the key hints sidebar (`K`).
    ToggleKeyHints,
    /// Activate selected heading/link (`Enter`).
    Activate,
    /// Open the selected link (`o`).
    OpenLink,
    /// Select next link (`Tab`).
    NextLink,
    /// Select previous link (`Shift-Tab`).
    PreviousLink,
    /// Toggle current section fold (`za`).
    ToggleFold,
    /// Collapse current section (`zc`).
    CollapseFold,
    /// Expand current section (`zo`).
    ExpandFold,
    /// Collapse all sections (`zM`).
    CollapseAll,
    /// Expand all sections (`zR`).
    ExpandAll,
    /// Show the help overlay (`?`).
    Help,
    /// Toggle Mermaid source view on a diagram (`s`).
    ToggleMermaidSource,
}

impl Action {
    /// All actions, in help-display order.
    pub const ALL: &'static [Action] = &[
        Action::Quit,
        Action::Cancel,
        Action::ScrollDown,
        Action::ScrollUp,
        Action::PageDown,
        Action::PageUp,
        Action::HalfPageDown,
        Action::HalfPageUp,
        Action::ScrollLeft,
        Action::ScrollRight,
        Action::Top,
        Action::Bottom,
        Action::Search,
        Action::NextSearch,
        Action::PreviousSearch,
        Action::NextHeading,
        Action::PreviousHeading,
        Action::NextHeadingSameLevel,
        Action::PreviousHeadingSameLevel,
        Action::ToggleToc,
        Action::ToggleKeyHints,
        Action::Activate,
        Action::OpenLink,
        Action::NextLink,
        Action::PreviousLink,
        Action::ToggleFold,
        Action::CollapseFold,
        Action::ExpandFold,
        Action::CollapseAll,
        Action::ExpandAll,
        Action::Help,
        Action::ToggleMermaidSource,
    ];

    /// snake_case name used in configuration.
    pub fn name(self) -> &'static str {
        match self {
            Action::Quit => "quit",
            Action::Cancel => "cancel",
            Action::ScrollDown => "scroll_down",
            Action::ScrollUp => "scroll_up",
            Action::PageDown => "page_down",
            Action::PageUp => "page_up",
            Action::HalfPageDown => "half_page_down",
            Action::HalfPageUp => "half_page_up",
            Action::ScrollLeft => "scroll_left",
            Action::ScrollRight => "scroll_right",
            Action::Top => "top",
            Action::Bottom => "bottom",
            Action::Search => "search",
            Action::NextSearch => "next_search",
            Action::PreviousSearch => "previous_search",
            Action::NextHeading => "next_heading",
            Action::PreviousHeading => "previous_heading",
            Action::NextHeadingSameLevel => "next_heading_same_level",
            Action::PreviousHeadingSameLevel => "previous_heading_same_level",
            Action::ToggleToc => "toggle_toc",
            Action::ToggleKeyHints => "toggle_key_hints",
            Action::Activate => "activate",
            Action::OpenLink => "open_link",
            Action::NextLink => "next_link",
            Action::PreviousLink => "previous_link",
            Action::ToggleFold => "toggle_fold",
            Action::CollapseFold => "collapse_fold",
            Action::ExpandFold => "expand_fold",
            Action::CollapseAll => "collapse_all",
            Action::ExpandAll => "expand_all",
            Action::Help => "help",
            Action::ToggleMermaidSource => "toggle_mermaid_source",
        }
    }

    /// Look up an action by its snake_case configuration name.
    pub fn from_name(name: &str) -> Option<Action> {
        Action::ALL.iter().copied().find(|a| a.name() == name)
    }

    /// Human-readable description for the help overlay.
    pub fn description(self) -> &'static str {
        match self {
            Action::Quit => "quit",
            Action::Cancel => "cancel / close",
            Action::ScrollDown => "scroll down",
            Action::ScrollUp => "scroll up",
            Action::PageDown => "page down",
            Action::PageUp => "page up",
            Action::HalfPageDown => "half page down",
            Action::HalfPageUp => "half page up",
            Action::ScrollLeft => "scroll left",
            Action::ScrollRight => "scroll right",
            Action::Top => "top of document",
            Action::Bottom => "bottom of document",
            Action::Search => "search",
            Action::NextSearch => "next search result",
            Action::PreviousSearch => "previous search result",
            Action::NextHeading => "next heading",
            Action::PreviousHeading => "previous heading",
            Action::NextHeadingSameLevel => "next heading (same or higher level)",
            Action::PreviousHeadingSameLevel => "previous heading (same or higher level)",
            Action::ToggleToc => "toggle table of contents",
            Action::ToggleKeyHints => "toggle key hints sidebar",
            Action::Activate => "activate heading/link",
            Action::OpenLink => "open selected link",
            Action::NextLink => "select next link",
            Action::PreviousLink => "select previous link",
            Action::ToggleFold => "toggle section",
            Action::CollapseFold => "collapse section",
            Action::ExpandFold => "expand section",
            Action::CollapseAll => "collapse all sections",
            Action::ExpandAll => "expand all sections",
            Action::Help => "help",
            Action::ToggleMermaidSource => "toggle mermaid source",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip() {
        for &a in Action::ALL {
            assert_eq!(Action::from_name(a.name()), Some(a), "{a:?}");
        }
        assert_eq!(Action::from_name("nope"), None);
        assert_eq!(Action::from_name("next_heading"), Some(Action::NextHeading));
    }

    #[test]
    fn names_are_unique() {
        let mut names: Vec<_> = Action::ALL.iter().map(|a| a.name()).collect();
        names.sort_unstable();
        let before = names.len();
        names.dedup();
        assert_eq!(before, names.len());
    }
}
