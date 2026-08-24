#![no_main]
//! Fuzz target `table` — see `fuzz/src/lib.rs` for the body.

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    diple_fuzz::table(data);
});
