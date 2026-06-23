//! End-to-end check that `#[otter_codegen::raw]` expands to something that
//! actually compiles, in both feature configurations. The emitted
//! `cfg(feature = "raw")` resolves against this test crate's view of
//! otter_codegen's features, so:
//!
//!   cargo test -p otter-nif-macros                  -> the plain (non-pub) copy
//!   cargo test -p otter-nif-macros --features raw   -> the `pub` copy
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

struct Wrapper(u32);

impl Wrapper {
    // A method with no visibility -> private (non-raw) / pub (raw).
    #[otter_codegen::raw]
    fn peek(&self) -> u32 {
        self.0
    }
}

#[test]
fn raw_items_compile_and_run() {
    assert_eq!(answer(), 42);
    assert_eq!(doubled(answer()), 84);
    assert_eq!(Wrapper(7).peek(), 7);
}
