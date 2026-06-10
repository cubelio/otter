use crate::sys::{NifEnv, NifMapIterator, NifMapIteratorEntry, NifTerm};
use crate::enif::funcs;

pub(crate) unsafe fn make_new_map(env: *mut NifEnv) -> NifTerm {
    unsafe { (funcs().make_new_map)(env) }
}

pub(crate) unsafe fn get_map_size(env: *mut NifEnv, map: NifTerm) -> Option<usize> {
    let mut size: usize = 0;
    if unsafe { (funcs().get_map_size)(env, map, &mut size) != 0 } {
        Some(size)
    } else {
        None
    }
}

pub(crate) unsafe fn get_map_value(
    env: *mut NifEnv,
    map: NifTerm,
    key: NifTerm,
) -> Option<NifTerm> {
    let mut value: NifTerm = 0;
    if unsafe { (funcs().get_map_value)(env, map, key, &mut value) != 0 } {
        Some(value)
    } else {
        None
    }
}

/// Returns the new map, or `None` if `map_in` is not a map.
pub(crate) unsafe fn make_map_put(
    env: *mut NifEnv,
    map_in: NifTerm,
    key: NifTerm,
    value: NifTerm,
) -> Option<NifTerm> {
    let mut map_out: NifTerm = 0;
    if unsafe { (funcs().make_map_put)(env, map_in, key, value, &mut map_out) != 0 } {
        Some(map_out)
    } else {
        None
    }
}

/// Returns the new map, or `None` if the key is not present or `map_in` is not a map.
pub(crate) unsafe fn make_map_update(
    env: *mut NifEnv,
    map_in: NifTerm,
    key: NifTerm,
    value: NifTerm,
) -> Option<NifTerm> {
    let mut map_out: NifTerm = 0;
    if unsafe { (funcs().make_map_update)(env, map_in, key, value, &mut map_out) != 0 } {
        Some(map_out)
    } else {
        None
    }
}

/// Returns the new map, or `None` if the key is not present or `map_in` is not a map.
pub(crate) unsafe fn make_map_remove(
    env: *mut NifEnv,
    map_in: NifTerm,
    key: NifTerm,
) -> Option<NifTerm> {
    let mut map_out: NifTerm = 0;
    if unsafe { (funcs().make_map_remove)(env, map_in, key, &mut map_out) != 0 } {
        Some(map_out)
    } else {
        None
    }
}

/// Initialise `iter` for iterating over `map`. Returns `false` if `map` is not a map.
/// Caller must call `map_iterator_destroy` when done.
pub(crate) unsafe fn map_iterator_create(
    env: *mut NifEnv,
    map: NifTerm,
    iter: &mut NifMapIterator,
    entry: NifMapIteratorEntry,
) -> bool {
    unsafe { (funcs().map_iterator_create)(env, map, iter, entry) != 0 }
}

pub(crate) unsafe fn map_iterator_destroy(env: *mut NifEnv, iter: &mut NifMapIterator) {
    unsafe { (funcs().map_iterator_destroy)(env, iter) }
}

/// Advance the iterator. Returns `false` when exhausted.
pub(crate) unsafe fn map_iterator_next(
    env: *mut NifEnv,
    iter: &mut NifMapIterator,
) -> bool {
    unsafe { (funcs().map_iterator_next)(env, iter) != 0 }
}

/// Get the current key and value. Returns `false` if the iterator is exhausted.
pub(crate) unsafe fn map_iterator_get_pair(
    env: *mut NifEnv,
    iter: &mut NifMapIterator,
) -> Option<(NifTerm, NifTerm)> {
    let mut key: NifTerm = 0;
    let mut value: NifTerm = 0;
    if unsafe { (funcs().map_iterator_get_pair)(env, iter, &mut key, &mut value) != 0 } {
        Some((key, value))
    } else {
        None
    }
}
