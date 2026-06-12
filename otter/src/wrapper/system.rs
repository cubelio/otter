use std::ffi::c_int;
use crate::sys::NifSysInfo;
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
