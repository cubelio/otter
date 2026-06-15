//! trybuild tests that lock in the diagnostics produced by `#[otter::nif]`.
//!
//! Each `.rs` file in `tests/ui/` compiled here is expected to fail to
//! compile. The accompanying `.stderr` file is the locked-in error output;
//! a regression in the macro's diagnostic quality changes one of those
//! messages and trybuild reports the diff.
//!
//! To refresh the `.stderr` files after an intentional change, run:
//!
//!     TRYBUILD=overwrite cargo test --test codegen_ui
//!
//! Then review the diff and commit.

#[test]
fn codegen_ui() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/fail_missing_env.rs");
    t.compile_fail("tests/ui/fail_return_not_encoder.rs");
    t.compile_fail("tests/ui/fail_init_duplicate_key.rs");
    t.compile_fail("tests/ui/fail_init_unknown_key.rs");

    // The `_raw` keys are gated on otter's `raw` feature. Without it the macro
    // rejects them; with it, it accepts them and the mutual-exclusion check is
    // what fires for `load` + `load_raw`. Run `cargo test -p otter --features raw`
    // to exercise the second arm.
    #[cfg(not(feature = "raw"))]
    t.compile_fail("tests/ui/fail_init_raw_without_feature.rs");
    #[cfg(feature = "raw")]
    {
        t.pass("tests/ui/pass_init_raw.rs");
        t.compile_fail("tests/ui/fail_init_mutual_exclusion.rs");
    }
}
