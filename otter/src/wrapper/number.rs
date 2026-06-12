use crate::sys::{NifEnv, NifTerm};
use crate::enif::funcs;

pub(crate) unsafe fn get_int64(env: *mut NifEnv, term: NifTerm, out: &mut i64) -> bool {
    unsafe { (funcs().get_int64)(env, term, out) != 0 }
}

pub(crate) unsafe fn get_uint64(env: *mut NifEnv, term: NifTerm, out: &mut u64) -> bool {
    unsafe { (funcs().get_uint64)(env, term, out) != 0 }
}

pub(crate) unsafe fn make_int64(env: *mut NifEnv, val: i64) -> NifTerm {
    unsafe { (funcs().make_int64)(env, val) }
}

pub(crate) unsafe fn make_uint64(env: *mut NifEnv, val: u64) -> NifTerm {
    unsafe { (funcs().make_uint64)(env, val) }
}

pub(crate) unsafe fn get_double(env: *mut NifEnv, term: NifTerm, out: &mut f64) -> bool {
    unsafe { (funcs().get_double)(env, term, out) != 0 }
}

pub(crate) unsafe fn make_double(env: *mut NifEnv, val: f64) -> NifTerm {
    unsafe { (funcs().make_double)(env, val) }
}
