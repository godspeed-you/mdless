# Terminal Compatibility Checklist

diple must be verified in real terminals before a
1.0 release. Rendering correctness at fixed widths is covered by automated
snapshot tests; what cannot be automated is how a real terminal handles raw
mode, escape sequences, image protocols and resize. This checklist covers that
gap and must be completed for each release candidate.

Run `diple --print-capabilities` first in every environment and record the
output — it states what diple detected and why.

## Procedure per environment

For each terminal below, perform every step and record pass/fail:

1. **Basic read** — `diple docs/configuration.md`, scroll with `j`/`k`,
   `Space`/`b`, `g`/`G`.
2. **No scrollback dump** — the document must appear on the alternate screen and
   leave the shell scrollback untouched.
3. **Clean exit** — quit with `q`; the prompt must return with no leftover
   color, no raw mode, a visible cursor and a working terminal.
   Then repeat with `Ctrl-C`.
4. **Panic safety** — kill the process with `SIGTERM` from another shell
   (`pkill diple`); the terminal must still be usable afterwards: cooked mode,
   visible cursor, primary screen, no mouse reporting.
5. **Resize** — resize the window narrow (≈40 cols) and wide (≈160 cols) while
   a table and a code block are on screen; text must reflow, tables must
   re-lay-out, and the reading position must stay on the same section.
6. **Search** — `/` a word inside a collapsed section, `Enter`, then `n`/`N`;
   the section must expand and the match must be highlighted.
7. **Folding** — `zM`, `zR`, `za` on nested sections.
8. **TOC** — `t`, navigate with `j`/`k`, `Enter` to jump.
9. **Wide characters** — open `tests/fixtures/unicode-cjk-emoji.md`; CJK and
   emoji must not break column alignment in the table.
10. **Links** — `Tab` to a link, `o` to open it; if the terminal supports
    OSC 8, `Ctrl`/`Cmd`-click must also work.
11. **Mermaid** — open `tests/fixtures/mermaid.md`; note whether the native
    renderer, an image protocol or the source fallback was used, and confirm
    `s` toggles the source.
12. **Mouse** — wheel scroll, click a heading to fold it (skip if `mouse = false`).
13. **stdin** — `cat README.md | diple` must be fully interactive (keyboard
    input comes from `/dev/tty`).

## Environments

| Environment | Expected notes |
|---|---|
| Local Linux terminal (default `$TERM`) | baseline |
| GNOME Terminal / VTE | OSC 8 yes (VTE ≥ 0.50), no images |
| Konsole | OSC 8 yes (≥ 20), no images |
| Kitty | true color, OSC 8, Kitty graphics protocol for diagrams |
| Alacritty | true color, no OSC 8, no images |
| WezTerm | true color, OSC 8, iTerm2-style images |
| foot | sixel path |
| tmux (inside each of the above) | images disabled by default; needs `set -g allow-passthrough on` to enable |
| GNU screen | conservative fallback |
| SSH session | check latency of scrolling and resize |
| macOS Terminal.app | no OSC 8, no images, 256 colors |
| macOS iTerm2 | inline images |
| `TERM=dumb` / `NO_COLOR=1` | no color, plain ASCII, still readable |
| Non-UTF-8 locale (`LC_ALL=C`) | ASCII box drawing and bullets, no broken glyphs |
| Very narrow terminal (40 cols) | tables remain usable, no horizontal body overflow |

## Recording results

Copy this file into the release notes directory per release candidate and fill
in the results, including terminal versions and the `--print-capabilities`
output for anything that failed. A release is blocked by any failure in steps
2, 3, 4 or 10 (terminal corruption and data-loss risks).

## Results

### 1.0.0 — 2026-08-24

The procedure was run by Marcel Arentz across the environments above and
reported as passing; this entry records that report. Terminal versions and the
`--print-capabilities` output were not captured — add them here if a later
release needs them for comparison.

One exception is recorded deliberately, because it was measured afterwards and
contradicts a blanket pass:

- **Step 4, the `SIGTERM` half, fails.** diple installs no signal handler
  (`grep -rn SIGTERM src` finds nothing), so a `SIGTERM` terminates the
  process before `TerminalGuard` can run. Measured against the 1.0.0 release
  binary in a pty: the alternate screen is never left, the cursor stays
  hidden, mouse reporting stays on and the pty stays in raw mode — the shell
  that follows is unusable until `reset`. `Ctrl-C` and panics are unaffected;
  both restore correctly, because they are ordinary exit paths rather than an
  external signal.

  Step 4 is one of the steps a failure in which blocks a release, so this
  needs either a fix (a `SIGTERM`/`SIGHUP` handler that restores the terminal
  and re-raises) or a deliberate, recorded waiver before the next tag.
