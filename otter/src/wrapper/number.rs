use crate::sys::{NifEnv, NifTerm};
use crate::enif::funcs;

pub(crate) unsafe fn get_i64(env: *mut NifEnv, term: NifTerm, out: &mut i64) -> bool {
    unsafe { (funcs().get_i64)(env, term, out) != 0 }
}

pub(crate) unsafe fn get_u64(env: *mut NifEnv, term: NifTerm, out: &mut u64) -> bool {
    unsafe { (funcs().get_u64)(env, term, out) != 0 }
}

pub(crate) unsafe fn make_i64(env: *mut NifEnv, val: i64) -> NifTerm {
    unsafe { (funcs().make_i64)(env, val) }
}

pub(crate) unsafe fn make_u64(env: *mut NifEnv, val: u64) -> NifTerm {
    unsafe { (funcs().make_u64)(env, val) }
}

pub(crate) unsafe fn get_double(env: *mut NifEnv, term: NifTerm, out: &mut f64) -> bool {
    unsafe { (funcs().get_double)(env, term, out) != 0 }
}

pub(crate) unsafe fn make_double(env: *mut NifEnv, val: f64) -> NifTerm {
    unsafe { (funcs().make_double)(env, val) }
}
