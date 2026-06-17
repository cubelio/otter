//! Raw C ABI types mirroring `erl_nif.h`.
//!
//! Direct Rust transcriptions of the types defined in `erl_nif.h`. No logic,
//! no safety wrappers — only type definitions and constants. All struct types
//! are `#[repr(C)]` to match the C ABI exactly.
//!
//! Naming convention: `Erl` prefix dropped, `Nif` prefix retained for the
//! items still defined here. Types already migrated to `enif-ffi` are
//! referenced as `enif_ffi::*` (and a few re-exported under their legacy
//! `Nif*` names while consumers catch up).

use std::ffi::c_int;

// ---------------------------------------------------------------------------
// Library entry point descriptor
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// OS event handle (for enif_select)
// ---------------------------------------------------------------------------

/// `ErlNifEvent` — re-export of [`enif_ffi::Event`] under otter's legacy name,
/// kept while consumers still reference `otter::sys::NifEvent`. NIF 2.12.
pub use enif_ffi::Event as NifEvent;

// ---------------------------------------------------------------------------
// Time
// ---------------------------------------------------------------------------

/// `ERL_NIF_TIME_ERROR` — sentinel returned by time functions on error.
/// NIF 2.10 (OTP 18.3).
pub const NIF_TIME_ERROR: enif_ffi::Time = i64::MIN;

// ---------------------------------------------------------------------------
// Select (I/O event multiplexing)
// ---------------------------------------------------------------------------

/// `ErlNifSelectFlags` — re-export of [`enif_ffi::SelectFlags`] under otter's
/// legacy name, kept while consumers still reference `otter::sys::NifSelectFlags`.
/// NIF 2.12 (OTP 20.0).
pub use enif_ffi::SelectFlags as NifSelectFlags;

// ---------------------------------------------------------------------------
// Schedule NIF flags
// ---------------------------------------------------------------------------

/// Flags for `enif_schedule_nif`: run on a normal scheduler. NIF 2.7 (OTP 17.3).
pub const NIF_DIRTY_JOB_NORMAL:    c_int = 0;
/// Flags for `enif_schedule_nif`: run on a dirty CPU scheduler. NIF 2.7 (OTP 17.3).
pub const NIF_DIRTY_JOB_CPU_BOUND: c_int = 1;
/// Flags for `enif_schedule_nif`: run on a dirty I/O scheduler. NIF 2.7 (OTP 17.3).
pub const NIF_DIRTY_JOB_IO_BOUND:  c_int = 2;

// ---------------------------------------------------------------------------
// I/O queue and iovec
// ---------------------------------------------------------------------------

/// Normal I/O queue mode. NIF 2.13 (OTP 20.1).
pub const NIF_IOQ_NORMAL: enif_ffi::IOQueueOpts = 1;


