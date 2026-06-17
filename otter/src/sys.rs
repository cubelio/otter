//! Legacy `Nif*` aliases for the raw C ABI, now provided by `enif-ffi`.
//!
//! Every raw `erl_nif.h` type and constant otter once defined here has migrated
//! to the [`enif-ffi`](enif_ffi) crate and is referenced throughout otter as
//! `enif_ffi::*`. What remains are a couple of re-exports under their historical
//! `Nif*` names, kept only while external consumers (and the in-tree demo) still
//! reference `otter::sys::Nif*`. New code should use the `enif_ffi::*` names
//! directly.

/// `ErlNifEvent` — re-export of [`enif_ffi::Event`] under otter's legacy name,
/// kept while consumers still reference `otter::sys::NifEvent`. NIF 2.12.
pub use enif_ffi::Event as NifEvent;

/// `ErlNifSelectFlags` — re-export of [`enif_ffi::SelectFlags`] under otter's
/// legacy name, kept while consumers still reference `otter::sys::NifSelectFlags`.
/// NIF 2.12 (OTP 20.0).
pub use enif_ffi::SelectFlags as NifSelectFlags;
