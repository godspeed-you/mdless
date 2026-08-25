# diple — Configuration Reference

## File location

diple reads, in order of precedence:

1. `--no-config` — skip all configuration files, use built-in defaults
2. `--config <PATH>` — this file (an error if it does not exist)
3. `$DIPLE_CONFIG` — this file (an error if it does not exist)
4. `$XDG_CONFIG_HOME/diple/config.toml`, normally
   `~/.config/diple/config.toml` (silently skipped if absent)

Validate a file without starting the reader:

```bash
diple --check-config
diple --config ./my.toml --check-config
```

## Value precedence

```
built-in defaults  <  configuration file  <  environment  <  command line
```

Environment overrides: `DIPLE_CONFIG`, `DIPLE_THEME`, `DIPLE_MERMAID`.

## Complete example with defaults

```toml
theme = "auto"          # auto | dark | light | crt | <name>
color = "auto"          # auto | always | never
mouse = true            # enable mouse reporting where supported
toc = false             # start with the table-of-contents sidebar open
key_hints = false       # start with the key hints sidebar open
line_numbers = false    # document line numbers
wrap = true             # reflow paragraphs to the terminal width
max_width = 160         # cap the line width in columns; 0 = the full width
center = true           # centre the document between the sidebars

[table]
mode = "auto"           # auto | wrap | scroll | compact
max_column_width = 60   # cells; must be >= 3

[code]
wrap = false            # wrap long code lines instead of scrolling
line_numbers = false    # line numbers inside code blocks
tab_width = 4           # tab expansion width; must be >= 1

[links]
opener = "xdg-open"     # command used to open external links
osc8 = "auto"           # auto | always | never — native terminal hyperlinks

[mermaid]
backend = "auto"        # auto | terminal | mmdc | source
images = "auto"         # auto | always | never — image protocol usage
mmdc_command = "mmdc"   # Mermaid CLI executable

[keys]
# Overrides only; every unlisted action keeps its default binding.
# See docs/keybindings.md for action names and key syntax.
quit = "q"
search = "/"
next_heading = "]"
previous_heading = "["
toggle_toc = "t"
toggle_key_hints = "K"
toggle_fold = "za"
```

## Keys

### `theme`

`auto` picks light or dark from terminal hints and falls back to dark. `dark`,
`light` and `crt` are built in. Any other name is accepted and resolved against
installed themes; an unknown name falls back to the built-in theme for the
detected background.

`crt` is the phosphor-terminal theme: green on a painted dark screen with amber
for anything alarming, the way a computer looked in a film from the early
nineties. It draws its own background rather than borrowing the terminal's, it
underlines emphasis instead of slanting it, and it turns syntax highlighting
off — a dozen hues in a code block would undo the two colours the rest of the
theme is built from.

### `color`

`auto` uses the detected color depth (true color → 256 → 16 → none). `never`
disables all color and reduces styling to bold/underline/reverse — this is also
what `NO_COLOR` does. `always` forces color even when output is not a terminal.

### `mouse`

Mouse wheel scrolling, clicking a heading to fold it, and clicking a link to
select it. diple asks the terminal for button presses and releases only
(modes `1000` and `1006`), never for drag or motion reporting, because those
are what stop a terminal from selecting text with the mouse.

Selecting still competes with reporting: while diple is listening, most
terminals need `Shift` held to select, and some will not select at all. `m`
(`toggle_mouse`) hands the mouse back at any time — dragging then selects and
copies exactly as it does elsewhere, and the wheel and the clickable sidebars
return when you press `m` again. `mouse = false` or `--no-mouse` starts that
way permanently.

### `[table] mode`

| Mode | Behavior |
|---|---|
| `auto` | shrink columns to fit; wrap where useful; scroll horizontally if the table still cannot fit reasonably |
| `wrap` | always wrap cell content down to the minimum column width |
| `scroll` | never wrap; render the table at full width and scroll horizontally |
| `compact` | no outer borders, minimal padding — best for very narrow terminals |

`max_column_width` caps how much width a single column may claim before the
remaining width is distributed to the others.

### `toc`

`true` opens the table-of-contents sidebar at startup; `t` toggles it at any
time. Its width follows the document: as wide as the widest entry needs, but
never more than 40 columns and never more than a third of the screen, so a
document of short headings gives the columns it does not need back to the
text. Entries wider than that are reached with `h`/`l` while the sidebar has
the focus — see [keybindings.md](keybindings.md).

### `key_hints`

`true` opens the key hints sidebar on the right-hand edge at startup; `K`
toggles it at any time. It defaults to `false` because the default layout
prioritises document space. The sidebar shows only the commands available in
the current mode and cursor context, with the key labels taken from the live
key map. It hides itself when the terminal is too narrow to leave 40 columns
for the document, and yields to the table of contents when only one of the two
sidebars fits.

### `max_width` and `center`

`max_width` caps the line width in columns before wrapping; `0` keeps the full
available width. It defaults to `160`, because a line much longer than that is
tiring to read on a wide terminal. Both `max_width` and `--width` only ever
narrow, so a value wider than the terminal changes nothing — on a terminal of
160 columns or fewer the default limit does nothing at all.

`center = true` — the default — splits the columns the limit leaves over into
two equal margins around the document: with `max_width = 140` on a 200-column
terminal the text sits in the middle 140 columns with 30 columns of air on
each side (an odd remainder gives the extra column to the right).

Set `max_width = 0` for the full width, and `center = false` to keep the
document against the left edge.

Only the document is centred. The table of contents keeps the left edge and
the key hints sidebar the right one, so with a narrow `max_width` both sit
outside the text rather than beside it, and the document stays centred in
whatever the two of them leave over.

Centring applies to the interactive view only: piped output (`diple doc.md |
less -R`) honours `max_width` but is not padded, because leading blanks in a
pipe belong to no screen.

### `[code]`

`wrap = false` renders long code lines at full width so they can be scrolled
horizontally with `h`/`l`, which keeps code copy-pasteable. `wrap = true`
soft-wraps with a continuation indent.

### `[links] osc8`

`auto` emits OSC 8 terminal hyperlinks only where they are known to work
(kitty, WezTerm, iTerm2, foot, Konsole, recent VTE terminals; not inside plain
tmux). `always` and `never` override the detection. Regardless of this setting,
`o` and `Enter` always work.

### `[mermaid]`

| Key | Effect |
|---|---|
| `backend = "auto"` | follow the fallback matrix (see [mermaid.md](mermaid.md)) |
| `backend = "terminal"` | only the built-in terminal renderer; unsupported diagrams show their source |
| `backend = "mmdc"` | always use the Mermaid CLI; fall back to source if it is unavailable or fails |
| `backend = "source"` | never render; always show the Mermaid source |
| `images` | `auto` uses an image protocol where detected, `never` forces text output, `always` forces image output where a protocol exists |
| `mmdc_command` | path or name of the Mermaid CLI binary |

Inspect what diple detected for your terminal:

```bash
diple --print-capabilities
```

## Error reporting

An invalid configuration is reported before anything is rendered, naming the
file, the line, the offending key, the value and what was expected:

```
~/.config/diple/config.toml:9: invalid value for `table.mode`: `fancy`
  — expected one of: auto, wrap, scroll, compact
```

Unknown keys are rejected rather than ignored, so typos surface immediately.
