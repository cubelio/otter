use std::ffi::c_int;
use crate::sys::{NifEnv, NifHash, NifTerm, NifTermType, NifUniqueInteger};
use crate::enif::funcs;

pub(crate) unsafe fn term_type(env: *mut NifEnv, term: NifTerm) -> NifTermType {
    unsafe { (funcs().term_type)(env, term) }
}

pub(crate) unsafe fn compare(lhs: NifTerm, rhs: NifTerm) -> c_int {
    unsafe { (funcs().compare)(lhs, rhs) }
}

pub(crate) unsafe fn is_identical(lhs: NifTerm, rhs: NifTerm) -> bool {
    unsafe { (funcs().is_identical)(lhs, rhs) != 0 }
}

pub(crate) unsafe fn make_copy(env: *mut NifEnv, src: NifTerm) -> NifTerm {
    unsafe { (funcs().make_copy)(env, src) }
}

pub(crate) unsafe fn consume_timeslice(env: *mut NifEnv, percent: c_int) -> c_int {
    unsafe { (funcs().consume_timeslice)(env, percent) }
}

/// Create a new unique reference.
pub(crate) unsafe fn make_ref(env: *mut NifEnv) -> NifTerm {
    unsafe { (funcs().make_ref)(env) }
}

/// Create a unique integer with the given properties.
pub(crate) unsafe fn make_unique_integer(
    env: *mut NifEnv,
    properties: NifUniqueInteger,
) -> NifTerm {
    unsafe { (funcs().make_unique_integer)(env, properties) }
}

/// Hash a term.
pub(crate) fn hash(algorithm: NifHash, term: NifTerm, salt: u64) -> u64 {
    unsafe { (funcs().hash)(algorithm, term, salt) }
}
