use std::ffi::c_uint;
use crate::sys::{NifBinary, NifEnv, NifTerm};
use crate::enif::funcs;

/// Inspect a binary term. Returns `false` if the term is not a binary
/// (including sub-byte bitstrings, which `enif_inspect_binary` rejects).
pub(crate) unsafe fn inspect_binary(
    env: *mut NifEnv,
    term: NifTerm,
    bin: &mut NifBinary,
) -> bool {
    unsafe { (funcs().inspect_binary)(env, term, bin) != 0 }
}

/// Allocate a new binary of `size` bytes. The caller must write the data
/// and then call `make_binary` to convert it to a term, or `release_binary`
/// to free it without creating a term.
pub(crate) unsafe fn alloc_binary(size: usize, bin: &mut NifBinary) -> bool {
    unsafe { (funcs().alloc_binary)(size, bin) != 0 }
}

/// Resize a binary that was allocated with `alloc_binary` but not yet
/// converted to a term. Returns `false` if reallocation fails.
pub(crate) unsafe fn realloc_binary(bin: &mut NifBinary, size: usize) -> bool {
    unsafe { (funcs().realloc_binary)(bin, size) != 0 }
}

/// Release a binary that was allocated with `alloc_binary` but not yet
/// converted to a term.
pub(crate) unsafe fn release_binary(bin: &mut NifBinary) {
    unsafe { (funcs().release_binary)(bin) }
}

/// Convert an allocated `NifBinary` into a term. After this call the binary
/// is owned by the BEAM and must not be used again.
pub(crate) unsafe fn make_binary(env: *mut NifEnv, bin: &mut NifBinary) -> NifTerm {
    unsafe { (funcs().make_binary)(env, bin) }
}

/// Allocate a new binary of `size` bytes directly on the heap, writing the
/// term handle to `*term_out`. Returns a pointer to the writable bytes.
/// The bytes must be written before `term_out` is returned from the NIF.
pub(crate) unsafe fn make_new_binary(
    env: *mut NifEnv,
    size: usize,
    term_out: &mut NifTerm,
) -> *mut u8 {
    unsafe { (funcs().make_new_binary)(env, size, term_out) }
}

/// Create a sub-binary (zero-copy slice) of an existing binary term.
pub(crate) unsafe fn make_sub_binary(
    env: *mut NifEnv,
    bin_term: NifTerm,
    pos: usize,
    len: usize,
) -> NifTerm {
    unsafe { (funcs().make_sub_binary)(env, bin_term, pos, len) }
}

/// Serialize a term to the external binary format.
/// The binary is allocated with `enif_alloc_binary`; caller must release.
/// Returns `true` on success.
pub(crate) unsafe fn term_to_binary(
    env: *mut NifEnv,
    term: NifTerm,
    bin: &mut NifBinary,
) -> bool {
    unsafe { (funcs().term_to_binary)(env, term, bin) != 0 }
}

/// Deserialize a term from the external binary format.
/// `opts` may include `BIN2TERM_SAFE`.
/// Returns the number of bytes consumed, or 0 on failure.
pub(crate) unsafe fn binary_to_term(
    env: *mut NifEnv,
    data: *const u8,
    size: usize,
    term: &mut NifTerm,
    opts: c_uint,
) -> usize {
    unsafe { (funcs().binary_to_term)(env, data, size, term, opts) }
}
