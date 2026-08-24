//! Startup-path benchmarks.
//!
//! These three measurements together account for everything diple does
//! between `main` and the first frame for a typical README:
//!
//! * `parse/readme` — Markdown → semantic [`Document`].
//! * `parse_layout/readme@80` — parse *and* lay out at width 80, i.e. the cold
//!   path including the first syntax-highlighting of every fenced block.
//! * `relayout/readme@{80,120}` — the warm path taken after a resize, where
//!   the highlighting cache is already populated (resize).
//!
//! * `first_frame/readme@80x24` — what the interactive path actually does
//!   before it can draw: parse, lay out with deferred highlighting, then
//!   highlight only the code blocks inside the first screen.
//!
//! One caveat, and it is the important one: syntect compiles a syntax
//! definition's regexes lazily, once per *process*, and that single compile is
//! what dominates a cold start — 44 ms for the `bash` syntax on the reference
//! machine, against ~1 ms for loading the syntax and theme dumps and ~0.6 ms
//! for laying a README out. Criterion's warm-up absorbs it entirely, so every
//! number in this file — `first_frame` included — is a steady-state number and
//! **not** a time to first frame. The startup budget must be measured out of
//! process, one `diple --debug` run per sample, on a pty; these benchmarks
//! only guard against the layout work itself regressing.

mod common;

use std::hint::black_box;

use criterion::{criterion_group, criterion_main, Criterion};
use diple::document::parse;
use diple::layout::{Layout, LayoutOptions};
use diple::render::theme::Theme;

fn bench_parse(c: &mut Criterion) {
    let src = common::readme();
    c.bench_function("parse/readme", |b| {
        b.iter(|| black_box(parse(black_box(&src))));
    });
}

fn bench_parse_and_layout(c: &mut Criterion) {
    let src = common::readme();
    let theme = Theme::dark();
    c.bench_function("parse_layout/readme@80", |b| {
        b.iter(|| {
            let doc = parse(black_box(&src));
            let opts = LayoutOptions::new(80, &theme);
            black_box(Layout::build(&doc, &opts))
        });
    });
}

fn bench_relayout(c: &mut Criterion) {
    let doc = parse(&common::readme());
    let theme = Theme::dark();
    // One warm-up layout populates the highlighting cache; the measured
    // iterations then only re-wrap, which is what a resize costs.
    let layout = Layout::new();
    let _ = layout.layout(&doc, &LayoutOptions::new(80, &theme));

    let mut group = c.benchmark_group("relayout");
    for width in [80usize, 120] {
        group.bench_function(format!("readme@{width}"), |b| {
            b.iter(|| {
                let opts = LayoutOptions::new(width, &theme);
                black_box(layout.layout(&doc, &opts))
            });
        });
    }
    group.finish();
}

/// The cold path of the interactive pager: parse, lay out without
/// highlighting anything, then realize just the first screen.
///
/// This is the work the `p50 < 30 ms` budget is spent on, minus the one-off
/// per-language regex compile that criterion cannot see (see the module docs).
fn bench_first_frame(c: &mut Criterion) {
    let src = common::readme();
    let theme = Theme::dark();
    const SCREEN: usize = 24;
    c.bench_function("first_frame/readme@80x24", |b| {
        b.iter(|| {
            let doc = parse(black_box(&src));
            let mut opts = LayoutOptions::new(80, &theme);
            opts.lazy_code = true;
            // A fresh engine per iteration: an empty highlighting cache is
            // what a new process starts with.
            let engine = Layout::new();
            let mut tree = engine.layout(&doc, &opts);
            engine.realize(&doc, &opts, &mut tree, 0, SCREEN);
            black_box(tree)
        });
    });
}

criterion_group!(
    startup,
    bench_parse,
    bench_parse_and_layout,
    bench_first_frame,
    bench_relayout
);
criterion_main!(startup);
