# Changelog

All notable changes to mdless are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/1.1.0/) and the project uses
[semantic versioning](https://semver.org/).

`scripts/release.sh` and the CI `release:prepare` job cut the release notes from
the `## [<version>]` section matching the tag they are given, so every release
must have its own section here **before** it is tagged. Keep `## [Unreleased]`
at the top for work that has not shipped yet.

## [Unreleased]

Nothing yet.

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

[Unreleased]: https://github.com/godspeed-you/mdless/compare/v0.2.0...main
[0.2.0]: https://github.com/godspeed-you/mdless/releases/tag/v0.2.0
