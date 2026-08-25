//! The `:` command line: parsing, completion and the line being typed.
//!
//! Everything here is a pure function of the typed text and the settings
//! table in [`crate::config::settings`] — no terminal, no application state.
//! [`crate::app::state::App`] owns the buffer and executes what
//! [`parse`] returns, which keeps the grammar testable on its own and stops
//! the command line from growing a second idea of what a setting is.

use crate::config::settings;

/// What a typed line asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Command {
    /// Nothing was typed.
    Empty,
    /// Show every setting with its values and default (`:help`).
    Help,
    /// Leave (`:q`, `:quit`).
    Quit,
    /// Report the current value of a key (`:center`).
    Show(String),
    /// Assign a value (`:center = false`, `:center false`).
    Set(String, String),
    /// Not a command at all.
    Unknown(String),
}

/// Parse a typed line.
///
/// The separator may be `=` or plain whitespace, because both read naturally:
/// `:center = true` mirrors the configuration file, `:theme crt` mirrors every
/// other pager. A leading `set` is accepted for the muscle memory it comes
/// from, and ignored.
pub(crate) fn parse(line: &str) -> Command {
    let line = line.trim();
    if line.is_empty() {
        return Command::Empty;
    }
    let line = line.strip_prefix("set ").map_or(line, str::trim);
    match line {
        "help" | "h" | "?" => return Command::Help,
        "q" | "quit" => return Command::Quit,
        _ => {}
    }

    let (key, value) = split(line);
    let key = key.to_string();
    match value {
        // A bare key is a question, not an assignment: `:center` says what
        // centring is currently doing rather than turning it on.
        None if settings::get(&key).is_some() => Command::Show(key),
        None => Command::Unknown(key),
        Some(value) => Command::Set(key, value.to_string()),
    }
}

/// Split a line into its key and, if there is one, its value.
fn split(line: &str) -> (&str, Option<&str>) {
    if let Some((key, value)) = line.split_once('=') {
        return (key.trim(), Some(value.trim()));
    }
    match line.split_once(char::is_whitespace) {
        Some((key, value)) => (key.trim(), Some(value.trim())),
        None => (line, None),
    }
}

/// The result of pressing Tab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Completion {
    /// The line after completing as far as it is unambiguous.
    pub(crate) line: String,
    /// What is still possible, when more than one thing is.
    pub(crate) candidates: Vec<String>,
}

/// Complete the last token of `line`.
///
/// Completion never guesses past the point where the answer stops being
/// unique: it extends the token by the longest prefix every candidate shares
/// and hands the rest back as candidates for the status line. A key that
/// completes to exactly one match gains its separator, so the next keystroke
/// is the value rather than a space.
pub(crate) fn complete(line: &str) -> Completion {
    let unchanged = |candidates: Vec<String>| Completion {
        line: line.to_string(),
        candidates,
    };
    let trimmed = line.trim_start();
    let indent = &line[..line.len() - trimmed.len()];

    // A value is being typed once a separator is there, even with nothing
    // after it: `:theme ` completes the theme names, not the key names.
    let (key, value) = split_for_completion(trimmed);
    let Some(partial) = value else {
        let names = settings::matching(key);
        if names.is_empty() {
            return unchanged(Vec::new());
        }
        let shared = common_prefix(&names);
        let line = if names.len() == 1 {
            format!("{indent}{shared} = ")
        } else {
            format!("{indent}{shared}")
        };
        return Completion {
            line,
            candidates: if names.len() == 1 {
                Vec::new()
            } else {
                names.iter().map(|n| (*n).to_string()).collect()
            },
        };
    };

    let Some(setting) = settings::get(key) else {
        return unchanged(Vec::new());
    };
    let values: Vec<&str> = setting
        .kind
        .values()
        .into_iter()
        .filter(|v| v.starts_with(partial))
        .collect();
    if values.is_empty() {
        // Free text and numbers have nothing to offer, so say what the key
        // takes instead of completing to nothing.
        return unchanged(vec![setting.kind.describe()]);
    }
    let shared = common_prefix(&values);
    Completion {
        line: format!("{indent}{key} = {shared}"),
        candidates: if values.len() == 1 {
            Vec::new()
        } else {
            values.iter().map(|v| (*v).to_string()).collect()
        },
    }
}

/// Like [`split`], but a trailing separator counts as an empty value so that
/// completion switches from keys to values as soon as one is typed.
fn split_for_completion(line: &str) -> (&str, Option<&str>) {
    let line = line.strip_prefix("set ").map_or(line, str::trim_start);
    if let Some((key, value)) = line.split_once('=') {
        return (key.trim(), Some(value.trim_start()));
    }
    match line.split_once(char::is_whitespace) {
        Some((key, value)) => (key.trim(), Some(value.trim_start())),
        None => (line, None),
    }
}

/// The longest prefix every candidate shares.
fn common_prefix(items: &[&str]) -> String {
    let Some(first) = items.first() else {
        return String::new();
    };
    let mut end = first.len();
    for item in &items[1..] {
        end = end.min(
            first
                .char_indices()
                .zip(item.char_indices())
                .take_while(|((_, a), (_, b))| a == b)
                .map(|((i, c), _)| i + c.len_utf8())
                .last()
                .unwrap_or(0),
        );
    }
    first[..end].to_string()
}

