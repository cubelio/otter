//! Environment operation verbs not owned by a single term type.

use std::ffi::{c_int, c_void, CStr};

use crate::types::{AnyEnv, AnyTerm, CallEnv, Env, InitEnv, Raised, Term};

impl<'id> AnyEnv<'id> {
    /// The dynamic type of `term` (`enif_term_type`). `None` for a type code
    /// this otter build does not recognize (a newer-OTP type).
    pub fn term_type(self, term: impl Term<'id>) -> Option<enif_ffi::TermType> {
        let code = unsafe { enif_ffi::term_type(self.raw_env(), term.raw_term()) };
        enif_ffi::TermType::from_raw(code)
    }

    /// Hash a term (`enif_hash`). `algorithm` is `Phash2` (portable) or
    /// `InternalHash` (node-local, faster).
    pub fn hash(self, algorithm: enif_ffi::Hash, term: impl Term<'id>, salt: u64) -> u64 {
        unsafe { enif_ffi::hash(algorithm, term.raw_term(), salt) }
    }

    /// Tell the scheduler how much of the timeslice this NIF used
    /// (`enif_consume_timeslice`). `true` if the timeslice is exhausted.
    pub fn consume_timeslice(self, percent: i32) -> bool {
        unsafe { enif_ffi::consume_timeslice(self.raw_env(), percent) != 0 }
    }

    /// Whether the calling process is still alive
    /// (`enif_is_current_process_alive`).
    pub fn is_current_process_alive(self) -> bool {
        unsafe { enif_ffi::is_current_process_alive(self.raw_env()) != 0 }
    }
}

impl<'id> CallEnv<'id> {
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
