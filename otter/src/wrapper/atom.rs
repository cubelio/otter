use std::ffi::{c_char, c_uint};
use crate::sys::{NifCharEncoding, NifEnv, NifTerm};
use crate::enif::funcs;

/// Create (or intern) an atom from a UTF-8 byte slice.
/// Uses `enif_make_new_atom_len` (NIF 2.17) which reports encoding errors.
/// Returns `None` if the name is not valid UTF-8 or atom table is full.
pub(crate) unsafe fn make_atom(env: *mut NifEnv, name: &[u8]) -> Option<NifTerm> {
    let mut term: NifTerm = 0;
    let ok = unsafe {
        (funcs().make_new_atom_len)(
            env,
            name.as_ptr() as *const c_char,
            name.len(),
            &mut term,
            NifCharEncoding::Utf8,
        )
    };
    if ok != 0 { Some(term) } else { None }
}

/// Get the UTF-8-encoded name length of an atom (not including null terminator).
/// Returns `None` if `term` is not an atom.
pub(crate) unsafe fn get_atom_length(env: *mut NifEnv, term: NifTerm) -> Option<usize> {
    let mut len: c_uint = 0;
    let ok = unsafe {
        (funcs().get_atom_length)(env, term, &mut len, NifCharEncoding::Utf8)
    };
    if ok != 0 { Some(len as usize) } else { None }
}

/// Copy the atom's UTF-8 name into `buf`. `buf` must be at least `len + 1` bytes
/// (the length returned by `get_atom_length`, plus one for the null terminator).
/// Returns the number of bytes written (excluding null terminator), or `None` on failure.
pub(crate) unsafe fn get_atom_into(
    env: *mut NifEnv,
    term: NifTerm,
    buf: &mut Vec<u8>,
) -> Option<usize> {
    let len = unsafe { get_atom_length(env, term)? };
    buf.resize(len + 1, 0);
    let written = unsafe {
        (funcs().get_atom)(
            env,
            term,
            buf.as_mut_ptr() as *mut c_char,
            buf.len() as c_uint,
            NifCharEncoding::Utf8,
        )
    };
    if written > 0 {
        buf.truncate((written - 1) as usize); // strip null terminator
        Some((written - 1) as usize)
    } else {
        None
    }
}

/// Look up an existing atom by name. Returns `None` if the atom does not exist.
/// Unlike `make_atom`, this never creates a new atom.
pub(crate) unsafe fn make_existing_atom(env: *mut NifEnv, name: &[u8]) -> Option<NifTerm> {
    let mut term: NifTerm = 0;
    let ok = unsafe {
        (funcs().make_existing_atom_len)(
            env,
            name.as_ptr() as *const c_char,
            name.len(),
            &mut term,
            NifCharEncoding::Utf8,
        )
    };
    if ok != 0 { Some(term) } else { None }
}
