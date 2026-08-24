//! Large-document benchmarks.
//!
//! The document is synthesised from the checked-in fixtures, so it is
//! deterministic but still contains the constructs that cost real work:
//! tables, fenced code, nested lists, footnotes and CJK/emoji text.
//!
//! Sizes are kept at ~1 MB for the layout benchmarks and ~2 MB for parsing and
//! search so that the whole file still finishes in a couple of minutes in CI.
//!
//! Note that the synthetic document repeats the same headings thousands of
//! times, which is the worst case for anchor de-duplication; see the note in
//! `bench_parse_large`.

mod common;

use std::hint::black_box;

use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use mdless::document::{parse, FoldState, SearchIndex};
use mdless::layout::{Layout, LayoutOptions};
use mdless::render::theme::Theme;

const MB: usize = 1024 * 1024;

/// Parsing a multi-megabyte document.
///
/// This is deliberately a pessimistic input: `common::large` repeats the same
/// fixtures, so thousands of headings share a slug and
/// `document::anchors::AnchorIndex::insert` probes `slug`, `slug-1`, `slug-2`,
/// … linearly for each of them — quadratic in the number of duplicates. A
/// regression here is therefore as likely to be an anchor-indexing change as a
/// parser change.
fn bench_parse_large(c: &mut Criterion) {
    let src = common::large(2 * MB);
    let mut group = c.benchmark_group("large");
    group.throughput(Throughput::Bytes(src.len() as u64));
    group.sample_size(10);
    group.bench_function("parse/2MB", |b| {
        b.iter(|| black_box(parse(black_box(&src))));
    });
    group.finish();
}

fn bench_layout_large(c: &mut Criterion) {
    let doc = parse(&common::large(MB));
    let theme = Theme::dark();
    let mut group = c.benchmark_group("large");
    group.sample_size(10);
    group.bench_function("layout/1MB@80", |b| {
        b.iter(|| black_box(Layout::build(&doc, &LayoutOptions::new(80, &theme))));
    });

    let layout = Layout::new();
    let _ = layout.layout(&doc, &LayoutOptions::new(80, &theme));
    group.bench_function("relayout/1MB@120", |b| {
        b.iter(|| black_box(layout.layout(&doc, &LayoutOptions::new(120, &theme))));
    });
    group.finish();
}

fn bench_search_large(c: &mut Criterion) {
    let doc = parse(&common::large(2 * MB));
    let index = SearchIndex::build(&doc);
    let mut group = c.benchmark_group("large");
    group.sample_size(10);
    group.bench_function("search_index/2MB", |b| {
        b.iter(|| black_box(SearchIndex::build(black_box(&doc))));
    });
    // "Chapter" hits once per repetition, "qwertzuiop" never: the hit and the
    // miss have very different costs and both matter for the search UI.
    group.bench_function("search_find/hit", |b| {
        b.iter(|| black_box(index.find(black_box("Chapter"), false)));
    });
    group.bench_function("search_find/miss", |b| {
        b.iter(|| black_box(index.find(black_box("qwertzuiop"), false)));
    });
    group.finish();
}

/// The three structural operations on a large document.
///
/// * `fold` collapses and expands one section. It splices the affected node
///   range into the cached tree instead of rebuilding the document, so the
///   cost is the index fix-up, not the layout.
/// * `search_paint` marks and unmarks the query over one viewport, which is
///   all an incremental search costs now that the query is not a layout input.
/// * `resize` is the honest full rebuild: widths really do change, so the
///   whole document is laid out again. It is the one operation on a
///   multi-megabyte document that cannot be done inside a frame.
fn bench_structural_large(c: &mut Criterion) {
    let doc = parse(&common::large(MB));
    let theme = Theme::dark();
    let engine = Layout::new();
    let folds = FoldState::new(&doc);
    let mut opts = LayoutOptions::new(80, &theme);
    opts.folds = Some(&folds);
    opts.lazy_code = true;

    let mut group = c.benchmark_group("large");
    group.sample_size(10);

    // The middle section of the document, so the splice has to move the
    // indices of half a million lines.
    let section = doc.sections.len() / 2;
    if let Some(s) = doc.sections.get(section) {
        let first = doc.nodes.partition_point(|n| n.id < s.heading);
        let last = doc.nodes.partition_point(|n| n.id < s.end);
        let mut collapsed = FoldState::new(&doc);
        collapsed.collapse(section);
        let mut folded = LayoutOptions::new(80, &theme);
        folded.folds = Some(&collapsed);
        folded.lazy_code = true;
        group.bench_function("fold/1MB", |b| {
            b.iter_batched(
                || engine.layout(&doc, &opts),
                |mut tree| {
                    engine.relayout_nodes(&doc, &folded, &mut tree, first, last - first);
                    engine.relayout_nodes(&doc, &opts, &mut tree, first, last - first);
                    black_box(tree)
                },
                criterion::BatchSize::LargeInput,
            );
        });
    }

    let mut tree = engine.layout(&doc, &opts);
    group.bench_function("search_paint/1MB@80x40", |b| {
        b.iter(|| {
            tree.mark_search(0, 40, black_box("widget"), false);
            tree.clear_search(0, 40);
        });
    });

    group.bench_function("resize/1MB@80->120", |b| {
        b.iter(|| {
            let mut wide = LayoutOptions::new(120, &theme);
            wide.folds = Some(&folds);
            wide.lazy_code = true;
            black_box(engine.layout(&doc, &wide))
        });
    });
    group.finish();
}

criterion_group!(
    large,
    bench_parse_large,
    bench_layout_large,
    bench_structural_large,
    bench_search_large
);
criterion_main!(large);
