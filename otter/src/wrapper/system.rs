use std::ffi::{c_int, c_void};
use crate::sys::{NifEnv, NifOption, NifSysInfo};
use crate::enif::funcs;

/// Fill `info` with BEAM system information.
pub(crate) fn system_info(info: &mut NifSysInfo) {
    unsafe {
        (funcs().system_info)(info, std::mem::size_of::<NifSysInfo>());
    }
}

/// Return the current thread type.
/// - 0 = non-scheduler thread (e.g. created by the user)
/// - 1 = normal scheduler
/// - 2 = dirty CPU scheduler
/// - 3 = dirty I/O scheduler
/// - -1 = undefined (thread not managed by ERTS)
pub(crate) fn thread_type() -> c_int {
    unsafe { (funcs().thread_type)() }
}

/// Enable the delay-halt option. Returns `true` on success.
///
/// `ERL_NIF_OPT_DELAY_HALT` takes no third argument — it is a boolean enable,
/// so the transmuted signature is two-arg.
pub(crate) unsafe fn set_option_delay_halt(env: *mut NifEnv) -> bool {
    type F = unsafe extern "C" fn(*mut NifEnv, NifOption) -> c_int;
    let f: F = unsafe { std::mem::transmute(funcs().set_option) };
    unsafe { f(env, NifOption::DelayHalt) == 0 }
}

/// Set the on-halt callback. Returns `true` on success.
pub(crate) unsafe fn set_option_on_halt(
    env: *mut NifEnv,
    callback: unsafe extern "C" fn(*mut c_void),
) -> bool {
    type F = unsafe extern "C" fn(
        *mut NifEnv,
        NifOption,
        unsafe extern "C" fn(*mut c_void),
    ) -> c_int;
    let f: F = unsafe { std::mem::transmute(funcs().set_option) };
    unsafe { f(env, NifOption::OnHalt, callback) == 0 }
}

/// Set the on-unload-thread callback. Returns `true` on success.
pub(crate) unsafe fn set_option_on_unload_thread(
    env: *mut NifEnv,
    callback: unsafe extern "C" fn(*mut c_void),
) -> bool {
    type F = unsafe extern "C" fn(
        *mut NifEnv,
        NifOption,
        unsafe extern "C" fn(*mut c_void),
    ) -> c_int;
    let f: F = unsafe { std::mem::transmute(funcs().set_option) };
    unsafe { f(env, NifOption::OnUnloadThread, callback) == 0 }
}
