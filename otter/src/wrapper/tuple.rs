use std::ffi::{c_int, c_uint};
use crate::sys::{NifEnv, NifTerm};
use crate::enif::funcs;

/// Decompose a tuple term. Returns a pointer into the BEAM heap (valid for
/// the lifetime of the env) and the arity, or `None` if not a tuple.
pub(crate) unsafe fn get_tuple(
    env: *mut NifEnv,
    term: NifTerm,
) -> Option<(*const NifTerm, usize)> {
    let mut arity: c_int = 0;
    let mut array: *const NifTerm = std::ptr::null();
    if unsafe { (funcs().get_tuple)(env, term, &mut arity, &mut array) != 0 } {
        Some((array, arity as usize))
    } else {
        None
    }
}

/// Construct a tuple from a slice of terms.
pub(crate) unsafe fn make_tuple(env: *mut NifEnv, terms: &[NifTerm]) -> NifTerm {
    unsafe {
        (funcs().make_tuple_from_array)(env, terms.as_ptr(), terms.len() as c_uint)
    }
}
