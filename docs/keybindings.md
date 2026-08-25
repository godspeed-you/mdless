# diple — Keybinding Reference

All bindings are configurable in the `[keys]` section of the configuration file
(see [configuration.md](configuration.md)). Press `?` or `F1` inside diple for
the same list with your own overrides applied.

Keys follow `less` and Vim conventions where practical.

## Paging and scrolling

| Keys | Action name | Description |
|---|---|---|
| `j`, `↓` | `scroll_down` | scroll down one line |
| `k`, `↑` | `scroll_up` | scroll up one line |
| `PgDn`, `Space` | `page_down` | page down |
| `PgUp`, `b` | `page_up` | page up |
| `Ctrl-D` | `half_page_down` | half page down |
| `Ctrl-U` | `half_page_up` | half page up |
| `h`, `←` | `scroll_left` | scroll left (wide tables, unwrapped code, the TOC) |
| `l`, `→` | `scroll_right` | scroll right |
| `g` | `top` | top of document |
| `G` | `bottom` | bottom of document |

## Search

| Keys | Action name | Description |
|---|---|---|
| `/` | `search` | open the search prompt |
| `n` | `next_search` | next search result |
| `N` | `previous_search` | previous search result |

Search runs over document content, not over rendered terminal lines. Jumping to
a match inside a collapsed section expands that section automatically.

## Heading navigation

| Keys | Action name | Description |
|---|---|---|
| `]` | `next_heading` | next heading |
| `[` | `previous_heading` | previous heading |
| `}` | `next_heading_same_level` | next heading at the same or a higher level |
| `{` | `previous_heading_same_level` | previous heading at the same or a higher level |

Heading jumps are semantic: they target the heading node, not a line number, and
they remain correct after a terminal resize.

## Folding

| Keys | Action name | Description |
|---|---|---|
| `Enter` | `activate` | toggle the section under the cursor (or open a selected link) |
| `za` | `toggle_fold` | toggle the current section |
| `zc` | `collapse_fold` | collapse the current section |
| `zo` | `expand_fold` | expand the current section |
| `zM` | `collapse_all` | collapse all sections |
| `zR` | `expand_all` | expand all sections |

A collapsed section shows `▶ Heading`; an expanded one shows `▼ Heading`.
Fold state lives for the current session only.

## Links

| Keys | Action name | Description |
|---|---|---|
| `Tab` | `next_link` | select the next link |
| `Shift-Tab` | `previous_link` | select the previous link |
| `o` | `open_link` | open the selected link with the configured opener |
| `Enter` | `activate` | open the selected link, or follow an internal `#anchor` |

Internal links (`[text](#anchor)`) jump within the document. External links are
handed to `links.opener` (default `xdg-open`). Where the terminal supports OSC 8,
links are also emitted as native terminal hyperlinks.

## View

| Keys | Action name | Description |
|---|---|---|
| `t` | `toggle_toc` | toggle the table-of-contents sidebar |
| `K` | `toggle_key_hints` | toggle the key hints sidebar |
| `m` | `toggle_mouse` | toggle mouse reporting (off: select text with the mouse) |
| `:` | `command_prompt` | open the command line to change a setting |
| `s` | `toggle_mermaid_source` | toggle Mermaid source view for the diagram at the cursor |
| `?`, `F1` | `help` | show the help overlay |
| `Esc` | `cancel` | close the overlay, prompt or sidebar |
| `q`, `Ctrl-C` | `quit` | quit |

Inside the TOC sidebar, `j`/`k` move the selection and `Enter` jumps to the
heading; the section currently shown in the document is marked. A jump keeps
the focus in the sidebar, so `j`/`k` go on walking the outline and further
jumps need no reopening; `Esc` or `t` hands the keys back to the document.

The sidebar is as wide as its widest entry, up to 40 columns and never more
than a third of the screen. A heading longer than that is not truncated for
good: while the sidebar has the focus, `h`/`l` (or `←`/`→`) scroll the outline
sideways instead of the document, and the key hints offer them only while
something is actually cut off. Closing and reopening the sidebar returns it to
the left edge of the outline.

## The command line

`:` opens a command line for changing a setting while diple is running. The key
is the one the configuration file uses, dotted for a section, and the separator
is `=` or a space — `:center = false`, `:theme crt`, `:table.mode compact` all
work, as does a leading `set` for the muscle memory it comes from.

| Input | Effect |
|---|---|
| `:center` | report what `center` is currently set to |
| `:center = false` | set it, and lay the document out again |
| `Tab` | complete the key, or the value once a separator is typed |
| `:help` | every setting, its accepted values and its default |
| `:q` | quit |
| `Esc` | leave the line without applying it |

Completion stops where the answer stops being unique: it fills in the longest
prefix every candidate shares and lists the rest in the status line. A key that
completes to exactly one match gains its ` = ` too, so the next keystroke is
already the value.

Changes apply immediately and last for the session — the configuration file is
never written, so a change that turns out badly is undone by restarting.

The key hints sidebar (`K`) is drawn on the right-hand edge and lists, grouped
and labelled, the commands that are available *right now* — it is not a static
copy of the help overlay. It follows the mode (normal, search prompt, TOC,
help) and the cursor context: horizontal scrolling appears only when the view
can actually scroll sideways, the link keys only when a link is visible, the
Mermaid source toggle only at or near a diagram, and `n`/`N` only while a
search has matches. Every key label is read from the live key map, so a
rebound key shows its new binding and an unbound action is left out entirely.
Where the mouse is enabled, clicking a row runs that command.

Opposed key pairs share a line (`j/k scroll`, `]/[ next/prev`, `zc/zo
collapse/expand`); if you unbind one half, the row collapses to the half that
still works.

The sidebar takes its width from the document area. In a terminal too narrow
to leave at least 40 columns for the document it hides itself, and if both
sidebars are open and only one fits, the table of contents wins. When the
groups are taller than the screen, the blank rows between them go first, then
whole groups in this order: Diagram, Links, Search, Fold, Headings — heading
navigation and folding outrank the generic pager rows, and `Move` and `View`
(which holds `q`, `?` and `K` itself) are the last to go.

## Key specification syntax

Values in `[keys]` use this syntax:

- single characters: `q`, `/`, `?`, `G`
- named keys: `enter`, `esc`, `space`, `tab`, `shift-tab`, `backtab`, `pgup`,
  `pgdn`, `up`, `down`, `left`, `right`, `home`, `end`, `f1` … `f24`
- modifiers: `ctrl-d`, `alt-enter`, `ctrl-shift-f5`
- multi-key sequences: written together (`za`, `zM`) or separated by spaces
  (`g g`, `z enter`)

A `[keys]` entry replaces **all** default bindings for that action. To bind
several keys to one action, use a list:

```toml
[keys]
quit = ["q", "ctrl-q"]
toggle_fold = "f t"
```

If a multi-key sequence fails to match, the last key is retried on its own — so
after a stray `z`, pressing `q` still quits.
