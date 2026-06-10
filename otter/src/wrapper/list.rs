use std::ffi::{c_char, c_int, c_uint};
use crate::sys::{NifCharEncoding, NifEnv, NifTerm};
use crate::enif::funcs;

/// Decompose a list term into head and tail.
/// Returns `false` if `term` is not a non-empty list (e.g. it is nil or a non-list).
pub(crate) unsafe fn get_list_cell(
    env: *mut NifEnv,
    term: NifTerm,
    head: &mut NifTerm,
    tail: &mut NifTerm,
) -> bool {
    unsafe { (funcs().get_list_cell)(env, term, head, tail) != 0 }
}

/// Returns the number of elements in a proper list, or `None` for an improper list.
pub(crate) unsafe fn get_list_length(env: *mut NifEnv, term: NifTerm) -> Option<usize> {
    let mut len: c_uint = 0;
    if unsafe { (funcs().get_list_length)(env, term, &mut len) != 0 } {
        Some(len as usize)
    } else {
        None
    }
}

/// Construct a list from a slice of terms.
pub(crate) unsafe fn make_list(env: *mut NifEnv, terms: &[NifTerm]) -> NifTerm {
    unsafe {
        (funcs().make_list_from_array)(env, terms.as_ptr(), terms.len() as c_uint)
    }
}

/// Get the length of a string (list of codepoints) in the given encoding.
pub(crate) unsafe fn get_string_length(
    env: *mut NifEnv,
    term: NifTerm,
    encoding: NifCharEncoding,
) -> Option<usize> {
    let mut len: c_uint = 0;
    if unsafe { (funcs().get_string_length)(env, term, &mut len, encoding) != 0 } {
        Some(len as usize)
    } else {
        None
    }
}

/// Extract a string (list of codepoints) into a buffer.
///
/// Returns the number of bytes written (including null terminator), or 0 on failure.
pub(crate) unsafe fn get_string(
    env: *mut NifEnv,
    term: NifTerm,
    buf: &mut [u8],
    encoding: NifCharEncoding,
) -> c_int {
    unsafe {
        (funcs().get_string)(
            env,
            term,
            buf.as_mut_ptr() as *mut c_char,
            buf.len() as c_uint,
            encoding,
        )
    }
}

/// Construct a string (list of codepoints) from a byte slice.
pub(crate) unsafe fn make_string_len(
    env: *mut NifEnv,
    string: &[u8],
    encoding: NifCharEncoding,
) -> NifTerm {
    unsafe {
        (funcs().make_string_len)(env, string.as_ptr() as *const c_char, string.len(), encoding)
    }
}

/// Reverse a proper list. Returns `None` for improper lists.
pub(crate) unsafe fn make_reverse_list(
    env: *mut NifEnv,
    term: NifTerm,
) -> Option<NifTerm> {
    let mut result: NifTerm = 0;
    if unsafe { (funcs().make_reverse_list)(env, term, &mut result) != 0 } {
        Some(result)
    } else {
        None
    }
}

/// Construct a cons cell: `[head | tail]`.
pub(crate) unsafe fn make_list_cell(
    env: *mut NifEnv,
    head: NifTerm,
    tail: NifTerm,
) -> NifTerm {
    unsafe { (funcs().make_list_cell)(env, head, tail) }
}
