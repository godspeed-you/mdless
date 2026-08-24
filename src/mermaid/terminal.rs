//! Native ASCII/Unicode flowchart rendering (Backend A).
//!
//! The algorithm is a small Sugiyama-style pipeline:
//!
//! 1. **Layering** — longest-path layering over the DAG. Cycles are broken
//!    deterministically by a depth-first search in node-declaration order; the
//!    ignored back edges are still *drawn* (as feedback edges pointing at an
//!    earlier layer).
//! 2. **Ordering** — barycentre heuristic, four alternating sweeps, ties broken
//!    by the current position (stable) so the output is byte-identical for
//!    identical input.
//! 3. **Sizing** — labels are wrapped to a maximum box width using
//!    Unicode display width (never `str::len`).
//! 4. **Placement & routing** — boxes are painted onto a character grid;
//!    edges are routed orthogonally through the gap between two layers.
//!
//! `LR`/`RL` place layers along the x axis, `TD`/`BT` along the y axis; `RL`
//! and `BT` simply invert the layer index, and arrow heads follow the actual
//! geometric direction.
//!
//! ## Width policy
//!
//! Output lines **never exceed** [`RenderOptions::width_cells`]. If the diagram
//! does not fit, labels are progressively shortened; if it still does not fit,
//! [`NativeError::TooWide`] is returned so that
//! [`select_backend`](crate::mermaid::select_backend) can fall back to source
//! rendering with a non-fatal warning.

use super::ast::{ArrowKind, Diagram, EdgeStyle, NodeShape};
use crate::util::unicode;
/// Display width of `s` in terminal cells.
///
/// The Mermaid canvas is a fixed character grid painted by column index, so it
/// must measure text exactly as the rest of the renderer does: grapheme-cluster
/// aware ([`crate::util::unicode::width`]), not `UnicodeWidthStr::width`.
use crate::util::unicode::width as display_width;

/// Options for [`render`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderOptions {
    /// Maximum number of terminal columns the diagram may occupy.
    /// `0` means "unlimited".
    pub width_cells: usize,
    /// Draw with box-drawing characters; `false` selects the ASCII fallback.
    pub unicode_box: bool,
    /// Preferred maximum label width (in display cells) before wrapping.
    pub max_label_width: usize,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            width_cells: 80,
            unicode_box: true,
            max_label_width: 32,
        }
    }
}

/// Why the native renderer declined to draw a diagram.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NativeError {
    /// The diagram declares no nodes.
    #[error("the diagram contains no nodes")]
    Empty,
    /// Even with maximally shortened labels the diagram is wider than the
    /// available terminal width.
    #[error("diagram needs {needed} columns but only {available} are available")]
    TooWide {
        /// Columns the narrowest attempt required.
        needed: usize,
        /// Columns available.
        available: usize,
    },
}

/// Renders `diagram` into padded terminal lines.
///
/// All returned lines have the same display width and none exceeds
/// [`RenderOptions::width_cells`].
///
/// # Errors
///
/// [`NativeError::Empty`] for a node-less diagram, [`NativeError::TooWide`]
/// when the graph cannot be squeezed into the available width.
pub fn render(diagram: &Diagram, opts: &RenderOptions) -> Result<Vec<String>, NativeError> {
    if diagram.nodes.is_empty() {
        return Err(NativeError::Empty);
    }
    let limit = if opts.width_cells == 0 {
        usize::MAX
    } else {
        opts.width_cells
    };

    let mut attempts: Vec<usize> = vec![opts.max_label_width, 24, 16, 12, 8, 6, 3];
    attempts.retain(|w| *w >= 1);
    attempts.dedup();

    let mut narrowest = usize::MAX;
    for max_label in attempts {
        let lines = layout_and_paint(diagram, opts, max_label);
        let width = lines.iter().map(|l| display_width(l)).max().unwrap_or(0);
        if width <= limit {
            return Ok(lines
                .into_iter()
                .map(|l| unicode::pad_to_width(&l, width))
                .collect());
        }
        narrowest = narrowest.min(width);
    }
    Err(NativeError::TooWide {
        needed: narrowest,
        available: limit,
    })
}

// ---------------------------------------------------------------------------
// Step 1: layering
// ---------------------------------------------------------------------------

