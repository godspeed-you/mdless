# mdless

**`less` for Markdown documents instead of text files.**

[![CI](https://github.com/godspeed-you/mdless/actions/workflows/ci.yml/badge.svg)](https://github.com/godspeed-you/mdless/actions/workflows/ci.yml)
[![Latest release](https://img.shields.io/github/v/release/godspeed-you/mdless?sort=semver)](https://github.com/godspeed-you/mdless/releases/latest)
[![License: MIT](https://img.shields.io/github/license/godspeed-you/mdless)](LICENSE)
[![Rust 1.80+](https://img.shields.io/badge/rust-1.80%2B-orange?logo=rust)](rust-toolchain.toml)

An interactive terminal Markdown reader with semantic navigation, collapsible
sections, terminal-aware tables, syntax-highlighted code and Mermaid diagrams.

mdless combines the rendering quality of tools such as `glow` with the
interaction model of `less`. Markdown is treated as a structured document, not
as colored text: navigation, folding and search operate on the document model,
so they stay correct when the terminal is resized.

```bash
mdless README.md
```

```text
┌────────────────────────────────────────────────────────────┐
│ README.md                                      37%  142/380 │
├────────────────────────────────────────────────────────────┤
│                                                            │
│  Project Foo                                               │
│  ═══════════                                               │
│                                                            │
│  This project provides ...                                 │
│                                                            │
│  Installation                                              │
│  ────────────                                              │
│                                                            │
│    $ cargo install foo                                     │
│                                                            │
│  ▶ Configuration                                           │
│                                                            │
└────────────────────────────────────────────────────────────┘
```

## Features

- **Interactive pager** — never dumps into your shell scrollback
- **Semantic heading navigation** — jump by heading, not by line number
- **Collapsible sections** — fold any heading, `zM`/`zR` for the whole document
- **Full-text search** — searches document content, and expands collapsed
  sections containing a match
- **Table of contents** — sidebar reflecting the real heading hierarchy
- **Key hints sidebar** — `K` shows, on the right, the commands available right
  now, following the mode and the cursor context
- **Terminal-aware tables** — column widths computed from content and terminal
  width, with wrapping or horizontal scrolling
- **Syntax highlighting** — fenced code blocks, optional line numbers
- **Links** — keyboard selection, opening via `xdg-open`, OSC 8 hyperlinks
  where supported
- **Mermaid diagrams** — rendered natively in the terminal, as images via
  `mmdc` where the terminal supports it, with a deterministic source fallback
- **Graceful degradation** — works over SSH, in tmux, without true color,
  without Unicode and without image support

## Installation

### Packages

```bash
# Debian / Ubuntu
sudo apt install ./mdless_0.2.0_amd64.deb

# Fedora / RHEL
sudo dnf install ./mdless-0.2.0.x86_64.rpm

# Arch Linux
cd packaging/arch && makepkg -si
```

### From source

```bash
cargo install --path .
```

Requires Rust 1.80 or newer and a C compiler. The C compiler is needed for
the oniguruma regex engine, which mdless uses by default because it is what
makes the startup budget reachable — a syntax definition is compiled on first
use, and that cost lands on the first frame. If you cannot provide a C
toolchain, build with the pure-Rust engine instead:

```bash
cargo install --path . --no-default-features --features syntax-fancy
```

Both produce byte-identical output; the pure-Rust engine is simply slower to
show the first frame (p50 73 ms versus 17 ms in a 100x24 terminal). For
`*-linux-musl` targets the C compiler must be a real musl compiler
(`musl-tools`), because the host's glibc `cc` emits `_FORTIFY_SOURCE` symbols
that musl does not provide:

```bash
CC_x86_64_unknown_linux_musl=musl-gcc \
CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=musl-gcc \
cargo build --release --target x86_64-unknown-linux-musl
```

Optional: `mmdc` (`npm install -g @mermaid-js/mermaid-cli`) for Mermaid
diagrams that the built-in renderer does not cover.

## Usage

```bash
mdless README.md            # read a file
cat README.md | mdless      # read from stdin
mdless < README.md          # read from a redirect
git show HEAD:README.md | mdless
```

When output is not a terminal, mdless prints the rendered document as plain
text and exits — so `mdless README.md | head -20` and CI usage behave sensibly.

### Key bindings

| Key | Action |
|---|---|
| `j` `k` `↓` `↑` | scroll |
| `Space` `b` `PgDn` `PgUp` | page |
| `g` `G` | top / bottom |
| `h` `l` `←` `→` | scroll horizontally |
| `/` `n` `N` | search, next, previous |
| `[` `]` | previous / next heading |
| `{` `}` | previous / next heading at the same or a higher level |
| `Enter` | toggle the section under the cursor, or open the selected link |
| `za` `zc` `zo` | toggle / collapse / expand the current section |
| `zM` `zR` | collapse / expand everything |
| `Tab` `Shift-Tab` `o` | select links, open the selected one |
| `t` | toggle the table of contents |
| `K` | toggle the key hints sidebar |
| `s` | toggle Mermaid source view |
| `?` | help |
| `q` | quit |

Full list, including how to rebind: [docs/keybindings.md](docs/keybindings.md).

### Options

```text
mdless [OPTIONS] [FILE]

  --theme <auto|dark|light|NAME>     --color <auto|always|never>
  --width <COLUMNS>                  --mouse / --no-mouse
  --toc / --no-toc                   --key-hints / --no-key-hints
  --line-numbers / --no-line-numbers
  --wrap / --no-wrap
  --mermaid <auto|terminal|mmdc|source>
  --mermaid-images <auto|always|never>
  --config <PATH> / --no-config
  --print-capabilities  --check-config  --debug
  -h, --help  -V, --version
```

## Configuration

`~/.config/mdless/config.toml`:

```toml
theme = "auto"
mouse = true
toc = false
key_hints = false

[table]
mode = "auto"

[code]
line_numbers = false

[mermaid]
backend = "auto"

[keys]
quit = "q"
next_heading = "]"
```

Command-line options override the configuration file. Full reference:
[docs/configuration.md](docs/configuration.md).

Validate a configuration without opening a document:

```bash
mdless --check-config
```

## Using mdless as a Git pager

```bash
git config core.pager mdless
git show HEAD:README.md | mdless
```

## Documentation

- [Configuration reference](docs/configuration.md)
- [Keybinding reference](docs/keybindings.md)
- [Mermaid behavior](docs/mermaid.md)
- [Terminal compatibility checklist](docs/terminal-compatibility-checklist.md)
- `man mdless`

## Troubleshooting

**Colors look wrong or are missing.** Run `mdless --print-capabilities`; it
reports what was detected and the evidence for each decision. `NO_COLOR` and a
non-terminal stdout always disable color. Force with `--color always`.

**Mermaid diagrams show as source.** See
[docs/mermaid.md](docs/mermaid.md#troubleshooting). Inside tmux, image
protocols need `set -g allow-passthrough on`.

**Box drawing shows as question marks.** Your locale is not UTF-8; mdless falls
back to ASCII automatically, so check `LANG`/`LC_ALL` if you expected Unicode.

**Tables are cut off.** Scroll horizontally with `h`/`l`, or set
`[table] mode = "wrap"` to wrap cells instead.

**The terminal looks broken after a crash.** mdless restores the terminal on
every exit path including panics; if something still slipped through, `reset`
fixes it — and please report it: mdless treats terminal
corruption as a release blocker.

## Development

```bash
cargo test                 # unit, integration and snapshot tests
cargo clippy --all-targets -- -D warnings
cargo fmt --all --check
cargo bench
```

Snapshot tests use [insta](https://insta.rs/); review changes with
`cargo insta review`.

## License

MIT — see [LICENSE](LICENSE).