/// The `:` line as it is being typed.
#[derive(Debug, Clone, Default)]
pub(crate) struct CommandState {
    /// The text after the `:`.
    pub(crate) line: String,
    /// Completion candidates from the last Tab, shown until the next key.
    pub(crate) candidates: Vec<String>,
}

impl CommandState {
    /// Start a new, empty line.
    pub(crate) fn open(&mut self) {
        self.line.clear();
        self.candidates.clear();
    }

    /// The prompt as drawn, including the leading `:`.
    pub(crate) fn prompt(&self) -> String {
        format!(":{}", self.line)
    }

    /// Type one character.
    pub(crate) fn push(&mut self, c: char) {
        self.line.push(c);
        self.candidates.clear();
    }

    /// Delete the character before the cursor. Returns `false` when the line
    /// was already empty, which closes the prompt the way it opened.
    pub(crate) fn backspace(&mut self) -> bool {
        self.candidates.clear();
        self.line.pop().is_some()
    }

    /// Complete the line in place, keeping any remaining candidates.
    pub(crate) fn complete(&mut self) {
        let done = complete(&self.line);
        self.line = done.line;
        self.candidates = done.candidates;
    }

    /// The candidates as one status line, cut to a sensible length.
    pub(crate) fn candidate_hint(&self) -> Option<String> {
        if self.candidates.is_empty() {
            return None;
        }
        const SHOWN: usize = 6;
        let mut hint = self
            .candidates
            .iter()
            .take(SHOWN)
            .cloned()
            .collect::<Vec<_>>()
            .join("  ");
        if self.candidates.len() > SHOWN {
            hint.push_str(&format!("  … +{}", self.candidates.len() - SHOWN));
        }
        Some(hint)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_separators_assign_and_a_bare_key_asks() {
        assert_eq!(
            parse("center = false"),
            Command::Set("center".into(), "false".into())
        );
        assert_eq!(
            parse("center false"),
            Command::Set("center".into(), "false".into())
        );
        assert_eq!(
            parse("set max_width=100"),
            Command::Set("max_width".into(), "100".into())
        );
        assert_eq!(parse("  center  "), Command::Show("center".into()));
        assert_eq!(parse("nonsense"), Command::Unknown("nonsense".into()));
        assert_eq!(parse("   "), Command::Empty);
        assert_eq!(parse("help"), Command::Help);
        assert_eq!(parse("?"), Command::Help);
        assert_eq!(parse("q"), Command::Quit);
    }

    /// A value may contain spaces and an `=`: an opener is a command line,
    /// not a word.
    #[test]
    fn a_value_keeps_what_it_contains() {
        assert_eq!(
            parse("links.opener = my-open --arg=1"),
            Command::Set("links.opener".into(), "my-open --arg=1".into())
        );
    }

    #[test]
    fn keys_complete_to_the_shared_prefix_and_then_to_the_separator() {
        // Several matches: only the shared prefix, and the rest is offered.
        let c = complete("c");
        assert_eq!(c.line, "c");
        assert!(c.candidates.contains(&"center".to_string()));
        assert!(c.candidates.contains(&"color".to_string()));

        let c = complete("ce");
        assert_eq!(c.line, "center = ", "unique keys gain the separator");
        assert!(c.candidates.is_empty());

        let c = complete("code.");
        assert_eq!(c.line, "code.", "the shared prefix is already typed");
        assert_eq!(c.candidates.len(), 3);

        assert_eq!(complete("zzz").line, "zzz", "nothing to complete");
    }

    #[test]
    fn values_complete_once_a_separator_is_there() {
        let c = complete("theme ");
        assert_eq!(c.line, "theme = ");
        assert_eq!(
            c.candidates,
            vec!["auto", "dark", "light", "crt", "cyberpunk"]
        );

        // Two themes start with `c`, so completion stops at what they share.
        let c = complete("theme = c");
        assert_eq!(c.line, "theme = c");
        assert_eq!(c.candidates, vec!["crt", "cyberpunk"]);
        let c = complete("theme = cy");
        assert_eq!(c.line, "theme = cyberpunk");
        assert!(c.candidates.is_empty());

        let c = complete("theme = d");
        assert_eq!(c.line, "theme = dark");
        assert!(c.candidates.is_empty());

        let c = complete("center = ");
        assert_eq!(c.line, "center = ");
        assert_eq!(c.candidates, vec!["true", "false"]);

        let c = complete("center = t");
        assert_eq!(c.line, "center = true");

        // A key that takes free text says so rather than completing.
        let c = complete("links.opener = ");
        assert_eq!(c.line, "links.opener = ");
        assert_eq!(c.candidates, vec!["<text>"]);
    }

    #[test]
    fn the_state_tracks_typing_and_drops_stale_candidates() {
        let mut state = CommandState::default();
        state.open();
        for c in "ce".chars() {
            state.push(c);
        }
        state.complete();
        assert_eq!(state.prompt(), ":center = ");
        state.complete();
        assert_eq!(state.candidates, vec!["true", "false"]);
        state.push('t');
        assert!(
            state.candidates.is_empty(),
            "typing invalidates the last Tab"
        );
        state.complete();
        assert_eq!(state.line, "center = true");

        while state.backspace() {}
        assert!(!state.backspace(), "an empty line reports it");
    }

    #[test]
    fn the_candidate_hint_is_bounded() {
        let mut state = CommandState::default();
        state.complete();
        let hint = state.candidate_hint().expect("every key matches");
        assert!(hint.contains("theme"));
        assert!(hint.contains("… +"), "a long list is summarised: {hint}");
    }
}
