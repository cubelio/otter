use std::ffi::c_void;
use crate::sys::{NifEnv, NifEvent, NifPid, NifSelectFlags, NifTerm};
use crate::enif::funcs;

/// Register interest in I/O events on an OS file descriptor / event handle.
///
/// Returns a bitmask of `SELECT_*` result flags.
pub(crate) unsafe fn select(
    env: *mut NifEnv,
    event: NifEvent,
    flags: NifSelectFlags,
    obj: *mut c_void,
    pid: *const NifPid,
    ref_term: NifTerm,
) -> i32 {
    unsafe { (funcs().select)(env, event, flags, obj, pid, ref_term) }
}

/// Register interest in I/O events with a custom message.
///
/// Returns a bitmask of `SELECT_*` result flags.
pub(crate) unsafe fn select_x(
    env: *mut NifEnv,
    event: NifEvent,
    flags: NifSelectFlags,
    obj: *mut c_void,
    pid: *const NifPid,
    msg: NifTerm,
    msg_env: *mut NifEnv,
) -> i32 {
    unsafe { (funcs().select_x)(env, event, flags, obj, pid, msg, msg_env) }
}
