use core::marker::PhantomData;

use crate::types::sealed::Sealed;
use crate::types::{CallEnv, Env, FreeTerm, Invariant, RawTerm, Term};

/// An Erlang process identifier whose locality is not yet established.
///
/// `Pid<'id>` is tied to the environment it was read from: an external
/// (remote-node) pid is a heap-boxed term whose validity ends with that env.
/// To *act* on the process — send, monitor, check liveness — refine it to a
/// [`LocalPid`] with [`to_local`](Pid::to_local); the NIF API can only act on
/// local processes.
#[derive(Clone, Copy)]
pub struct Pid<'id> {
    raw_term: RawTerm,
    _id: Invariant<'id>,
}

impl<'id> Pid<'id> {
    #[crate::raw]
    pub(crate) fn from_raw(raw_term: RawTerm) -> Self {
        Self { raw_term, _id: PhantomData }
    }

    /// Refine to a [`LocalPid`] if this pid is node-local (`enif_get_local_pid`).
    /// `None` for an external (remote-node) pid.
    pub fn to_local(self, env: impl Env<'id>) -> Option<LocalPid> {
        let mut out = enif_ffi::Pid { pid: 0 };
        (unsafe { enif_ffi::get_local_pid(env.raw_env(), self.raw_term, &mut out) } != 0)
            .then_some(LocalPid { pid: out })
    }

    /// Returns `true` if `term` is a pid (`enif_is_pid`).
    pub fn is_pid(env: impl Env<'id>, term: impl Term<'id>) -> bool {
        unsafe { enif_ffi::is_pid(env.raw_env(), term.raw_term()) != 0 }
    }
}

/// A node-local Erlang process identifier.
///
/// Holds an *internal* pid: a tagged immediate with no heap pointer. It is
/// `Copy`, carries no brand ([`FreeTerm`] — valid in any env), and is safe to
/// store anywhere. Every NIF operation that acts on a process requires a local
/// pid.
#[derive(Clone, Copy)]
pub struct LocalPid {
    pub(crate) pid: enif_ffi::Pid,
}

impl LocalPid {
    /// The pid of the calling process (`enif_self`) — always local. Available
    /// only on a [`CallEnv`]; no other env kind has a calling process.
    pub fn self_(env: CallEnv<'_>) -> LocalPid {
        let mut out = enif_ffi::Pid { pid: 0 };
        // enif_self returns NULL outside a process-bound env; a Pid{0} there
        // would be an invalid term word. CallEnv rules that out by type, so the
        // assert only guards against an internal contract slip.
        let ok = unsafe { !enif_ffi::self_(env.raw_env(), &mut out).is_null() };
        assert!(ok, "enif_self returned NULL on a CallEnv");
        LocalPid { pid: out }
    }

    /// Look up a process by its registered name (`enif_whereis_pid`).
    /// `None` if no process is registered under `name`.
    pub fn whereis<'id>(env: impl Env<'id>, name: impl Term<'id>) -> Option<LocalPid> {
        let mut out = enif_ffi::Pid { pid: 0 };
        (unsafe { enif_ffi::whereis_pid(env.raw_env(), name.raw_term(), &mut out) } != 0)
            .then_some(LocalPid { pid: out })
    }

    /// Check if the process is alive (`enif_is_process_alive`).
    pub fn is_alive<'id>(self, env: impl Env<'id>) -> bool {
        unsafe { enif_ffi::is_process_alive(env.raw_env(), &self.pid) != 0 }
    }
}

impl PartialEq for Pid<'_> {
    fn eq(&self, other: &Self) -> bool {
        unsafe { enif_ffi::is_identical(self.raw_term, other.raw_term) != 0 }
    }
}
impl Eq for Pid<'_> {}
impl PartialOrd for Pid<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Pid<'_> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        unsafe { enif_ffi::compare(self.raw_term, other.raw_term) }.cmp(&0)
    }
}
impl std::fmt::Debug for Pid<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Pid")
    }
}

impl PartialEq for LocalPid {
    fn eq(&self, other: &Self) -> bool {
        unsafe { enif_ffi::is_identical(self.pid.pid, other.pid.pid) != 0 }
    }
}
impl Eq for LocalPid {}
impl PartialOrd for LocalPid {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for LocalPid {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        unsafe { enif_ffi::compare(self.pid.pid, other.pid.pid) }.cmp(&0)
    }
}
impl std::fmt::Debug for LocalPid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "LocalPid")
    }
}

impl<'id> Sealed for Pid<'id> {}
impl<'id> Term<'id> for Pid<'id> {
    fn raw_term(self) -> RawTerm {
        self.raw_term
    }
}

// LocalPid holds an immediate id — valid in any env, hence a FreeTerm.
impl Sealed for LocalPid {}
impl<'id> Term<'id> for LocalPid {
    fn raw_term(self) -> RawTerm {
        self.pid.pid
    }
}
impl FreeTerm for LocalPid {}
