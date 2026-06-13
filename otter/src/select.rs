//! I/O event multiplexing.
//!
//! Wraps `enif_select` and `enif_select_x` for asynchronous I/O on file
//! descriptors (Unix) or event handles (Windows).

use crate::env::Env;
use crate::resource::{Resource, ResourceArc};
use crate::sys::{NifEvent, NifPid, NifSelectFlags};
use crate::term::TypedTerm;
use crate::types::Pid;

pub use crate::sys::{
    NIF_SELECT_STOP_CALLED, NIF_SELECT_STOP_SCHEDULED, NIF_SELECT_INVALID_EVENT,
    NIF_SELECT_FAILED, NIF_SELECT_READ_CANCELLED, NIF_SELECT_WRITE_CANCELLED,
    NIF_SELECT_ERROR_CANCELLED, NIF_SELECT_NOTSUP,
};

/// Register interest in I/O events on an OS-level event handle.
///
/// When the event becomes ready, the BEAM sends a message to `pid`.
/// `obj` is the resource object associated with this event (its `stop`
/// callback will be invoked on cleanup). `ref_term` is included in the
/// notification message.
///
/// Returns a bitmask of `SELECT_*` result flags.
///
/// Wraps `enif_select`.
pub fn select<T: Resource>(
    env: Env<'_>,
    event: NifEvent,
    flags: NifSelectFlags,
    obj: &ResourceArc<T>,
    pid: &Pid,
    ref_term: TypedTerm<'_>,
) -> i32 {
    let nif_pid = NifPid { pid: pid.term };
    unsafe {
        crate::enif::select(
            env.as_ptr(),
            event,
            flags,
            obj.raw_ptr(),
            &nif_pid,
            ref_term.as_raw(),
        )
    }
}

/// Register interest in I/O events with a custom message.
///
/// Like [`select`] but sends `msg` (built in `msg_env`) instead of
/// the standard `{select, ...}` tuple.
///
/// Wraps `enif_select_x`.
pub fn select_x<T: Resource>(
    env: Env<'_>,
    event: NifEvent,
    flags: NifSelectFlags,
    obj: &ResourceArc<T>,
    pid: &Pid,
    msg: TypedTerm<'_>,
    msg_env: Option<Env<'_>>,
) -> i32 {
    let nif_pid = NifPid { pid: pid.term };
    let msg_env_ptr = msg_env.map(|e| e.as_ptr()).unwrap_or(std::ptr::null_mut());
    unsafe {
        crate::enif::select_x(
            env.as_ptr(),
            event,
            flags,
            obj.raw_ptr(),
            &nif_pid,
            msg.as_raw(),
            msg_env_ptr,
        )
    }
}
