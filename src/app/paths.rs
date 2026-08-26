//! Filesystem paths for the `:open` command: completion and resolution.
//!
//! The rest of [`crate::app::command`] is a pure function of the typed text;
//! completing a path is not, because only the filesystem knows what is there.
//! That one impurity is kept here, behind two small functions, so the command
//! grammar stays testable without a directory to look at.
//!
//! # Where a relative path is relative to
//!
//! To the working directory diple was started in, first — that is where the
//! reader typed the path from, and it is what Tab completes against. A path
//! that does not exist there is tried again next to the document that is
//! currently open, so `:open tab notes.md` also works while reading
//! `docs/index.md` in a repository checked out elsewhere.

use std::path::{Path, PathBuf};

/// Expand a leading `~` from `$HOME`.
///
/// Only the leading `~` — `~user` is left alone, because resolving another
/// user's home needs the password database and no pager needs that.
fn expand_tilde(path: &str) -> PathBuf {
    let Some(rest) = path.strip_prefix('~') else {
        return PathBuf::from(path);
    };
    if !(rest.is_empty() || rest.starts_with('/')) {
        return PathBuf::from(path);
    }
    let Some(home) = std::env::var_os("HOME") else {
        return PathBuf::from(path);
    };
    let mut expanded = PathBuf::from(home);
    if let Some(rest) = rest.strip_prefix('/') {
        expanded.push(rest);
    }
    expanded
}

/// Turn a typed path into one to read, trying the working directory before
/// the directory of the document that is currently open.
///
/// `near` is that document's path (the file name shown in the status bar);
/// `None` for a document that came from stdin.
pub(crate) fn resolve(typed: &str, near: Option<&str>) -> PathBuf {
    let path = expand_tilde(typed);
    if path.is_absolute() || path.exists() {
        return path;
    }
    let beside = near
        .map(Path::new)
        .and_then(Path::parent)
        .map(|dir| dir.join(&path));
    match beside {
        Some(candidate) if candidate.exists() => candidate,
        _ => path,
    }
}

/// Complete a partially typed path.
///
/// Returns the token as far as it can be completed unambiguously, plus what
/// is still possible when more than one thing is. Directories complete with a
/// trailing `/`, so the next Tab descends into them.
///
/// Hidden entries only appear once the reader typed the leading dot, which is
/// the shell convention and keeps `~/` from listing a home directory's worth
/// of dotfiles.
pub(crate) fn complete(partial: &str) -> (String, Vec<String>) {
    let (prefix, stem) = match partial.rfind('/') {
        Some(cut) => (&partial[..=cut], &partial[cut + 1..]),
        None => ("", partial),
    };
    let dir = if prefix.is_empty() {
        PathBuf::from(".")
    } else {
        expand_tilde(prefix)
    };
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return (partial.to_string(), Vec::new());
    };

    let mut names: Vec<String> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !name.starts_with(stem) {
                return None;
            }
            if name.starts_with('.') && !stem.starts_with('.') {
                return None;
            }
            let is_dir = entry.file_type().is_ok_and(|t| t.is_dir());
            Some(if is_dir { format!("{name}/") } else { name })
        })
        .collect();
    names.sort_unstable();
    if names.is_empty() {
        return (partial.to_string(), Vec::new());
    }
    let shared = common_prefix(&names);
    let line = format!("{prefix}{shared}");
    let candidates = if names.len() == 1 { Vec::new() } else { names };
    (line, candidates)
}

/// The longest prefix every candidate shares.
fn common_prefix(items: &[String]) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// A temporary directory that removes itself, so the completion tests can
    /// look at a filesystem they built themselves.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> TempDir {
            let dir =
                std::env::temp_dir().join(format!("diple-paths-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).expect("temp dir");
            TempDir(dir)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn a_directory_completes_with_a_slash_and_files_share_their_prefix() {
        let tmp = TempDir::new("complete");
        fs::write(tmp.path().join("alpha.md"), "# a").expect("write");
        fs::write(tmp.path().join("album.md"), "# b").expect("write");
        fs::create_dir(tmp.path().join("nested")).expect("mkdir");
        fs::write(tmp.path().join(".hidden.md"), "# h").expect("write");

        let base = tmp.path().display().to_string();
        let (line, candidates) = complete(&format!("{base}/al"));
        assert_eq!(line, format!("{base}/al"), "two files share only `al`");
        assert_eq!(candidates, vec!["album.md", "alpha.md"]);

        let (line, candidates) = complete(&format!("{base}/alp"));
        assert_eq!(line, format!("{base}/alpha.md"));
        assert!(candidates.is_empty(), "a unique match needs no list");

        let (line, _) = complete(&format!("{base}/n"));
        assert_eq!(line, format!("{base}/nested/"), "directories gain a slash");

        let (_, candidates) = complete(&format!("{base}/"));
        assert!(
            !candidates.iter().any(|c| c.starts_with('.')),
            "hidden entries stay hidden until the dot is typed: {candidates:?}"
        );
        let (line, _) = complete(&format!("{base}/.h"));
        assert_eq!(line, format!("{base}/.hidden.md"));
    }

    #[test]
    fn an_unreadable_directory_completes_to_nothing() {
        let (line, candidates) = complete("/no/such/directory/at/all/x");
        assert_eq!(line, "/no/such/directory/at/all/x");
        assert!(candidates.is_empty());
    }

    #[test]
    fn a_relative_path_falls_back_to_the_directory_of_the_open_document() {
        let tmp = TempDir::new("resolve");
        let docs = tmp.path().join("docs");
        fs::create_dir(&docs).expect("mkdir");
        fs::write(docs.join("other.md"), "# o").expect("write");

        let near = docs.join("index.md").display().to_string();
        assert_eq!(
            resolve("other.md", Some(&near)),
            docs.join("other.md"),
            "a name that is not in the working directory is tried beside the document"
        );
        // Nothing anywhere: the typed path is handed back, so the error
        // message names what the reader typed.
        assert_eq!(
            resolve("nowhere.md", Some(&near)),
            PathBuf::from("nowhere.md")
        );
        assert_eq!(resolve("/tmp/x.md", None), PathBuf::from("/tmp/x.md"));
    }

    #[test]
    fn a_leading_tilde_expands_and_anything_else_does_not() {
        // Read `$HOME` rather than setting it: the suite runs in threads, and
        // one test changing the process environment is one test deciding what
        // every other test sees.
        let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
            return;
        };
        assert_eq!(expand_tilde("~/a.md"), home.join("a.md"));
        assert_eq!(expand_tilde("~"), home);
        assert_eq!(expand_tilde("~other/a.md"), PathBuf::from("~other/a.md"));
        assert_eq!(expand_tilde("./a.md"), PathBuf::from("./a.md"));
    }
}
