use std::ffi::{c_char, c_void};
use crate::sys::{NifEnv, NifResourceFlags, NifResourceType, NifResourceTypeInit, NifTerm};
use crate::enif::funcs;

/// Register a resource type. Must be called from the NIF load callback.
/// Returns `None` if registration failed.
pub(crate) unsafe fn init_resource_type(
    env: *mut NifEnv,
    name: *const c_char,
    init: *const NifResourceTypeInit,
    flags: NifResourceFlags,
    tried: *mut NifResourceFlags,
) -> Option<*mut NifResourceType> {
    let rt = unsafe { (funcs().init_resource_type)(env, name, init, flags, tried) };
    if rt.is_null() { None } else { Some(rt) }
}

/// Allocate a resource object of `size` bytes.
/// The memory is zeroed. The caller must initialise it before calling `make_resource`.
pub(crate) unsafe fn alloc_resource(
    resource_type: *mut NifResourceType,
    size: usize,
) -> *mut c_void {
    unsafe { (funcs().alloc_resource)(resource_type, size) }
}

/// Decrement the reference count on a resource object.
/// Called when the Rust side releases its handle.
pub(crate) unsafe fn release_resource(obj: *mut c_void) {
    unsafe { (funcs().release_resource)(obj) }
}

/// Wrap a resource pointer as a term. The BEAM takes a reference.
pub(crate) unsafe fn make_resource(env: *mut NifEnv, obj: *mut c_void) -> NifTerm {
    unsafe { (funcs().make_resource)(env, obj) }
}

/// Unwrap a resource term. Returns `false` if the term is not a resource of
/// the expected type.
pub(crate) unsafe fn get_resource(
    env: *mut NifEnv,
    term: NifTerm,
    resource_type: *mut NifResourceType,
    obj: *mut *mut c_void,
) -> bool {
    unsafe { (funcs().get_resource)(env, term, resource_type, obj) != 0 }
}

/// Increment the reference count on a resource object.
pub(crate) unsafe fn keep_resource(obj: *mut c_void) {
    unsafe { (funcs().keep_resource)(obj) }
}

/// Invoke a dynamic resource call.
///
/// `mod_term` and `name_term` identify the resource type, `rsrc_term` is the
/// resource term, and `call_data` is passed to the dyncall callback.
pub(crate) unsafe fn dynamic_resource_call(
    env: *mut NifEnv,
    mod_term: NifTerm,
    name_term: NifTerm,
    rsrc_term: NifTerm,
    call_data: *mut c_void,
) -> i32 {
    unsafe { (funcs().dynamic_resource_call)(env, mod_term, name_term, rsrc_term, call_data) }
}
