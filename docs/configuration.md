# mdless — Configuration Reference

## File location

mdless reads, in order of precedence:

1. `--no-config` — skip all configuration files, use built-in defaults
2. `--config <PATH>` — this file (an error if it does not exist)
3. `$MDLESS_CONFIG` — this file (an error if it does not exist)
4. `$XDG_CONFIG_HOME/mdless/config.toml`, normally
   `~/.config/mdless/config.toml` (silently skipped if absent)

Validate a file without starting the reader:

```bash
mdless --check-config
mdless --config ./my.toml --check-config
```

## Value precedence

```
built-in defaults  <  configuration file  <  environment  <  command line
```

Environment overrides: `MDLESS_CONFIG`, `MDLESS_THEME`, `MDLESS_MERMAID`.

## Complete example with defaults

```toml
theme = "auto"          # auto | dark | light | <name>
color = "auto"          # auto | always | never
mouse = true            # enable mouse reporting where supported
toc = false             # start with the table-of-contents sidebar open
key_hints = false       # start with the key hints sidebar open
line_numbers = false    # document line numbers
wrap = true             # reflow paragraphs to the terminal width

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

`auto` picks light or dark from terminal hints and falls back to dark. `dark`
and `light` are built in. Any other name is accepted and resolved against
installed themes; an unknown name falls back to the built-in theme for the
detected background.

### `color`

`auto` uses the detected color depth (true color → 256 → 16 → none). `never`
disables all color and reduces styling to bold/underline/reverse — this is also
what `NO_COLOR` does. `always` forces color even when output is not a terminal.

### `mouse`

Mouse wheel scrolling, clicking a heading to fold it, and clicking a link to
select it. Disable with `mouse = false` or `--no-mouse` if it interferes with
your terminal's own selection handling.

### `[table] mode`

| Mode | Behavior |
|---|---|
| `auto` | shrink columns to fit; wrap where useful; scroll horizontally if the table still cannot fit reasonably |
| `wrap` | always wrap cell content down to the minimum column width |
| `scroll` | never wrap; render the table at full width and scroll horizontally |
| `compact` | no outer borders, minimal padding — best for very narrow terminals |

`max_column_width` caps how much width a single column may claim before the
remaining width is distributed to the others.

### `key_hints`

`true` opens the key hints sidebar on the right-hand edge at startup; `K`
toggles it at any time. It defaults to `false` because the default layout
prioritises document space. The sidebar shows only the commands available in
the current mode and cursor context, with the key labels taken from the live
key map. It hides itself when the terminal is too narrow to leave 40 columns
for the document, and yields to the table of contents when only one of the two
sidebars fits.

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

Inspect what mdless detected for your terminal:

```bash
mdless --print-capabilities
```

## Error reporting

An invalid configuration is reported before anything is rendered, naming the
file, the line, the offending key, the value and what was expected:

```
~/.config/mdless/config.toml:9: invalid value for `table.mode`: `fancy`
  — expected one of: auto, wrap, scroll, compact
```

Unknown keys are rejected rather than ignored, so typos surface immediately.
