use crate::codec::{CodecError, Decoder, Encoder};
use crate::env::Env;
use crate::sys::{NifPid, NifTerm};
use crate::term::{Term, AsNifTerm};

/// An Erlang process identifier whose locality is not yet established.
///
/// `Pid<'a>` is tied to the environment it was read from: an external
/// (remote-node) pid is a heap-boxed term whose validity ends with that env,
/// so it must not outlive `'a`. `Pid<'a>` supports identity (`PartialEq` /
/// `Ord`), encoding, and forwarding back to Erlang. To *act* on the process —
/// send, monitor, check liveness — refine it to a [`LocalPid`] with
/// [`to_local`](Pid::to_local); the NIF API can only act on local processes.
#[derive(Clone, Copy)]
pub struct Pid<'a> {
    pub(crate) term: NifTerm,
    pub(crate) env: Env<'a>,
}

impl<'a> Pid<'a> {
    /// Refine to a [`LocalPid`] if this pid is node-local. `None` for an
    /// external (remote-node) pid. Wraps `enif_get_local_pid`.
    pub fn to_local(self) -> Option<LocalPid> {
        self.env.get_local_pid(self).map(|pid| LocalPid { pid })
    }
}

/// A node-local Erlang process identifier.
///
/// Obtained via `enif_self` / `enif_whereis_pid` / `enif_get_local_pid`
/// (see [`Pid::to_local`]), so it holds an *internal* pid: a tagged immediate
/// with no heap pointer. It is `Copy`, carries no lifetime, and is safe to
/// store anywhere. Every NIF operation that acts on a process — `enif_send`,
/// `enif_monitor_process`, `enif_is_process_alive`, `enif_select` — requires a
/// local pid, so those APIs take `&LocalPid`.
#[derive(Clone, Copy)]
pub struct LocalPid {
    pub(crate) pid: NifPid,
}

impl LocalPid {
    /// The pid of the calling process (`enif_self`) — always local.
    pub fn self_(env: Env<'_>) -> LocalPid {
        env.self_pid()
    }

    /// Look up a process by its registered name (`enif_whereis_pid`).
    /// Returns `None` if no process is registered under `name`.
    pub fn whereis(env: Env<'_>, name: crate::types::Atom) -> Option<LocalPid> {
        env.whereis_pid(name)
    }

    /// Check if the process is alive (`enif_is_process_alive`).
    pub fn is_alive(self, env: Env<'_>) -> bool {
        env.is_process_alive(self.pid)
    }
}

impl PartialEq for Pid<'_> {
    fn eq(&self, other: &Self) -> bool {
        unsafe { crate::enif::is_identical(self.term, other.term) != 0 }
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
        unsafe { crate::enif::compare(self.term, other.term) }.cmp(&0)
    }
}
impl std::fmt::Debug for Pid<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Pid")
    }
}

impl PartialEq for LocalPid {
    fn eq(&self, other: &Self) -> bool {
        unsafe { crate::enif::is_identical(self.pid.pid, other.pid.pid) != 0 }
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
        unsafe { crate::enif::compare(self.pid.pid, other.pid.pid) }.cmp(&0)
    }
}
impl std::fmt::Debug for LocalPid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "LocalPid")
    }
}

impl Encoder for Pid<'_> {
    fn encode<'a>(&self, env: Env<'a>) -> Term<'a> {
        Term::new(env, self.term)
    }
}

impl Encoder for LocalPid {
    fn encode<'a>(&self, env: Env<'a>) -> Term<'a> {
        // The internal pid term is a tagged immediate — valid in any env.
        Term::new(env, self.pid.pid)
    }
}

impl<'a> Env<'a> {
    /// Returns `true` if `term` is a pid (`enif_is_pid`).
    pub fn is_pid(self, term: impl AsNifTerm<'a>) -> bool {
        unsafe { crate::enif::is_pid(self.as_ptr(), term.as_nif_term()) != 0 }
    }

    /// The pid of the calling process (`enif_self`).
    pub fn self_pid(self) -> LocalPid {
        let mut out = NifPid { pid: 0 };
        unsafe { crate::enif::self_(self.as_ptr(), &mut out) };
        LocalPid { pid: out }
    }

    /// Decode a term into a local `NifPid` (`enif_get_local_pid`).
    /// `None` if `term` is not a local pid.
    pub fn get_local_pid(self, term: impl AsNifTerm<'a>) -> Option<NifPid> {
        let mut out = NifPid { pid: 0 };
        if unsafe { crate::enif::get_local_pid(self.as_ptr(), term.as_nif_term(), &mut out) != 0 } {
            Some(out)
        } else {
            None
        }
    }

    /// Whether the process identified by `pid` is alive
    /// (`enif_is_process_alive`).
    pub fn is_process_alive(self, pid: NifPid) -> bool {
        let mut pid = pid;
        unsafe { crate::enif::is_process_alive(self.as_ptr(), &mut pid) != 0 }
    }

    /// Look up a process by its registered name (`enif_whereis_pid`).
    /// `None` if no process is registered under `name`.
    pub fn whereis_pid(self, name: impl AsNifTerm<'a>) -> Option<LocalPid> {
        let mut out = NifPid { pid: 0 };
        if unsafe { crate::enif::whereis_pid(self.as_ptr(), name.as_nif_term(), &mut out) != 0 } {
            Some(LocalPid { pid: out })
        } else {
            None
        }
    }

    /// Send `msg` (a term in this env) to local process `to` (`enif_send`).
    ///
    /// The message is copied into the target's mailbox. Returns `true` if `to`
    /// was alive. This is the in-NIF send; from a non-scheduler thread build
    /// the message in an `OwnedEnv` and use `OwnedEnv::send` instead.
    pub fn send(self, to: &LocalPid, msg: impl AsNifTerm<'a>) -> bool {
        // null msg_env: msg is a term in this (caller) env and is copied.
        unsafe {
            crate::enif::send(self.as_ptr(), &to.pid, std::ptr::null_mut(), msg.as_nif_term()) != 0
        }
    }
}

impl<'a> Decoder<'a> for Pid<'a> {
    fn decode(term: Term<'a>) -> Result<Self, CodecError> {
        if term.env.is_pid(term) {
            Ok(Pid { term: term.term, env: term.env })
        } else {
            Err(CodecError::WrongType)
        }
    }
}

impl<'a> Decoder<'a> for LocalPid {
    fn decode(term: Term<'a>) -> Result<Self, CodecError> {
        // An external (remote-node) pid passes enif_is_pid but is not local;
        // get_local_pid rejects it. Treated as a wrong-type for LocalPid.
        term.env
            .get_local_pid(term)
            .map(|pid| LocalPid { pid })
            .ok_or(CodecError::WrongType)
    }
}
