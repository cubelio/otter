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

use std::ffi::{c_int, c_uint};

// ---------------------------------------------------------------------------
// Version constants
// ---------------------------------------------------------------------------

/// NIF 0.1 (OTP R13B03).
pub const NIF_MAJOR_VERSION: c_int = 2;
/// NIF 0.1 (OTP R13B03).
#[cfg(not(feature = "nif_2_18"))]
pub const NIF_MINOR_VERSION: c_int = 17;
/// NIF 0.1 (OTP R13B03).
#[cfg(feature = "nif_2_18")]
pub const NIF_MINOR_VERSION: c_int = 18;
/// NIF 2.1 (OTP R14B02).
pub const NIF_VM_VARIANT: &std::ffi::CStr = c"beam.vanilla";
/// NIF 2.14 (OTP 21.0).
pub const NIF_MIN_ERTS_VERSION: &std::ffi::CStr = c"erts-14.0";

// ---------------------------------------------------------------------------
// Core term type
// ---------------------------------------------------------------------------


// ---------------------------------------------------------------------------
// Function descriptor
// ---------------------------------------------------------------------------

/// `enif_ffi::Func.flags` value: run on dirty CPU scheduler. NIF 2.7 (OTP 17.3).
pub const NIF_FUNC_DIRTY_CPU: c_uint = 1;
/// `enif_ffi::Func.flags` value: run on dirty I/O scheduler. NIF 2.7 (OTP 17.3).
pub const NIF_FUNC_DIRTY_IO: c_uint = 2;

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

/// Return bits from `enif_select`. NIF 2.12 (OTP 20.0).
pub const NIF_SELECT_STOP_CALLED:     c_int = 1 << 0;
/// NIF 2.12 (OTP 20.0).
pub const NIF_SELECT_STOP_SCHEDULED:  c_int = 1 << 1;
/// NIF 2.12 (OTP 20.0).
pub const NIF_SELECT_INVALID_EVENT:   c_int = 1 << 2;
/// NIF 2.12 (OTP 20.0).
pub const NIF_SELECT_FAILED:          c_int = 1 << 3;
/// NIF 2.15 (OTP 22.0).
pub const NIF_SELECT_READ_CANCELLED:  c_int = 1 << 4;
/// NIF 2.15 (OTP 22.0).
pub const NIF_SELECT_WRITE_CANCELLED: c_int = 1 << 5;
/// NIF 2.16 (OTP 24.0).
pub const NIF_SELECT_ERROR_CANCELLED: c_int = 1 << 6;
/// NIF 2.16 (OTP 24.0).
pub const NIF_SELECT_NOTSUP:          c_int = 1 << 7;

// ---------------------------------------------------------------------------
// binary_to_term options
// ---------------------------------------------------------------------------

/// Safe decoding for `enif_binary_to_term`: reject encoded atoms that don't
/// already exist. NIF 2.11 (OTP 19.0).
pub const NIF_BIN2TERM_SAFE: c_uint = 0x20000000;

// ---------------------------------------------------------------------------
// Thread type (return values from enif_thread_type)
// ---------------------------------------------------------------------------

/// Not a scheduler thread. NIF 2.11 (OTP 19.0).
pub const NIF_THR_UNDEFINED:          c_int = 0;
/// Normal BEAM scheduler thread. NIF 2.11 (OTP 19.0).
pub const NIF_THR_NORMAL_SCHEDULER:   c_int = 1;
/// Dirty CPU scheduler thread. NIF 2.11 (OTP 19.0).
pub const NIF_THR_DIRTY_CPU_SCHEDULER: c_int = 2;
/// Dirty I/O scheduler thread. NIF 2.11 (OTP 19.0).
pub const NIF_THR_DIRTY_IO_SCHEDULER: c_int = 3;

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


