use crate::sys::{NifEnv, NifTerm};
use crate::enif::funcs;

/// Raise a `badarg` error in the calling process.
/// The returned term must be returned from the NIF function.
pub(crate) unsafe fn make_badarg(env: *mut NifEnv) -> NifTerm {
    unsafe { (funcs().make_badarg)(env) }
}

/// Raise an exception with an arbitrary reason term.
/// The returned term must be returned from the NIF function.
pub(crate) unsafe fn raise_exception(env: *mut NifEnv, reason: NifTerm) -> NifTerm {
    unsafe { (funcs().raise_exception)(env, reason) }
}
