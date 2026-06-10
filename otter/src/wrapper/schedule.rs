use std::ffi::{c_char, c_int};
use crate::sys::{NifEnv, NifTerm};
use crate::enif::funcs;

/// Reschedule the NIF to run `fp` with the given arguments.
///
/// `flags` is one of `NIF_DIRTY_JOB_NORMAL`, `NIF_DIRTY_JOB_CPU_BOUND`,
/// or `NIF_DIRTY_JOB_IO_BOUND`.
pub(crate) unsafe fn schedule_nif(
    env: *mut NifEnv,
    fun_name: *const c_char,
    flags: c_int,
    fp: unsafe extern "C" fn(*mut NifEnv, c_int, *const NifTerm) -> NifTerm,
    argc: c_int,
    argv: *const NifTerm,
) -> NifTerm {
    unsafe { (funcs().schedule_nif)(env, fun_name, flags, fp, argc, argv) }
}
