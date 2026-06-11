use crate::sys::{NifEnv, NifTerm};
use crate::enif::funcs;

/// Returns `true` if `term` is an atom (`enif_is_atom`).
pub(crate) unsafe fn is_atom(env: *mut NifEnv, term: NifTerm) -> bool {
    unsafe { (funcs().is_atom)(env, term) != 0 }
}

/// Returns `true` if `term` is a byte-aligned binary (`enif_is_binary`).
/// Sub-byte bitstrings return `false`.
pub(crate) unsafe fn is_binary(env: *mut NifEnv, term: NifTerm) -> bool {
    unsafe { (funcs().is_binary)(env, term) != 0 }
}

/// Returns `true` if `term` is a fun (`enif_is_fun`).
pub(crate) unsafe fn is_fun(env: *mut NifEnv, term: NifTerm) -> bool {
    unsafe { (funcs().is_fun)(env, term) != 0 }
}

/// Returns `true` if `term` is a list, including improper and empty lists
/// (`enif_is_list`).
pub(crate) unsafe fn is_list(env: *mut NifEnv, term: NifTerm) -> bool {
    unsafe { (funcs().is_list)(env, term) != 0 }
}

/// Returns `true` if `term` is a map (`enif_is_map`).
pub(crate) unsafe fn is_map(env: *mut NifEnv, term: NifTerm) -> bool {
    unsafe { (funcs().is_map)(env, term) != 0 }
}

/// Returns `true` if `term` is a pid (`enif_is_pid`).
pub(crate) unsafe fn is_pid(env: *mut NifEnv, term: NifTerm) -> bool {
    unsafe { (funcs().is_pid)(env, term) != 0 }
}

/// Returns `true` if `term` is a port (`enif_is_port`).
pub(crate) unsafe fn is_port(env: *mut NifEnv, term: NifTerm) -> bool {
    unsafe { (funcs().is_port)(env, term) != 0 }
}

/// Returns `true` if `term` is a reference (`enif_is_ref`).
pub(crate) unsafe fn is_ref(env: *mut NifEnv, term: NifTerm) -> bool {
    unsafe { (funcs().is_ref)(env, term) != 0 }
}

/// Returns `true` if `term` is a tuple (`enif_is_tuple`).
pub(crate) unsafe fn is_tuple(env: *mut NifEnv, term: NifTerm) -> bool {
    unsafe { (funcs().is_tuple)(env, term) != 0 }
}

