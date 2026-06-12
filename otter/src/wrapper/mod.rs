//! Thin unsafe shims over `enif_*` functions.
//!
//! The raw function pointer table lives in [`crate::enif`].  This module
//! provides Rust-idiomatic wrappers (returning `Option`, managing buffers,
//! etc.) that the rest of the crate calls.
//!
//! All public items in this module and its submodules are `pub(crate)`.
//! Nothing here is part of the public otter API.

pub(crate) mod env;
pub(crate) mod port;
pub(crate) mod select;
pub(crate) mod term;
