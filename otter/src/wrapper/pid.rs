use crate::sys::{NifEnv, NifPid, NifTerm};
use crate::enif::funcs;

/// Write the calling process's pid into `out`. Returns a pointer to `out`.
pub(crate) unsafe fn self_pid(env: *mut NifEnv, out: &mut NifPid) -> *mut NifPid {
    unsafe { (funcs().self_pid)(env, out) }
}

/// Decode a pid term. Returns `false` if `term` is not a local pid.
pub(crate) unsafe fn get_local_pid(
    env: *mut NifEnv,
    term: NifTerm,
    out: &mut NifPid,
) -> bool {
    unsafe { (funcs().get_local_pid)(env, term, out) != 0 }
}

/// Check if a process is alive. Returns `true` if the process exists.
pub(crate) unsafe fn is_process_alive(env: *mut NifEnv, pid: &mut NifPid) -> bool {
    unsafe { (funcs().is_process_alive)(env, pid) != 0 }
}

/// Check if the calling process is alive. Returns `true` if still alive.
pub(crate) unsafe fn is_current_process_alive(env: *mut NifEnv) -> bool {
    unsafe { (funcs().is_current_process_alive)(env) != 0 }
}

/// Look up a process by registered name. Returns `false` if no process
/// is registered with that name.
pub(crate) unsafe fn whereis_pid(
    env: *mut NifEnv,
    name: NifTerm,
    out: &mut NifPid,
) -> bool {
    unsafe { (funcs().whereis_pid)(env, name, out) != 0 }
}
