# Changelog

All notable changes to diple are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/1.1.0/) and the project uses
[semantic versioning](https://semver.org/).

`scripts/release.sh` and the CI `release:prepare` job cut the release notes from
the `## [<version>]` section matching the tag they are given, so every release
must have its own section here **before** it is tagged. Keep `## [Unreleased]`
at the top for work that has not shipped yet.

## [Unreleased]

### Fixed

- **The line width limit and centring are now on by default.** `max_width` and
  `center` shipped as `0` and `false`, so a fresh installation still laid every
  document out across the full terminal — the very reading problem the two
  settings were added to solve, left switched off. They now default to
  `max_width = 160` and `center = true`, so a wide terminal gets a comfortable
  measure in the middle of the screen without any configuration. Nothing
  changes on a terminal of 160 columns or fewer, since the limit only ever
  narrows. Set `max_width = 0` and `center = false` (or pass `--max-width 0
  --no-center`) for the previous behaviour.

## [1.0.0] - 2026-08-24

First stable release.

### Fixed

- **A signal no longer leaves the terminal unusable.** `SIGTERM`, `SIGHUP`,
  `SIGINT` and `SIGQUIT` killed diple mid-frame, so the shell that came back
  was still in raw mode on the alternate screen with the cursor hidden and
  mouse reporting on — unusable until `reset`. They now run the same
  restoration as every other exit path and then re-raise with the default
  disposition, so the exit status still reports the signal. Ctrl-C and panics
  were never affected.

### Added

- `max_width` caps the line width before wrapping, and `center` puts the
  document in the middle of the screen with equal margins on both sides.
  Available as `--max-width <COLUMNS>` and `--center` / `--no-center` too. The
  sidebars keep the screen edges: a centred document has the table of contents
  to its left and the key hints to its right, outside the text. Piped output
  honours `max_width` but is never padded.

### Changed

- A jump from the table of contents (`Enter`) keeps the focus in the sidebar
  instead of returning to the document, so `j`/`k` go on walking the outline
  and several headings can be visited in a row. `Esc` or `t` leaves the
  sidebar.

- **The project is now called `diple`.** The binary, the crate and the
  configuration all follow: run `diple`, configure it in
  `~/.config/diple/config.toml` and override it with `DIPLE_*` environment
  variables. There is no compatibility shim — `mdless`, `~/.config/mdless/`
  and `MDLESS_*` are gone. Move your configuration file and rename your
  environment variables when upgrading from 0.2.0.

## [0.2.0] - 2026-08-23

Feature-complete for 1.0, released as a minor version because four of the
release checks cannot be performed without real hardware and root, and are
therefore still open:

- the terminal matrix (GNOME Terminal, Konsole, Kitty, Alacritty,
  WezTerm, tmux, SSH, macOS, terminals without true colour or images) — see
  `docs/terminal-compatibility-checklist.md`
- that diagram images actually appear in an image-capable terminal
- installing and purging the `.deb` and `.rpm` as root
- resizing a multi-megabyte document, which still re-lays out the whole
  document (144 ms at 1 MB, 561 ms at 4 MB)

`1.0.0` is reserved for the release in which those are verified.

### Changed

- **Building from source now requires a C compiler.** The default syntax
  regex engine is oniguruma, because it is what makes the startup budget
  reachable: measured in a 100x24 terminal, time to first frame is p50 17 ms
  against p50 73 ms with the previous pure-Rust engine, where the budget
  requires p50 < 30 ms. Behaviour is unaffected — glibc, static musl and the
  pure-Rust build produce byte-identical output including highlighting.
  Environments without a C toolchain can build the previous engine with
  `--no-default-features --features syntax-fancy`; for `*-linux-musl` the
  compiler must be a real musl compiler (`musl-tools`).

### Added

- Interactive terminal Markdown reader: the document is rendered as structure,
  not as coloured text, and read in an alternate screen that leaves no output
  in the shell's scrollback.
- Semantic navigation: heading-to-heading jumps, a table-of-contents sidebar
  and a viewport anchored to `(node, offset)` so position survives resizing,
  folding and re-layout.
- Key hints sidebar (`K`, `key_hints`, `--key-hints`/`--no-key-hints`): a
  right-hand list of the commands available right now, grouped and labelled,
  following the mode and the cursor context, with labels read from the live
  key map so custom bindings are shown.
- Collapsible sections with per-section and document-wide fold commands;
  searching reveals a match inside a collapsed section.
- Incremental full-text search with wrap-around and match highlighting.
- Terminal-aware rendering: width-driven table layout with scroll, wrap and
  compact modes, syntax-highlighted code blocks, nested lists, blockquotes,
  footnotes and task lists.
- Link interaction: selection, in-document anchor jumps, external opening and
  OSC 8 hyperlinks where the terminal supports them.
- Mermaid support: a built-in renderer for the supported flowchart subset,
  `mmdc` integration, terminal image protocols (Kitty, Sixel, iTerm2) and a
  source fallback that is always reachable, including on the non-interactive
  path.
- Graceful degradation for colour depth, Unicode box drawing, mouse, images,
  OSC 8 and a missing `mmdc`, with `--print-capabilities` to explain every
  decision.
- Configuration file (`~/.config/mdless/config.toml`) with rebindable keys,
  `--check-config` validation reporting path, line, key, value and expected
  form, and `MDLESS_*` environment overrides.
- Non-interactive output for pipes and CI, honouring `--color always`,
  `--color never` and `--width`.
- Packaging: `.deb`, `.rpm`, an Arch `PKGBUILD`, standalone Linux tarballs, a
  man page and bash/zsh/fish completions.

[Unreleased]: https://github.com/godspeed-you/diple/compare/v1.0.0...main
[1.0.0]: https://github.com/godspeed-you/diple/releases/tag/v1.0.0
[0.2.0]: https://github.com/godspeed-you/diple/releases/tag/v0.2.0
