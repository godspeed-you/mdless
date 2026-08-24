//! External Mermaid CLI (`mmdc`) integration.
//!
//! * The binary is located with an in-tree `which`-style `PATH` search.
//! * The diagram source is written to a private temp directory and rendered
//!   with `mmdc -i <in.mmd> -o <out.png> -b transparent -w <pixels>`.
//! * The child process is polled against a hard deadline and killed on timeout.
//! * Results are cached in memory and on disk under
//!   `<cache dir>/diple/mermaid/<sha256>.png` so that a resize-triggered
//!   re-layout never re-invokes `mmdc`.
//!
//! Every failure mode is returned as [`MmdcError`]; the selector turns it into
//! source rendering plus a non-fatal warning. Nothing here panics or aborts.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use super::hash::sha256_hex;
use super::image::ImageData;

/// Default hard timeout for one `mmdc` invocation.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);

/// A failure while rendering through `mmdc`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MmdcError {
    /// The configured command was not found on `PATH`.
    #[error("`{0}` was not found on PATH")]
    NotFound(String),
    /// The process could not be started.
    #[error("cannot start `{command}`: {message}")]
    Spawn {
        /// Command that failed to start.
        command: String,
        /// OS error text.
        message: String,
    },
    /// The process exceeded the deadline and was killed.
    #[error("`{command}` timed out after {seconds}s")]
    Timeout {
        /// Command that timed out.
        command: String,
        /// Timeout in seconds.
        seconds: u64,
    },
    /// The process exited with a non-zero status.
    #[error("`{command}` exited with {status}: {stderr}")]
    Exit {
        /// Command that failed.
        command: String,
        /// Exit status description.
        status: String,
        /// Captured stderr (trimmed).
        stderr: String,
    },
    /// The output file was missing or unreadable.
    #[error("cannot read the diagram rendered by `{command}`: {message}")]
    Output {
        /// Command that produced the output.
        command: String,
        /// I/O or decode error text.
        message: String,
    },
}

/// Searches `PATH` for `command`, mirroring `which(1)`.
///
/// A `command` containing a path separator is checked directly. Returns the
/// first existing, executable candidate.
#[must_use]
pub fn find_executable(command: &str) -> Option<PathBuf> {
    if command.is_empty() {
        return None;
    }
    if command.contains(std::path::MAIN_SEPARATOR) || command.contains('/') {
        let p = PathBuf::from(command);
        return is_executable(&p).then_some(p);
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(command))
        .find(|candidate| is_executable(candidate))
}

