//! I/O event multiplexing.
//!
//! Wraps `enif_select` and `enif_select_x` for asynchronous I/O on file
//! descriptors (Unix) or event handles (Windows).

use crate::env::Env;
use crate::resource::{Resource, ResourceArc};
use crate::term::AsNifTerm;
use crate::types::LocalPid;

// `select`/`select_x` return a raw `i32` bitmask of result flags. otter does not
// yet wrap that in a typed result, so callers decode it against the raw
// `enif_ffi::SELECT_*` constants (`SELECT_STOP_CALLED`, `SELECT_NOTSUP`, …). A
// proper typed surface is tracked as issue enhance-12.

/// Register interest in I/O events on an OS-level event handle.
///
/// When the event becomes ready, the BEAM sends a message to `pid`.
/// `obj` is the resource object associated with this event (its `stop`
/// callback will be invoked on cleanup). `ref_term` is included in the
/// notification message.
///
/// Returns a raw `i32` bitmask of `enif_ffi::SELECT_*` result flags.
///
/// Wraps `enif_select`.
pub fn select<'a, T: Resource>(
    env: Env<'a>,
    event: enif_ffi::Event,
    flags: enif_ffi::SelectFlags,
    obj: &ResourceArc<T>,
    pid: &LocalPid,
    ref_term: impl AsNifTerm<'a>,
) -> i32 {
    unsafe {
        enif_ffi::select(
            env.as_ptr(),
            event,
            flags,
            obj.raw_ptr(),
            &pid.pid,
            ref_term.as_nif_term(),
        )
    }
}

/// Register interest in I/O events with a custom message.
///
/// Like [`select`] but sends `msg` (built in `msg_env`) instead of
/// the standard `{select, ...}` tuple.
///
/// Wraps `enif_select_x`.
pub fn select_x<'a, T: Resource>(
    env: Env<'a>,
    event: enif_ffi::Event,
    flags: enif_ffi::SelectFlags,
    obj: &ResourceArc<T>,
    pid: &LocalPid,
    msg: impl AsNifTerm<'a>,
    msg_env: Option<Env<'_>>,
) -> i32 {
    let msg_env_ptr = msg_env.map(|e| e.as_ptr()).unwrap_or(std::ptr::null_mut());
    unsafe {
        enif_ffi::select_x(
            env.as_ptr(),
            event,
            flags,
            obj.raw_ptr(),
            &pid.pid,
            msg.as_nif_term(),
            msg_env_ptr,
        )
    }
}