/// Returns `(layer_of_node, is_back_edge_per_edge)`.
fn layering(diagram: &Diagram) -> (Vec<usize>, Vec<bool>) {
    let n = diagram.nodes.len();
    let mut back = vec![false; diagram.edges.len()];

    // Depth-first search in declaration order marks edges that close a cycle.
    let mut state = vec![0u8; n]; // 0 = new, 1 = on stack, 2 = done
    let mut out: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (ei, e) in diagram.edges.iter().enumerate() {
        if e.from != e.to {
            if let Some(list) = out.get_mut(e.from) {
                list.push(ei);
            }
        } else {
            back[ei] = true; // self-loop: never a layering constraint
        }
    }

    for root in 0..n {
        if state.get(root).copied() != Some(0) {
            continue;
        }
        // Iterative DFS: (node, index into out[node]).
        let mut stack: Vec<(usize, usize)> = vec![(root, 0)];
        if let Some(s) = state.get_mut(root) {
            *s = 1;
        }
        while let Some(&mut (node, ref mut idx)) = stack.last_mut() {
            let Some(edges) = out.get(node) else {
                stack.pop();
                continue;
            };
            if *idx >= edges.len() {
                if let Some(s) = state.get_mut(node) {
                    *s = 2;
                }
                stack.pop();
                continue;
            }
            let ei = edges[*idx];
            *idx += 1;
            let Some(target) = diagram.edges.get(ei).map(|e| e.to) else {
                continue;
            };
            match state.get(target).copied() {
                Some(0) => {
                    if let Some(s) = state.get_mut(target) {
                        *s = 1;
                    }
                    stack.push((target, 0));
                }
                Some(1) => {
                    if let Some(b) = back.get_mut(ei) {
                        *b = true;
                    }
                }
                _ => {}
            }
        }
    }

    // Longest-path layering over the forward edges (Kahn, index order).
    let mut indeg = vec![0usize; n];
    let mut fwd: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (ei, e) in diagram.edges.iter().enumerate() {
        if back.get(ei).copied().unwrap_or(false) {
            continue;
        }
        if let Some(list) = fwd.get_mut(e.from) {
            list.push(e.to);
        }
        if let Some(d) = indeg.get_mut(e.to) {
            *d += 1;
        }
    }
    let mut layer = vec![0usize; n];
    let mut queue: Vec<usize> = (0..n).filter(|v| indeg.get(*v) == Some(&0)).collect();
    let mut head = 0usize;
    let mut processed = 0usize;
    while head < queue.len() {
        let v = queue[head];
        head += 1;
        processed += 1;
        let Some(targets) = fwd.get(v).cloned() else {
            continue;
        };
        let lv = layer.get(v).copied().unwrap_or(0);
        for t in targets {
            if let Some(lt) = layer.get_mut(t) {
                *lt = (*lt).max(lv + 1);
            }
            if let Some(d) = indeg.get_mut(t) {
                *d -= 1;
                if *d == 0 {
                    queue.push(t);
                }
            }
        }
    }
    // Defensive: a residual cycle (should not happen) leaves nodes at layer 0.
    debug_assert!(processed <= n);
    let _ = processed;
    (layer, back)
}

// ---------------------------------------------------------------------------
// Step 2: ordering
// ---------------------------------------------------------------------------

/// Returns the node order for every layer.
fn ordering(diagram: &Diagram, layer: &[usize], layer_count: usize) -> Vec<Vec<usize>> {
    let mut layers: Vec<Vec<usize>> = vec![Vec::new(); layer_count];
    for (v, l) in layer.iter().enumerate() {
        if let Some(bucket) = layers.get_mut(*l) {
            bucket.push(v);
        }
    }

    let n = diagram.nodes.len();
    let mut preds: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut succs: Vec<Vec<usize>> = vec![Vec::new(); n];
    for e in &diagram.edges {
        if e.from == e.to {
            continue;
        }
        if let Some(p) = preds.get_mut(e.to) {
            p.push(e.from);
        }
        if let Some(s) = succs.get_mut(e.from) {
            s.push(e.to);
        }
    }

    let mut position = vec![0usize; n];
    let sync = |layers: &Vec<Vec<usize>>, position: &mut Vec<usize>| {
        for bucket in layers {
            for (i, v) in bucket.iter().enumerate() {
                if let Some(p) = position.get_mut(*v) {
                    *p = i;
                }
            }
        }
    };
    sync(&layers, &mut position);

    for sweep in 0..4 {
        let down = sweep % 2 == 0;
        let order: Vec<usize> = if down {
            (1..layer_count).collect()
        } else {
            (0..layer_count.saturating_sub(1)).rev().collect()
        };
        for li in order {
            let Some(bucket) = layers.get(li).cloned() else {
                continue;
            };
            let neighbours = if down { &preds } else { &succs };
            let mut keyed: Vec<(f64, usize, usize)> = bucket
                .iter()
                .enumerate()
                .map(|(idx, &v)| {
                    let ns = neighbours.get(v).map(Vec::as_slice).unwrap_or(&[]);
                    let bary = if ns.is_empty() {
                        idx as f64
                    } else {
                        ns.iter()
                            .map(|u| position.get(*u).copied().unwrap_or(0) as f64)
                            .sum::<f64>()
                            / ns.len() as f64
                    };
                    (bary, idx, v)
                })
                .collect();
            keyed.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
            if let Some(slot) = layers.get_mut(li) {
                *slot = keyed.into_iter().map(|(_, _, v)| v).collect();
            }
            sync(&layers, &mut position);
        }
    }
    layers
}

// ---------------------------------------------------------------------------
// Step 3: boxes
// ---------------------------------------------------------------------------

/// Border characters of a node box.
struct BoxChars {
    tl: char,
    tr: char,
    bl: char,
    br: char,
    top: char,
    bottom: char,
    left: char,
    right: char,
}

fn box_chars(shape: NodeShape, unicode: bool) -> BoxChars {
    if !unicode {
        return BoxChars {
            tl: '+',
            tr: '+',
            bl: '+',
            br: '+',
            top: '-',
            bottom: '-',
            left: '|',
            right: '|',
        };
    }
    match shape {
        NodeShape::Rect | NodeShape::Subroutine => BoxChars {
            tl: '┌',
            tr: '┐',
            bl: '└',
            br: '┘',
            top: '─',
            bottom: '─',
            left: if shape == NodeShape::Subroutine {
                '║'
            } else {
                '│'
            },
            right: if shape == NodeShape::Subroutine {
                '║'
            } else {
                '│'
            },
        },
        NodeShape::Round | NodeShape::Stadium => BoxChars {
            tl: '╭',
            tr: '╮',
            bl: '╰',
            br: '╯',
            top: '─',
            bottom: '─',
            left: '│',
            right: '│',
        },
        NodeShape::Circle => BoxChars {
            tl: '╭',
            tr: '╮',
            bl: '╰',
            br: '╯',
            top: '─',
            bottom: '─',
            left: '(',
            right: ')',
        },
        NodeShape::Diamond => BoxChars {
            tl: '╱',
            tr: '╲',
            bl: '╲',
            br: '╱',
            top: '─',
            bottom: '─',
            left: '│',
            right: '│',
        },
        NodeShape::Hexagon => BoxChars {
            tl: '╱',
            tr: '╲',
            bl: '╲',
            br: '╱',
            top: '‾',
            bottom: '_',
            left: '│',
            right: '│',
        },
        NodeShape::Cylinder => BoxChars {
            tl: '╭',
            tr: '╮',
            bl: '╰',
            br: '╯',
            top: '═',
            bottom: '═',
            left: '│',
            right: '│',
        },
    }
}

