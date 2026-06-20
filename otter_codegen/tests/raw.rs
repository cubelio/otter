//! End-to-end check that `#[otter_codegen::raw]` expands to something that
//! actually compiles, in both feature configurations. The emitted
//! `cfg(feature = "raw")` resolves against this test crate's view of
//! otter_codegen's features, so:
//!
//!   cargo test -p otter_codegen                  -> the plain (non-pub) copy
//!   cargo test -p otter_codegen --features raw   -> the `pub` copy
//!
//! Exactly one copy is active in each config; both must compile and the item
//! must be usable.

// No visibility written -> private (non-raw) / pub (raw).
#[otter_codegen::raw]
fn answer() -> u32 {
    42
}

// A different visibility -> pub(crate) (non-raw) / pub (raw).
#[otter_codegen::raw]
pub(crate) fn doubled(x: u32) -> u32 {
    x * 2
}

#[test]
fn raw_items_compile_and_run() {
    assert_eq!(answer(), 42);
    assert_eq!(doubled(answer()), 84);
}
