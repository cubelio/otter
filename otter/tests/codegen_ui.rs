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
}
