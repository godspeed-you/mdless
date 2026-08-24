# Fuzzing diple

The rule: *the application must never panic on arbitrary document input*.

This is a separate crate with its own workspace, so `cargo build`, `cargo test`
and `cargo clippy` in the repository root never see it.

## Targets

| Target | Entry point |
|---|---|
| `parse_markdown` | `document::parser::parse` on arbitrary UTF-8 |
| `layout` | `parse` + `Layout::build` at a fuzzed width, theme and wrap mode |
| `table` | `layout::table::layout_table` with fuzzed cells, alignments and width |
| `unicode` | `layout::unicode` width / split / pad / wrap / tab helpers |
| `config` | `config::loader::load_file` (TOML parsing and validation) |
| `mermaid` | `mermaid::parser::parse` + the native terminal renderer |

The bodies live in `src/lib.rs`; `fuzz_targets/*.rs` are thin libFuzzer shims.
Seed corpora are in `corpus/<target>/`, derived from `tests/fixtures/`.

## Coverage-guided run (needs nightly)

```bash
cargo install cargo-fuzz
rustup toolchain install nightly
cargo +nightly fuzz run parse_markdown -- -max_total_time=60
```

`+nightly` is required explicitly because the repository pins stable in
`rust-toolchain.toml`.

To keep the checked-in seed corpus clean, write new inputs elsewhere:

```bash
cargo +nightly fuzz run layout /tmp/diple-fuzz/layout fuzz/corpus/layout -- -max_total_time=60
```

## Stable-toolchain smoke run (CI)

`cargo fuzz` needs nightly and libFuzzer instrumentation, which is not
available on every builder. The `smoke` binary runs the same target bodies
over the seed corpora plus deterministic mutations, on stable:

```bash
cargo run --release --bin smoke -- 30            # 30 s per target
cargo run --release --bin smoke -- 10 table      # one target only
```

A panic aborts the process with a non-zero exit status.
