//! I/O event multiplexing.
//!
//! Wraps `enif_select` and `enif_select_x` for asynchronous I/O on file
//! descriptors (Unix) or event handles (Windows).

use crate::resource::{Resource, ResourceArc};
use crate::types::{Env, LocalPid, Term};

// `select`/`select_x` return a raw `i32` bitmask of result flags. otter does not
// yet wrap that in a typed result, so callers decode it against the raw
// `enif_ffi::SELECT_*` constants. A typed surface is tracked as enhance-12.

/// Register interest in I/O events on an OS-level event handle (`enif_select`).
///
/// When the event becomes ready, the BEAM sends a message to `pid`. `obj` is the
/// resource associated with this event (its `stop` callback runs on cleanup).
/// `ref_term` is included in the notification message. Returns a raw `i32`
/// bitmask of `enif_ffi::SELECT_*` result flags.
pub fn select<'id, T: Resource>(
    env: impl Env<'id>,
    event: enif_ffi::Event,
    flags: enif_ffi::SelectFlags,
    obj: &ResourceArc<T>,
    pid: &LocalPid,
    ref_term: impl Term<'id>,
) -> i32 {
    unsafe {
        enif_ffi::select(
            env.raw_env(),
            event,
            flags,
            obj.raw_ptr(),
            &pid.pid,
            ref_term.raw_term(),
        )
    }
}

/// Like [`select`] but sends `msg` (built in `msg_env`) instead of the standard
/// `{select, ...}` tuple (`enif_select_x`). `msg_env` is the process-independent
/// env `msg` was built in, or `None` to copy from the caller env.
pub fn select_x<'id, 'm, T: Resource, M: Env<'m>>(
    env: impl Env<'id>,
    event: enif_ffi::Event,
    flags: enif_ffi::SelectFlags,
    obj: &ResourceArc<T>,
    pid: &LocalPid,
    msg: impl Term<'m>,
    msg_env: Option<M>,
) -> i32 {
    let msg_env_ptr = msg_env.map(|e| e.raw_env()).unwrap_or(std::ptr::null_mut());
    unsafe {
        enif_ffi::select_x(
            env.raw_env(),
            event,
            flags,
            obj.raw_ptr(),
            &pid.pid,
            msg.raw_term(),
            msg_env_ptr,
        )
    }
}