/// Greedy word wrap on display width; over-long words are split on grapheme
/// boundaries.
///
/// Kept local rather than using [`crate::util::unicode::wrap`]: the greedy
/// split semantics (hard-splitting any word wider than the box) differ
/// deliberately from the layout engine's token-aware wrapper.
fn wrap_label(label: &str, max: usize) -> Vec<String> {
    let max = max.max(1);
    let mut out: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut current_w = 0usize;
    for word in label.split_whitespace() {
        let mut word = word.to_string();
        while display_width(&word) > max {
            let (head, tail) = unicode::split_at_width(&word, max);
            // A single grapheme wider than `max` would make no progress:
            // take it whole and overflow this one line instead of looping.
            let (head, rest) = if head.is_empty() {
                match unicode::graphemes(&word).next() {
                    Some(g) => (g.to_string(), word[g.len()..].to_string()),
                    None => break,
                }
            } else {
                (head.to_string(), tail.to_string())
            };
            if !current.is_empty() {
                out.push(std::mem::take(&mut current));
                current_w = 0;
            }
            out.push(head);
            word = rest;
        }
        let w = display_width(&word);
        if current.is_empty() {
            current = word;
            current_w = w;
        } else if current_w + 1 + w <= max {
            current.push(' ');
            current.push_str(&word);
            current_w += 1 + w;
        } else {
            out.push(std::mem::take(&mut current));
            current = word;
            current_w = w;
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    if out.is_empty() {
        out.push(String::new());
    }
    out
}

struct Boxed {
    lines: Vec<String>,
    w: usize,
    h: usize,
    x: usize,
    y: usize,
    shape: NodeShape,
}

fn build_boxes(diagram: &Diagram, max_label: usize) -> Vec<Boxed> {
    diagram
        .nodes
        .iter()
        .map(|n| {
            let lines = wrap_label(&n.label, max_label);
            let inner = lines.iter().map(|l| display_width(l)).max().unwrap_or(0);
            Boxed {
                w: inner + 4,
                h: lines.len() + 2,
                lines,
                x: 0,
                y: 0,
                shape: n.shape,
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Canvas
// ---------------------------------------------------------------------------

struct Canvas {
    cells: Vec<Vec<char>>,
    /// Cells occupied by a node box: edges may never write there.
    blocked: Vec<Vec<bool>>,
}

impl Canvas {
    fn new(w: usize, h: usize) -> Self {
        Self {
            cells: vec![vec![' '; w]; h],
            blocked: vec![vec![false; w]; h],
        }
    }
    fn is_blocked(&self, x: usize, y: usize) -> bool {
        self.blocked
            .get(y)
            .and_then(|r| r.get(x))
            .copied()
            .unwrap_or(true)
    }
    /// `true` when `(x, y)` exists, is not part of a box and is still blank.
    fn is_free(&self, x: usize, y: usize) -> bool {
        !self.is_blocked(x, y) && self.cells.get(y).and_then(|r| r.get(x)) == Some(&' ')
    }
    /// Writes `c` unless the cell belongs to a box or is already used.
    fn put(&mut self, x: usize, y: usize, c: char) {
        if self.is_free(x, y) {
            if let Some(cell) = self.cells.get_mut(y).and_then(|r| r.get_mut(x)) {
                *cell = c;
            }
        }
    }
    /// Writes `c` over any previous edge glyph, but never over a box.
    fn force(&mut self, x: usize, y: usize, c: char) {
        if self.is_blocked(x, y) {
            return;
        }
        if let Some(cell) = self.cells.get_mut(y).and_then(|r| r.get_mut(x)) {
            *cell = c;
        }
    }
    /// Writes a corner glyph, merging with a corner already drawn there so
    /// that two edges meeting in the same cell become a tee.
    fn join(&mut self, x: usize, y: usize, c: char) {
        if self.is_blocked(x, y) {
            return;
        }
        let old = self
            .cells
            .get(y)
            .and_then(|r| r.get(x))
            .copied()
            .unwrap_or(' ');
        let merged = match (old, c) {
            (' ', _) => c,
            (a, b) if a == b => a,
            ('├', '┤') | ('┤', '├') | ('┴', '┬') | ('┬', '┴') | ('┼', _) => '┼',
            ('┌', '└') | ('└', '┌') | ('│', '┌') | ('│', '└') | ('├', _) => '├',
            ('┐', '┘') | ('┘', '┐') | ('│', '┐') | ('│', '┘') | ('┤', _) => '┤',
            ('└', '┘') | ('┘', '└') | ('─', '└') | ('─', '┘') | ('┴', _) => '┴',
            ('┌', '┐') | ('┐', '┌') | ('─', '┌') | ('─', '┐') | ('┬', _) => '┬',
            _ => c,
        };
        self.force(x, y, merged);
    }

    /// Box painting: writes unconditionally and marks the cell as occupied.
    fn paint(&mut self, x: usize, y: usize, c: char) {
        if let Some(cell) = self.cells.get_mut(y).and_then(|r| r.get_mut(x)) {
            *cell = c;
        }
        if let Some(b) = self.blocked.get_mut(y).and_then(|r| r.get_mut(x)) {
            *b = true;
        }
    }
    fn paint_text(&mut self, x: usize, y: usize, s: &str) {
        let mut cx = x;
        for c in s.chars() {
            self.paint(cx, y, c);
            cx += unicode::char_width(c).max(1);
        }
    }
    /// Writes `s` starting at `(x, y)` only if every cell it needs is free.
    fn text_if_free(&mut self, x: usize, y: usize, s: &str) -> bool {
        let w = display_width(s);
        if w == 0 || (0..w).any(|i| !self.is_free(x + i, y)) {
            return false;
        }
        let mut cx = x;
        for c in s.chars() {
            self.force(cx, y, c);
            cx += unicode::char_width(c).max(1);
        }
        true
    }
    fn into_lines(self) -> Vec<String> {
        self.cells
            .into_iter()
            .map(|row| {
                let mut s: String = row.into_iter().collect();
                while s.ends_with(' ') {
                    s.pop();
                }
                s
            })
            .collect()
    }
}

fn paint_box(canvas: &mut Canvas, b: &Boxed, unicode: bool) {
    let ch = box_chars(b.shape, unicode);
    let (x, y, w, h) = (b.x, b.y, b.w, b.h);
    if w < 2 || h < 2 {
        return;
    }
    for row in 0..h {
        for col in 0..w {
            canvas.paint(x + col, y + row, ' ');
        }
    }
    canvas.paint(x, y, ch.tl);
    canvas.paint(x + w - 1, y, ch.tr);
    canvas.paint(x, y + h - 1, ch.bl);
    canvas.paint(x + w - 1, y + h - 1, ch.br);
    for i in 1..w - 1 {
        canvas.paint(x + i, y, ch.top);
        canvas.paint(x + i, y + h - 1, ch.bottom);
    }
    for (i, line) in b.lines.iter().enumerate() {
        let row = y + 1 + i;
        canvas.paint(x, row, ch.left);
        canvas.paint(x + w - 1, row, ch.right);
        canvas.paint_text(x + 2, row, line);
    }
}

// ---------------------------------------------------------------------------
// Edge glyphs
// ---------------------------------------------------------------------------

struct Glyphs {
    horiz: char,
    vert: char,
    right: char,
    left: char,
    down: char,
    up: char,
    /// Corner joining the west and south arms.
    ws: char,
    /// Corner joining the west and north arms.
    wn: char,
    /// Corner joining the east and south arms.
    es: char,
    /// Corner joining the east and north arms.
    en: char,
}

fn glyphs(style: EdgeStyle, unicode: bool) -> Glyphs {
    if !unicode {
        return Glyphs {
            horiz: match style {
                EdgeStyle::Dotted => '.',
                EdgeStyle::Thick => '=',
                _ => '-',
            },
            vert: match style {
                EdgeStyle::Dotted => ':',
                _ => '|',
            },
            right: '>',
            left: '<',
            down: 'v',
            up: '^',
            ws: '+',
            wn: '+',
            es: '+',
            en: '+',
        };
    }
    Glyphs {
        horiz: match style {
            EdgeStyle::Dotted => '┄',
            EdgeStyle::Thick => '═',
            _ => '─',
        },
        vert: match style {
            EdgeStyle::Dotted => '┆',
            EdgeStyle::Thick => '║',
            _ => '│',
        },
        right: '▶',
        left: '◀',
        down: '▼',
        up: '▲',
        ws: '┐',
        wn: '┘',
        es: '┌',
        en: '└',
    }
}

// ---------------------------------------------------------------------------
// Step 4: placement and routing
// ---------------------------------------------------------------------------

const H_GAP: usize = 5;
const V_GAP: usize = 3;
const NODE_SPACING: usize = 1;
const COL_SPACING: usize = 3;
/// Maximum number of detour lanes reserved for edges spanning >1 layer.
const MAX_CHANNELS: usize = 8;

fn layout_and_paint(diagram: &Diagram, opts: &RenderOptions, max_label: usize) -> Vec<String> {
    let (layer, _back) = layering(diagram);
    let layer_count = layer.iter().copied().max().unwrap_or(0) + 1;
    let layers = ordering(diagram, &layer, layer_count);
    let mut boxes = build_boxes(diagram, max_label);

    // Reverse layer order for RL/BT so arrows keep their geometric meaning.
    let ordered_layers: Vec<Vec<usize>> = if diagram.orientation.is_reversed() {
        layers.into_iter().rev().collect()
    } else {
        layers
    };

    let mut pos_of = vec![0usize; diagram.nodes.len()];
    for (li, bucket) in ordered_layers.iter().enumerate() {
        for v in bucket {
            if let Some(p) = pos_of.get_mut(*v) {
                *p = li;
            }
        }
    }
    let horizontal = diagram.orientation.is_horizontal();

    // Gap sizes: widened where an edge between adjacent layers carries a label.
    let mut gaps = vec![if horizontal { H_GAP } else { V_GAP }; layer_count.saturating_sub(1)];
    for e in &diagram.edges {
        let (Some(&a), Some(&b)) = (pos_of.get(e.from), pos_of.get(e.to)) else {
            continue;
        };
        let (lo, hi) = (a.min(b), a.max(b));
        if hi != lo + 1 {
            continue;
        }
        if let Some(label) = &e.label {
            let need = if horizontal {
                display_width(label) + 6
            } else {
                4
            };
            if let Some(g) = gaps.get_mut(lo) {
                *g = (*g).max(need);
            }
        }
    }

    let (content_w, content_h) = if horizontal {
        place_horizontal(&ordered_layers, &mut boxes, &gaps)
    } else {
        place_vertical(&ordered_layers, &mut boxes, &gaps)
    };

    // Edges spanning more than one layer get their own detour lane so they
    // never cut through the boxes of the layers in between.
    let long: Vec<usize> = diagram
        .edges
        .iter()
        .enumerate()
        .filter(|(_, e)| {
            e.style != EdgeStyle::Invisible
                && e.from != e.to
                && pos_of
                    .get(e.from)
                    .zip(pos_of.get(e.to))
                    .is_some_and(|(a, b)| a.abs_diff(*b) >= 2)
        })
        .map(|(i, _)| i)
        .collect();
    let channels = long.len().min(MAX_CHANNELS);

    let label_room = diagram
        .edges
        .iter()
        .filter_map(|e| e.label.as_deref())
        .map(|l| display_width(l) + 3)
        .max()
        .unwrap_or(0);

    let (cw, chh) = if horizontal {
        (content_w + label_room.min(24) + 1, content_h + channels + 2)
    } else {
        (content_w + label_room + channels + 2, content_h + 1)
    };

    let mut canvas = Canvas::new(cw, chh);
    for b in &boxes {
        paint_box(&mut canvas, b, opts.unicode_box);
    }
    for (ei, e) in diagram.edges.iter().enumerate() {
        if e.style == EdgeStyle::Invisible || e.from == e.to {
            continue;
        }
        let (Some(s), Some(t)) = (boxes.get(e.from), boxes.get(e.to)) else {
            continue;
        };
        let g = glyphs(e.style, opts.unicode_box);
        let lane = long.iter().position(|i| *i == ei);
        match (horizontal, lane) {
            (true, Some(k)) if k < channels => {
                route_long_horizontal(&mut canvas, s, t, e.label.as_deref(), &g, content_h + 1 + k);
            }
            (false, Some(k)) if k < channels => {
                route_long_vertical(&mut canvas, s, t, e.label.as_deref(), &g, content_w + 1 + k);
            }
            (true, _) => route_horizontal(&mut canvas, s, t, e.label.as_deref(), e.arrow, &g),
            (false, _) => route_vertical(&mut canvas, s, t, e.label.as_deref(), e.arrow, &g),
        }
    }

    let mut lines = canvas.into_lines();
    while lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }
    if diagram.has_unsupported_features {
        for note in &diagram.unsupported_notes {
            lines.push(String::new());
            lines.push(format!("[note: {note}]"));
        }
    }
    lines
}

/// Places boxes for `LR`/`RL`; returns the content size.
fn place_horizontal(layers: &[Vec<usize>], boxes: &mut [Boxed], gaps: &[usize]) -> (usize, usize) {
    let col_w: Vec<usize> = layers
        .iter()
        .map(|b| {
            b.iter()
                .filter_map(|v| boxes.get(*v))
                .map(|b| b.w)
                .max()
                .unwrap_or(0)
        })
        .collect();
    let col_h: Vec<usize> = layers
        .iter()
        .map(|b| {
            let sum: usize = b.iter().filter_map(|v| boxes.get(*v)).map(|b| b.h).sum();
            sum + NODE_SPACING * b.len().saturating_sub(1)
        })
        .collect();
    let max_h = col_h.iter().copied().max().unwrap_or(0);

    let mut x = 0usize;
    for (li, bucket) in layers.iter().enumerate() {
        let mut y = (max_h - col_h.get(li).copied().unwrap_or(0)) / 2;
        let width = col_w.get(li).copied().unwrap_or(0);
        for v in bucket {
            if let Some(b) = boxes.get_mut(*v) {
                b.x = x + (width - b.w) / 2;
                b.y = y;
                y += b.h + NODE_SPACING;
            }
        }
        x += width;
        if li + 1 < layers.len() {
            x += gaps.get(li).copied().unwrap_or(H_GAP);
        }
    }
    (x, max_h)
}

/// Places boxes for `TD`/`BT`; returns the content size.
fn place_vertical(layers: &[Vec<usize>], boxes: &mut [Boxed], gaps: &[usize]) -> (usize, usize) {
    let row_h: Vec<usize> = layers
        .iter()
        .map(|b| {
            b.iter()
                .filter_map(|v| boxes.get(*v))
                .map(|b| b.h)
                .max()
                .unwrap_or(0)
        })
        .collect();
    let row_w: Vec<usize> = layers
        .iter()
        .map(|b| {
            let sum: usize = b.iter().filter_map(|v| boxes.get(*v)).map(|b| b.w).sum();
            sum + COL_SPACING * b.len().saturating_sub(1)
        })
        .collect();
    let max_w = row_w.iter().copied().max().unwrap_or(0);

    let mut y = 0usize;
    for (li, bucket) in layers.iter().enumerate() {
        let mut x = (max_w - row_w.get(li).copied().unwrap_or(0)) / 2;
        let height = row_h.get(li).copied().unwrap_or(0);
        for v in bucket {
            if let Some(b) = boxes.get_mut(*v) {
                b.x = x;
                b.y = y + (height - b.h) / 2;
                x += b.w + COL_SPACING;
            }
        }
        y += height;
        if li + 1 < layers.len() {
            y += gaps.get(li).copied().unwrap_or(V_GAP);
        }
    }
    (max_w, y)
}

/// Corner joining a horizontal arm coming from `dir` with a vertical arm to
/// `down`.
fn turn_from_horizontal(g: &Glyphs, dir: isize, down: bool) -> char {
    match (dir > 0, down) {
        (true, true) => g.ws,
        (true, false) => g.wn,
        (false, true) => g.es,
        (false, false) => g.en,
    }
}

/// Corner joining a vertical arm arriving from `down` with a horizontal arm
/// leaving towards `dir`.
fn turn_to_horizontal(g: &Glyphs, dir: isize, down: bool) -> char {
    match (dir > 0, down) {
        (true, true) => g.en,
        (true, false) => g.es,
        (false, true) => g.wn,
        (false, false) => g.ws,
    }
}

fn route_horizontal(
    canvas: &mut Canvas,
    s: &Boxed,
    t: &Boxed,
    label: Option<&str>,
    arrow: ArrowKind,
    g: &Glyphs,
) {
    let sy = s.y + s.h / 2;
    let ty = t.y + t.h / 2;
    let (start, end, dir): (isize, isize, isize) = if t.x >= s.x + s.w {
        ((s.x + s.w) as isize, t.x as isize - 1, 1)
    } else if s.x >= t.x + t.w {
        (s.x as isize - 1, (t.x + t.w) as isize, -1)
    } else {
        return; // overlapping columns: nothing sensible to draw
    };
    if start < 0 || end < 0 {
        return;
    }
    let free = (end - start) * dir + 1;
    if free < 2 {
        return;
    }
    let head = if dir > 0 { g.right } else { g.left };
    let tail = if dir > 0 { g.left } else { g.right };
    let arrow_x = end - dir;
    if arrow_x < 0 {
        return;
    }

    let draw_head = |canvas: &mut Canvas, x: isize, y: usize| {
        let c = match arrow {
            ArrowKind::None => g.horiz,
            _ => head,
        };
        canvas.force(x.max(0) as usize, y, c);
        if arrow == ArrowKind::DoubleArrow {
            canvas.force((start + dir).max(0) as usize, sy, tail);
        }
    };

    if sy == ty {
        let mut x = start + dir;
        while (arrow_x - x) * dir >= 0 {
            canvas.put(x.max(0) as usize, sy, g.horiz);
            x += dir;
        }
        draw_head(canvas, arrow_x, sy);
        if let Some(l) = label {
            place_h_label(canvas, l, start + dir, arrow_x - dir, sy);
        }
        return;
    }

    let mid = start + dir * (free / 2);
    let down = ty > sy;
    let mut x = start + dir;
    while (mid - x) * dir > 0 {
        canvas.put(x.max(0) as usize, sy, g.horiz);
        x += dir;
    }
    canvas.join(mid.max(0) as usize, sy, turn_from_horizontal(g, dir, down));
    let (lo, hi) = if down { (sy, ty) } else { (ty, sy) };
    for y in lo + 1..hi {
        canvas.put(mid.max(0) as usize, y, g.vert);
    }
    canvas.join(mid.max(0) as usize, ty, turn_to_horizontal(g, dir, down));
    let mut x = mid + dir;
    while (arrow_x - x) * dir > 0 {
        canvas.put(x.max(0) as usize, ty, g.horiz);
        x += dir;
    }
    draw_head(canvas, arrow_x, ty);
    if let Some(l) = label {
        place_h_label(canvas, l, mid + dir, arrow_x - dir, ty);
    }
}

/// Writes `label` centred in the inclusive cell range `[a, b]` of row `y`.
fn place_h_label(canvas: &mut Canvas, label: &str, a: isize, b: isize, y: usize) {
    let (lo, hi) = (a.min(b), a.max(b));
    if lo < 0 || hi < lo {
        return;
    }
    let room = (hi - lo + 1) as usize;
    let text = truncate(label, room);
    let w = display_width(&text);
    if w == 0 || w > room {
        return;
    }
    let x = lo as usize + (room - w) / 2;
    // Prefer the row above when the line row is not free (keeps the line intact).
    let mut cx = x;
    for c in text.chars() {
        canvas.force(cx, y, c);
        cx += unicode::char_width(c).max(1);
    }
}

/// Shorten `s` to at most `max` cells, marking the cut with an ellipsis.
fn truncate(s: &str, max: usize) -> String {
    unicode::truncate_with_ellipsis(s, max, "…")
}

fn route_vertical(
    canvas: &mut Canvas,
    s: &Boxed,
    t: &Boxed,
    label: Option<&str>,
    arrow: ArrowKind,
    g: &Glyphs,
) {
    let sx = s.x + s.w / 2;
    let tx = t.x + t.w / 2;
    let (start, end, dir): (isize, isize, isize) = if t.y >= s.y + s.h {
        ((s.y + s.h) as isize, t.y as isize - 1, 1)
    } else if s.y >= t.y + t.h {
        (s.y as isize - 1, (t.y + t.h) as isize, -1)
    } else {
        return;
    };
    if start < 0 || end < 0 {
        return;
    }
    let free = (end - start) * dir + 1;
    if free < 1 {
        return;
    }
    let head = if dir > 0 { g.down } else { g.up };
    let tail = if dir > 0 { g.up } else { g.down };
    let down = dir > 0;

    if sx == tx {
        let mut y = start;
        while (end - y) * dir > 0 {
            canvas.put(sx, y.max(0) as usize, g.vert);
            y += dir;
        }
        canvas.force(
            sx,
            end.max(0) as usize,
            if arrow == ArrowKind::None {
                g.vert
            } else {
                head
            },
        );
        if arrow == ArrowKind::DoubleArrow {
            canvas.force(sx, start.max(0) as usize, tail);
        }
        if let Some(l) = label {
            let text = truncate(l, 24);
            canvas.text_if_free(sx + 2, start.max(0) as usize, &text);
        }
        return;
    }

    let mid = start + dir * (free / 2);
    let hdir: isize = if tx > sx { 1 } else { -1 };
    let mut y = start;
    while (mid - y) * dir > 0 {
        canvas.put(sx, y.max(0) as usize, g.vert);
        y += dir;
    }
    let midr = mid.max(0) as usize;
    canvas.join(sx, midr, turn_to_horizontal(g, hdir, down));
    let (lo, hi) = if tx > sx { (sx, tx) } else { (tx, sx) };
    for x in lo + 1..hi {
        canvas.put(x, midr, g.horiz);
    }
    canvas.join(tx, midr, turn_from_horizontal(g, hdir, down));
    let mut y = mid + dir;
    while (end - y) * dir > 0 {
        canvas.put(tx, y.max(0) as usize, g.vert);
        y += dir;
    }
    canvas.force(
        tx,
        end.max(0) as usize,
        if arrow == ArrowKind::None {
            g.vert
        } else {
            head
        },
    );
    if arrow == ArrowKind::DoubleArrow {
        canvas.force(sx, start.max(0) as usize, tail);
    }
    if let Some(l) = label {
        let text = truncate(l, 24);
        let y = (mid + dir).max(0) as usize;
        if !canvas.text_if_free(tx + 2, y, &text) {
            canvas.text_if_free(hi + 2, midr, &text);
        }
    }
}

/// Routes an edge spanning more than one layer (`LR`/`RL`) through a detour
/// lane below the diagram, entering the target box from underneath.
fn route_long_horizontal(
    canvas: &mut Canvas,
    s: &Boxed,
    t: &Boxed,
    label: Option<&str>,
    g: &Glyphs,
    lane: usize,
) {
    let sy = s.y + s.h / 2;
    let to_right = t.x >= s.x + s.w;
    let dir: isize = if to_right { 1 } else { -1 };
    let x1: isize = if to_right {
        (s.x + s.w) as isize
    } else {
        s.x as isize - 1
    };
    if x1 < 0 || lane <= sy {
        return;
    }
    let x1u = x1 as usize;
    let xt = t.x + t.w / 2;
    let entry = t.y + t.h; // first row below the target box

    canvas.force(x1u, sy, turn_from_horizontal(g, dir, true));
    for y in sy + 1..lane {
        canvas.put(x1u, y, g.vert);
    }
    let hdir: isize = if xt > x1u { 1 } else { -1 };
    canvas.force(x1u, lane, turn_to_horizontal(g, hdir, true));
    let (lo, hi) = if xt > x1u { (x1u, xt) } else { (xt, x1u) };
    for x in lo + 1..hi {
        canvas.put(x, lane, g.horiz);
    }
    if xt != x1u {
        canvas.force(xt, lane, turn_from_horizontal(g, hdir, false));
    }
    for y in entry + 1..lane {
        canvas.put(xt, y, g.vert);
    }
    canvas.force(xt, entry, g.up);
    if let Some(l) = label {
        let text = truncate(l, 24);
        canvas.text_if_free(lo + 1, lane, &text);
    }
}

/// Routes an edge spanning more than one layer (`TD`/`BT`) through a detour
/// lane to the right of the diagram, entering the target box from the right.
fn route_long_vertical(
    canvas: &mut Canvas,
    s: &Boxed,
    t: &Boxed,
    label: Option<&str>,
    g: &Glyphs,
    lane: usize,
) {
    let sx = s.x + s.w / 2;
    let downward = t.y >= s.y + s.h;
    let y1: isize = if downward {
        (s.y + s.h) as isize
    } else {
        s.y as isize - 1
    };
    if y1 < 0 || lane <= sx {
        return;
    }
    let y1u = y1 as usize;
    let yt = t.y + t.h / 2;
    let entry = t.x + t.w; // first column right of the target box

    canvas.force(sx, y1u, turn_to_horizontal(g, 1, downward));
    for x in sx + 1..lane {
        canvas.put(x, y1u, g.horiz);
    }
    let vdown = yt > y1u;
    canvas.force(lane, y1u, turn_from_horizontal(g, 1, vdown));
    let (lo, hi) = if yt > y1u { (y1u, yt) } else { (yt, y1u) };
    for y in lo + 1..hi {
        canvas.put(lane, y, g.vert);
    }
    if yt != y1u {
        canvas.join(lane, yt, turn_to_horizontal(g, -1, vdown));
    }
    for x in entry + 1..lane {
        canvas.put(x, yt, g.horiz);
    }
    canvas.force(entry, yt, g.left);
    if let Some(l) = label {
        let text = truncate(l, 24);
        canvas.text_if_free(sx + 2, y1u, &text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mermaid::parser::parse;

    fn draw(src: &str, opts: &RenderOptions) -> Vec<String> {
        render(&parse(src).unwrap(), opts).unwrap()
    }

    #[test]
    fn spec_11_1_golden_output() {
        let out = draw(
            "graph LR\n    A --> B\n    B --> C\n",
            &RenderOptions::default(),
        );
        let trimmed: Vec<String> = out.iter().map(|l| l.trim_end().to_string()).collect();
        assert_eq!(
            trimmed,
            vec![
                "┌───┐     ┌───┐     ┌───┐".to_string(),
                "│ A │ ──▶ │ B │ ──▶ │ C │".to_string(),
                "└───┘     └───┘     └───┘".to_string(),
            ]
        );
    }

    #[test]
    fn all_lines_are_padded_to_equal_width() {
        let out = draw(
            "graph TD\nA[Start] --> B{Choice}\nB --> C[Yes]\nB --> D[No]\n",
            &RenderOptions::default(),
        );
        let widths: Vec<usize> = out.iter().map(|l| display_width(l)).collect();
        assert!(widths.windows(2).all(|w| w[0] == w[1]), "{widths:?}");
    }

    #[test]
    fn never_exceeds_width_cells() {
        let src = "graph LR\nA[a very long label indeed] --> B[another quite long label]\n";
        for width in [20usize, 30, 40, 80, 120] {
            let opts = RenderOptions {
                width_cells: width,
                ..RenderOptions::default()
            };
            if let Ok(out) = render(&parse(src).unwrap(), &opts) {
                assert!(
                    out.iter().all(|l| display_width(l) <= width),
                    "width {width} exceeded"
                );
            }
        }
    }

    #[test]
    fn too_narrow_reports_too_wide() {
        let src = "graph LR\nA --> B --> C --> D --> E --> F\n";
        let opts = RenderOptions {
            width_cells: 10,
            ..RenderOptions::default()
        };
        assert!(matches!(
            render(&parse(src).unwrap(), &opts),
            Err(NativeError::TooWide { .. })
        ));
    }

    #[test]
    fn ascii_fallback() {
        let opts = RenderOptions {
            unicode_box: false,
            ..RenderOptions::default()
        };
        let out = draw("graph LR\nA --> B\n", &opts);
        let joined = out.join("\n");
        assert!(joined.contains("+---+"), "{joined}");
        assert!(joined.contains("| A |"), "{joined}");
        assert!(joined.contains(">"), "{joined}");
        assert!(
            joined.is_ascii(),
            "ascii fallback produced non-ascii: {joined}"
        );
    }

    #[test]
    fn ascii_fallback_vertical() {
        let opts = RenderOptions {
            unicode_box: false,
            ..RenderOptions::default()
        };
        let out = draw("graph TD\nA --> B\n", &opts);
        let joined = out.join("\n");
        assert!(joined.contains('v'), "{joined}");
        assert!(joined.contains('|'), "{joined}");
        assert!(joined.is_ascii());
    }

    #[test]
    fn top_down_uses_vertical_connectors() {
        let out = draw("graph TD\nA --> B\n", &RenderOptions::default());
        let joined = out.join("\n");
        assert!(joined.contains('▼'), "{joined}");
    }

    #[test]
    fn right_to_left_points_left() {
        let out = draw("graph RL\nA --> B\n", &RenderOptions::default());
        let joined = out.join("\n");
        assert!(joined.contains('◀'), "{joined}");
    }

    #[test]
    fn rendering_is_deterministic() {
        let src = "graph TD\nA --> B & C\nB --> D\nC --> D\nD --> A\n";
        let a = draw(src, &RenderOptions::default());
        let b = draw(src, &RenderOptions::default());
        assert_eq!(a, b);
        // Layering/ordering must not depend on iteration order of hash maps.
        for _ in 0..20 {
            assert_eq!(draw(src, &RenderOptions::default()), a);
        }
    }

    #[test]
    fn cycles_do_not_hang() {
        let out = draw(
            "graph LR\nA --> B\nB --> C\nC --> A\n",
            &RenderOptions::default(),
        );
        assert!(!out.is_empty());
    }

    #[test]
    fn subgraph_note_is_appended() {
        let out = draw(
            "graph LR\nsubgraph s\nA --> B\nend\n",
            &RenderOptions::default(),
        );
        assert!(
            out.iter().any(|l| l.contains("[note:")),
            "expected an unsupported-feature note: {out:?}"
        );
    }

    #[test]
    fn empty_diagram_is_an_error() {
        let d = parse("graph LR\n").unwrap();
        assert_eq!(
            render(&d, &RenderOptions::default()),
            Err(NativeError::Empty)
        );
    }

    #[test]
    fn edge_labels_are_drawn() {
        let out = draw("graph LR\nA -->|yes| B\n", &RenderOptions::default());
        assert!(out.join("\n").contains("yes"), "{out:?}");
    }

    #[test]
    fn wide_characters_are_measured_correctly() {
        let out = draw("graph LR\nA[日本語] --> B\n", &RenderOptions::default());
        let widths: Vec<usize> = out.iter().map(|l| display_width(l)).collect();
        assert!(widths.windows(2).all(|w| w[0] == w[1]), "{widths:?}");
    }

    #[test]
    fn wrap_label_respects_display_width() {
        assert_eq!(wrap_label("hello world", 5), vec!["hello", "world"]);
        assert_eq!(wrap_label("abcdefgh", 3), vec!["abc", "def", "gh"]);
        assert_eq!(wrap_label("", 5), vec![""]);
    }

    #[test]
    fn shapes_render_distinct_borders() {
        let out = draw(
            "graph LR\nA{d} --> B((c))\nB --> C{{h}}\nC --> D[(y)]\nD --> E([s])\n",
            &RenderOptions {
                width_cells: 200,
                ..RenderOptions::default()
            },
        );
        let j = out.join("\n");
        assert!(j.contains('╱'), "diamond/hexagon corners missing: {j}");
        assert!(j.contains('('), "circle markers missing: {j}");
        assert!(j.contains('═'), "cylinder top missing: {j}");
        assert!(j.contains('╭'), "rounded corners missing: {j}");
    }
}
