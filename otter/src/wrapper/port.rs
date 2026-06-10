use crate::sys::{NifEnv, NifPort, NifTerm};
use crate::enif::funcs;

/// Send a message to a port. `msg_env` may be null (uses `env`).
/// Returns `true` if the command was accepted.
pub(crate) unsafe fn port_command(
    env: *mut NifEnv,
    to_port: &NifPort,
    msg_env: *mut NifEnv,
    msg: NifTerm,
) -> bool {
    unsafe { (funcs().port_command)(env, to_port, msg_env, msg) != 0 }
}

/// Look up a port by registered name. Returns `false` if no port
/// is registered with that name.
pub(crate) unsafe fn whereis_port(
    env: *mut NifEnv,
    name: NifTerm,
    out: &mut NifPort,
) -> bool {
    unsafe { (funcs().whereis_port)(env, name, out) != 0 }
}
