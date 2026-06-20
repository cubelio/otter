//! Environment operation verbs not owned by a single term type.

use std::ffi::{c_int, c_void, CStr};

use crate::types::{
    AnyTerm, BinaryBuf, CallEnv, CallingEnv, Env, InitEnv, LocalPid, LocalPort, RawTerm, Raised,
    Term, Tuple,
};

/// Serialize a term to the external term format (`enif_term_to_binary`),
/// returning the bytes in a [`BinaryBuf`]. Type-agnostic. `None` on failure.
pub fn serialize<'id>(env: impl Env<'id>, term: impl Term<'id>) -> Option<BinaryBuf> {
    let mut bin: enif_ffi::Binary = unsafe { std::mem::zeroed() };
    if unsafe { enif_ffi::term_to_binary(env.raw_env(), term.raw_term(), &mut bin) } != 0 {
        Some(BinaryBuf::from_filled(bin))
    } else {
        None
    }
}

/// Deserialize a term from external-term-format bytes (`enif_binary_to_term`).
/// If `safe`, encoded atoms not already in the atom table are rejected. `None`
/// on decode failure.
pub fn deserialize<'id>(env: impl Env<'id>, data: &[u8], safe: bool) -> Option<AnyTerm<'id>> {
    let opts = if safe { enif_ffi::BIN2TERM_SAFE } else { 0 };
    let mut term: RawTerm = 0;
    let consumed =
        unsafe { enif_ffi::binary_to_term(env.raw_env(), data.as_ptr(), data.len(), &mut term, opts) };
    (consumed != 0).then(|| AnyTerm::wrap(term, env))
}

/// Send `msg` to `pid` from inside a NIF, attributing it to the calling process
/// (`enif_send` with a NULL msg_env — the term is copied from the caller env).
/// `true` if the process was alive. Only a [`CallingEnv`] carries a caller.
pub fn send_from<'id>(env: impl CallingEnv<'id>, pid: &LocalPid, msg: impl Term<'id>) -> bool {
    unsafe { enif_ffi::send(env.raw_env(), &pid.pid, std::ptr::null_mut(), msg.raw_term()) != 0 }
}

/// Send a command to local `port` (`enif_port_command`, NULL msg_env — copied
/// from the caller env). `true` if accepted.
pub fn port_command<'id>(env: impl CallingEnv<'id>, port: &LocalPort, msg: impl Term<'id>) -> bool {
    unsafe {
        enif_ffi::port_command(env.raw_env(), &port.port, std::ptr::null_mut(), msg.raw_term()) != 0
    }
}

impl<'id> CallEnv<'id> {
    /// The current logical CPU's execution time in `erlang:timestamp/0` format
    /// (`enif_cpu_time`). `Err(Raised)` (`badarg`) if the OS does not support it.
    pub fn cpu_time(self) -> Result<Tuple<'id>, Raised<'id>> {
        let raw = unsafe { enif_ffi::cpu_time(self.raw_env()) };
        let term = self.check_raised(AnyTerm::wrap(raw, self))?;
        Ok(Tuple::from_raw(term.raw_term()))
    }

    /// Reschedule the current NIF to run `fp` (`enif_schedule_nif`). The success
    /// value must be returned directly from the NIF; a bad `fun_name` raises
    /// `badarg`, surfaced as `Err(Raised)`.
    ///
    /// # Safety
    /// `fp` must be a valid NIF function pointer; `argv` must point to `argc`
    /// valid terms.
    pub unsafe fn schedule_nif(
        self,
        fun_name: &CStr,
        flags: i32,
        fp: unsafe extern "C" fn(*mut enif_ffi::Env, c_int, *const enif_ffi::Term) -> enif_ffi::Term,
        argc: i32,
        argv: *const enif_ffi::Term,
    ) -> Result<AnyTerm<'id>, Raised<'id>> {
        let raw =
            unsafe { enif_ffi::schedule_nif(self.raw_env(), fun_name.as_ptr(), flags, fp, argc, argv) };
        self.check_raised(AnyTerm::wrap(raw, self))
    }
}

impl<'id> InitEnv<'id> {
    /// Enable delayed halt: the VM waits for running NIF calls before halting
    /// (`enif_set_option(ERL_NIF_OPT_DELAY_HALT)`). `true` on success.
    pub fn set_option_delay_halt(self) -> bool {
        unsafe { enif_ffi::set_option_delay_halt(self.raw_env()) == 0 }
    }

    /// Set the on-halt callback (`enif_set_option(ERL_NIF_OPT_ON_HALT)`).
    ///
    /// # Safety
    /// `callback` must remain valid for the lifetime of the VM.
    pub unsafe fn set_option_on_halt(self, callback: unsafe extern "C" fn(*mut c_void)) -> bool {
        unsafe { enif_ffi::set_option_on_halt(self.raw_env(), callback) == 0 }
    }

    /// Set the on-unload-thread callback
    /// (`enif_set_option(ERL_NIF_OPT_ON_UNLOAD_THREAD)`).
    ///
    /// # Safety
    /// `callback` must remain valid for the lifetime of the VM.
    pub unsafe fn set_option_on_unload_thread(
        self,
        callback: unsafe extern "C" fn(*mut c_void),
    ) -> bool {
        unsafe { enif_ffi::set_option_on_unload_thread(self.raw_env(), callback) == 0 }
    }
}