fn is_executable(path: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    if !meta.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

/// Returns `<cache dir>/diple/mermaid`, e.g. `~/.cache/diple/mermaid`.
#[must_use]
pub fn cache_dir() -> Option<PathBuf> {
    directories::BaseDirs::new().map(|d| d.cache_dir().join("diple").join("mermaid"))
}

/// Cache key for a diagram: SHA-256 over the requested pixel width, the source
/// byte length and the source itself.
///
/// The key is stable across processes and platforms, which is what makes the
/// on-disk cache reusable between runs.
#[must_use]
pub fn cache_key(source: &str, width_px: u32) -> String {
    let material = format!("diple-mermaid-v1\n{width_px}\n{}\n{source}", source.len());
    sha256_hex(material.as_bytes())
}

/// Renders Mermaid sources through the `mmdc` CLI, with caching.
#[derive(Debug)]
pub struct MmdcRunner {
    command: String,
    timeout: Duration,
    cache_dir: Option<PathBuf>,
    memory: Mutex<HashMap<String, ImageData>>,
}

impl MmdcRunner {
    /// Creates a runner for `command` with [`DEFAULT_TIMEOUT`] and the default
    /// on-disk cache location.
    #[must_use]
    pub fn new(command: impl Into<String>) -> Self {
        Self {
            command: command.into(),
            timeout: DEFAULT_TIMEOUT,
            cache_dir: cache_dir(),
            memory: Mutex::new(HashMap::new()),
        }
    }

    /// Overrides the hard timeout.
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Overrides (or disables, with `None`) the on-disk cache directory.
    #[must_use]
    pub fn with_cache_dir(mut self, dir: Option<PathBuf>) -> Self {
        self.cache_dir = dir;
        self
    }

    /// The configured command name.
    #[must_use]
    pub fn command(&self) -> &str {
        &self.command
    }

    /// `true` when the configured command exists on `PATH`.
    #[must_use]
    pub fn is_available(&self) -> bool {
        find_executable(&self.command).is_some()
    }

    /// Renders `source` to a PNG of roughly `width_px` pixels width.
    ///
    /// # Errors
    ///
    /// See [`MmdcError`]. All failures are recoverable: the caller falls back to
    /// source rendering with a non-fatal warning.
    pub fn render(&self, source: &str, width_px: u32) -> Result<ImageData, MmdcError> {
        let key = cache_key(source, width_px);

        if let Ok(mem) = self.memory.lock() {
            if let Some(hit) = mem.get(&key) {
                return Ok(hit.clone());
            }
        }
        if let Some(png) = self.disk_hit(&key) {
            if let Ok(data) = ImageData::from_png(png) {
                self.remember(key, &data);
                return Ok(data);
            }
        }

        let bin = find_executable(&self.command)
            .ok_or_else(|| MmdcError::NotFound(self.command.clone()))?;
        let png = self.invoke(&bin, source, width_px, &key)?;
        let data = ImageData::from_png(png.clone()).map_err(|e| MmdcError::Output {
            command: self.command.clone(),
            message: e.to_string(),
        })?;
        self.store_disk(&key, &png);
        self.remember(key, &data);
        Ok(data)
    }

    fn remember(&self, key: String, data: &ImageData) {
        if let Ok(mut mem) = self.memory.lock() {
            mem.insert(key, data.clone());
        }
    }

    fn disk_hit(&self, key: &str) -> Option<Vec<u8>> {
        let dir = self.cache_dir.as_ref()?;
        std::fs::read(dir.join(format!("{key}.png"))).ok()
    }

    fn store_disk(&self, key: &str, png: &[u8]) {
        let Some(dir) = self.cache_dir.as_ref() else {
            return;
        };
        if std::fs::create_dir_all(dir).is_err() {
            return;
        }
        // Write-then-rename so a concurrent reader never sees a partial file.
        let tmp = dir.join(format!("{key}.png.tmp{}", std::process::id()));
        if std::fs::write(&tmp, png).is_ok() {
            let _ = std::fs::rename(&tmp, dir.join(format!("{key}.png")));
        }
        let _ = std::fs::remove_file(&tmp);
    }

    fn invoke(
        &self,
        bin: &Path,
        source: &str,
        width_px: u32,
        key: &str,
    ) -> Result<Vec<u8>, MmdcError> {
        let work = TempDir::create(key)?;
        let input = work.path.join("diagram.mmd");
        let output = work.path.join("diagram.png");
        std::fs::write(&input, source).map_err(|e| MmdcError::Spawn {
            command: self.command.clone(),
            message: format!("cannot write {}: {e}", input.display()),
        })?;

        let mut child = Command::new(bin)
            .arg("-i")
            .arg(&input)
            .arg("-o")
            .arg(&output)
            .arg("-b")
            .arg("transparent")
            .arg("-w")
            .arg(width_px.max(1).to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| MmdcError::Spawn {
                command: self.command.clone(),
                message: e.to_string(),
            })?;

        // Drain stderr on a helper thread so the pipe can never fill up and
        // deadlock the poll loop below.
        let stderr_handle = child.stderr.take().map(|mut pipe| {
            std::thread::spawn(move || {
                use std::io::Read;
                let mut buf = String::new();
                let _ = pipe.read_to_string(&mut buf);
                buf
            })
        });

        let deadline = Instant::now() + self.timeout;
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) => {
                    if Instant::now() >= deadline {
                        let _ = child.kill();
                        let _ = child.wait();
                        // Do NOT join the stderr reader: a grandchild may still
                        // hold the pipe open, which would block us for as long
                        // as the runaway process lives. Detach it instead.
                        drop(stderr_handle);
                        return Err(MmdcError::Timeout {
                            command: self.command.clone(),
                            seconds: self.timeout.as_secs().max(1),
                        });
                    }
                    std::thread::sleep(Duration::from_millis(20));
                }
                Err(e) => {
                    return Err(MmdcError::Spawn {
                        command: self.command.clone(),
                        message: e.to_string(),
                    })
                }
            }
        };

        let stderr = stderr_handle
            .and_then(|h| h.join().ok())
            .unwrap_or_default();

        if !status.success() {
            return Err(MmdcError::Exit {
                command: self.command.clone(),
                status: status
                    .code()
                    .map_or_else(|| "signal".to_string(), |c| c.to_string()),
                stderr: trim_stderr(&stderr),
            });
        }

        std::fs::read(&output).map_err(|e| MmdcError::Output {
            command: self.command.clone(),
            message: e.to_string(),
        })
    }
}

