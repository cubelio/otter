use crate::sys::{NifEnv, NifPid, NifTerm};
use crate::enif::funcs;

/// Allocate a new process-independent environment.
/// Must be freed with `free_env` or cleared with `clear_env`.
pub(crate) unsafe fn alloc_env() -> *mut NifEnv {
    unsafe { (funcs().alloc_env)() }
}

/// Free an environment allocated with `alloc_env`.
pub(crate) unsafe fn free_env(env: *mut NifEnv) {
    unsafe { (funcs().free_env)(env) }
}

/// Clear an environment, invalidating all terms created in it.
/// The environment can be reused after clearing.
pub(crate) unsafe fn clear_env(env: *mut NifEnv) {
    unsafe { (funcs().clear_env)(env) }
}

/// Send `msg` to `pid`. `env` is the caller's environment (may be null for
/// non-NIF threads). `msg_env` is the message environment owning `msg`.
/// Returns `false` if the process is not alive or the send failed.
pub(crate) unsafe fn send(
    env: *mut NifEnv,
    to: *const NifPid,
    msg_env: *mut NifEnv,
    msg: NifTerm,
) -> bool {
    unsafe { (funcs().send)(env, to, msg_env, msg) != 0 }
}
