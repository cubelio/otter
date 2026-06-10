use std::ffi::c_void;
use crate::sys::{NifEnv, NifMonitor, NifPid, NifTerm};
use crate::enif::funcs;

/// Monitor a process from a resource object.
///
/// Returns 0 on success, > 0 if the process is already dead, < 0 if `pid`
/// is not a local pid. On success, `mon` is populated with the monitor handle.
pub(crate) unsafe fn monitor_process(
    env: *mut NifEnv,
    obj: *mut c_void,
    pid: *const NifPid,
    mon: *mut NifMonitor,
) -> i32 {
    unsafe { (funcs().monitor_process)(env, obj, pid, mon) }
}

/// Remove a process monitor from a resource object.
///
/// Returns 0 if the monitor was found and removed, > 0 if no such monitor
/// exists (it may have already fired).
pub(crate) unsafe fn demonitor_process(
    env: *mut NifEnv,
    obj: *mut c_void,
    mon: *const NifMonitor,
) -> i32 {
    unsafe { (funcs().demonitor_process)(env, obj, mon) }
}

/// Compare two monitors. Returns 0 if equal, < 0 or > 0 otherwise.
pub(crate) fn compare_monitors(mon1: &NifMonitor, mon2: &NifMonitor) -> i32 {
    unsafe { (funcs().compare_monitors)(mon1, mon2) }
}

/// Create a term from a monitor handle.
pub(crate) unsafe fn make_monitor_term(env: *mut NifEnv, mon: &NifMonitor) -> NifTerm {
    unsafe { (funcs().make_monitor_term)(env, mon) }
}
