#![no_main]
//! Fuzz target `config` — see `fuzz/src/lib.rs` for the body.

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    mdless_fuzz::config(data);
});