fn trim_stderr(s: &str) -> String {
    let text = s.trim();
    const MAX: usize = 400;
    if text.chars().count() <= MAX {
        return text.to_string();
    }
    text.chars().take(MAX).collect::<String>() + "…"
}

/// A private temp directory removed on drop.
///
/// `tempfile` is a dev-dependency only, so the directory is created by hand
/// under [`std::env::temp_dir`] with a name derived from the cache key.
///
/// The name also carries the pid and a process-wide counter: the cache key
/// alone is shared by every caller rendering the same source at the same
/// width, so two concurrent threads would otherwise pick the same scratch
/// directory and delete each other's files on drop. The *cache* stays keyed
/// by content only — this uniqueness applies to the scratch space alone.
struct TempDir {
    path: PathBuf,
}

/// Distinguishes concurrent scratch directories within one process.
static TEMP_DIR_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

impl TempDir {
    fn create(key: &str) -> Result<Self, MmdcError> {
        let short: String = key.chars().take(16).collect();
        let seq = TEMP_DIR_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "diple-mermaid-{}-{seq}-{short}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).map_err(|e| MmdcError::Spawn {
            command: "mmdc".to_string(),
            message: format!("cannot create {}: {e}", path.display()),
        })?;
        Ok(Self { path })
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_key_is_stable_and_source_sensitive() {
        let a = cache_key("graph LR\nA-->B\n", 800);
        assert_eq!(a, cache_key("graph LR\nA-->B\n", 800));
        assert_eq!(a.len(), 64);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, cache_key("graph LR\nA-->C\n", 800));
        assert_ne!(a, cache_key("graph LR\nA-->B\n", 1600));
        assert_ne!(a, cache_key("graph LR\nA-->B", 800));
    }

    #[test]
    fn find_executable_handles_missing_and_present() {
        assert!(find_executable("definitely-not-a-real-binary-xyz").is_none());
        assert!(find_executable("").is_none());
        assert!(find_executable("/nonexistent/path/to/mmdc").is_none());
        // `sh` exists on every supported platform's PATH. A test environment
        // without it is broken, not "unsupported", so fail loudly.
        assert!(
            std::env::var_os("PATH").is_some(),
            "PATH must be set for the test environment"
        );
        assert!(
            find_executable("sh").is_some(),
            "`sh` must be on PATH for the test environment"
        );
    }

    #[test]
    fn missing_binary_is_a_recoverable_error() {
        let runner = MmdcRunner::new("definitely-not-a-real-binary-xyz").with_cache_dir(None);
        assert!(!runner.is_available());
        let err = runner.render("graph LR\nA-->B\n", 800).unwrap_err();
        assert!(matches!(err, MmdcError::NotFound(_)), "{err:?}");
    }

    #[test]
    fn non_zero_exit_is_reported() {
        let runner = MmdcRunner::new("false").with_cache_dir(None);
        assert!(
            runner.is_available(),
            "`false` must be on PATH for the test environment"
        );
        let err = runner.render("graph LR\nA-->B\n", 800).unwrap_err();
        assert!(matches!(err, MmdcError::Exit { .. }), "{err:?}");
    }

    #[test]
    fn timeout_kills_the_child() {
        // A stand-in "mmdc" that ignores its arguments and never terminates.
        let dir = std::env::temp_dir().join(format!("diple-mermaid-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("a writable temp dir is required");
        let script = dir.join("slow-mmdc");
        std::fs::write(&script, "#!/bin/sh\nsleep 30\n").expect("write the stand-in mmdc script");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755));
        }
        // Wait until the freshly written script can actually be exec'd.
        // Writing it opened a descriptor for writing, and any test thread that
        // forks a child in that instant hands the child an inherited copy of
        // it; until that child reaches its own `exec` and the descriptor is
        // closed, the kernel answers an `exec` of the script with ETXTBSY
        // ("Text file busy"). Nothing about the runner is wrong then, so wait
        // the window out here instead of failing the test for it. Once no
        // process holds a write descriptor the file stays exec'able — it is
        // never written again.
        for _ in 0..100 {
            match std::process::Command::new(&script)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
            {
                Ok(mut probe) => {
                    let _ = probe.kill();
                    let _ = probe.wait();
                    break;
                }
                // `libc::ETXTBSY`, without taking a dependency on libc for it.
                Err(e) if e.raw_os_error() == Some(26) => {
                    std::thread::sleep(Duration::from_millis(20));
                }
                Err(e) => panic!("the stand-in mmdc script cannot be started: {e}"),
            }
        }

        let runner = MmdcRunner::new(script.to_string_lossy().to_string())
            .with_timeout(Duration::from_millis(200))
            .with_cache_dir(None);
        assert!(
            runner.is_available(),
            "the stand-in script must be executable"
        );

        let started = Instant::now();
        let result = runner.render("graph LR\nA-->B\n", 800);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(
            matches!(result, Err(MmdcError::Timeout { .. })),
            "{result:?}"
        );
        assert!(started.elapsed() < Duration::from_secs(10));
    }

    #[test]
    fn cache_dir_is_under_the_user_cache() {
        let dir = cache_dir().expect("a home directory is required to locate the cache");
        assert!(dir.ends_with("diple/mermaid"), "{}", dir.display());
    }

    /// Exercises the real `mmdc` binary end to end.
    ///
    /// Ignored by default. Run it with:
    ///
    /// ```text
    /// DIPLE_TEST_MMDC=1 cargo test --lib mermaid::mmdc::tests::real_mmdc_round_trip -- --ignored
    /// ```
    ///
    /// With the opt-in set, an `mmdc` that is missing or broken (for example a
    /// headless Chromium that cannot start) fails the test — it is never
    /// skipped.
    #[test]
    #[ignore = "requires a working `mmdc`; set DIPLE_TEST_MMDC=1 and pass --ignored"]
    fn real_mmdc_round_trip() {
        assert_eq!(
            std::env::var("DIPLE_TEST_MMDC").ok().as_deref(),
            Some("1"),
            "set DIPLE_TEST_MMDC=1 to run this test"
        );
        // A private cache directory, so the "served from cache" assertion below
        // cannot be satisfied by a PNG left behind by an earlier run.
        let cache = std::env::temp_dir().join(format!(
            "diple-mermaid-roundtrip-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&cache);
        let runner = MmdcRunner::new("mmdc")
            .with_timeout(Duration::from_secs(60))
            .with_cache_dir(Some(cache.clone()));
        assert!(runner.is_available(), "`mmdc` is not installed");

        let result = runner.render("graph LR\n  A --> B\n", 600);
        let img = match result {
            Ok(img) => img,
            Err(e) => {
                let _ = std::fs::remove_dir_all(&cache);
                panic!("mmdc is present but not usable here: {e}");
            }
        };
        assert!(img.width_px > 0 && img.height_px > 0);
        assert!(
            cache
                .join(format!("{}.png", cache_key("graph LR\n  A --> B\n", 600)))
                .is_file(),
            "the first call must have populated the cache"
        );
        // Second call must be served from cache.
        let again = runner.render("graph LR\n  A --> B\n", 600).unwrap();
        assert_eq!(again.png, img.png);
        let _ = std::fs::remove_dir_all(&cache);
    }
}
