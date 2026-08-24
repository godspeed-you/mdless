#![no_main]
//! Fuzz target `unicode` — see `fuzz/src/lib.rs` for the body.

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    diple_fuzz::unicode_helpers(data);
});
