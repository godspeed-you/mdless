#![no_main]
//! Fuzz target `layout` — see `fuzz/src/lib.rs` for the body.

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    diple_fuzz::layout(data);
});
