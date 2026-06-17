//! BEAM system information and thread introspection.

use std::ffi::c_int;

pub use enif_ffi::SysInfo;

/// The type of thread the current code is running on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ThreadType {
    /// A normal BEAM scheduler thread.
    Scheduler,
    /// A dirty CPU scheduler thread.
    DirtyCpu,
    /// A dirty I/O scheduler thread.
    DirtyIo,
    /// A non-scheduler thread (e.g. created by the user).
    NonScheduler,
    /// Unknown thread type (returned -1 from the BEAM).
    Unknown(c_int),
}

/// Return the type of the current thread.
///
/// Wraps `enif_thread_type`.
pub fn thread_type() -> ThreadType {
    match unsafe { crate::enif::thread_type() } {
        enif_ffi::THR_UNDEFINED           => ThreadType::NonScheduler,
        enif_ffi::THR_NORMAL_SCHEDULER    => ThreadType::Scheduler,
        enif_ffi::THR_DIRTY_CPU_SCHEDULER => ThreadType::DirtyCpu,
        enif_ffi::THR_DIRTY_IO_SCHEDULER  => ThreadType::DirtyIo,
        other                             => ThreadType::Unknown(other),
    }
}

/// Fill a `SysInfo` struct with BEAM system information.
///
/// Wraps `enif_system_info`.
pub fn system_info(info: &mut SysInfo) {
    unsafe { crate::enif::system_info(info, std::mem::size_of::<SysInfo>()) };
}
