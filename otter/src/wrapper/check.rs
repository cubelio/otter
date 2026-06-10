use crate::sys::{NifEnv, NifTerm};
use crate::enif::funcs;

/// Returns `true` if `term` is a byte-aligned binary (`enif_is_binary`).
/// Sub-byte bitstrings return `false`.
pub(crate) unsafe fn is_binary(env: *mut NifEnv, term: NifTerm) -> bool {
    unsafe { (funcs().is_binary)(env, term) != 0 }
}

