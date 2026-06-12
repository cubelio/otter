use crate::codec::{CodecError, Decoder, Encoder};
use crate::env::Env;
use crate::sys::{NifPid, NifTerm};
use crate::term::{Term, AsNifTerm};

/// An Erlang process identifier.
///
/// `Pid` has no lifetime — it encodes its identity directly in the term word
/// with no heap pointer. It is `Copy` and safe to store anywhere.
#[derive(Clone, Copy)]
pub struct Pid {
    pub(crate) term: NifTerm,
}

impl Pid {
    /// Return the pid of the calling process.
    pub fn self_(env: Env<'_>) -> Pid {
        env.self_pid()
    }

    /// Check if the process identified by this pid is alive.
    ///
    /// Wraps `enif_is_process_alive`.
    pub fn is_alive(self, env: Env<'_>) -> bool {
        env.is_process_alive(NifPid { pid: self.term })
    }

    /// Look up a process by its registered name.
    ///
    /// Returns `None` if no process is registered under `name`.
    /// Wraps `enif_whereis_pid`.
    pub fn whereis(env: Env<'_>, name: crate::types::Atom) -> Option<Pid> {
        env.whereis_pid(name)
    }

    /// Convert to a `NifPid` for use with lower-level NIF operations (e.g.
    /// `enif_send`). Returns `None` for distributed (non-local) pids.
    pub fn as_nif_pid(self, env: Env<'_>) -> Option<NifPid> {
        env.get_local_pid(self)
    }
}

impl PartialEq for Pid {
    fn eq(&self, other: &Self) -> bool {
        unsafe { crate::enif::is_identical(self.term, other.term) != 0 }
    }
}

impl Eq for Pid {}

impl PartialOrd for Pid {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Pid {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let c = unsafe { crate::enif::compare(self.term, other.term) };
        c.cmp(&0)
    }
}

impl std::fmt::Debug for Pid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Pid")
    }
}

impl Encoder for Pid {
    fn encode<'a>(&self, env: Env<'a>) -> Term<'a> {
        // Pids are tagged immediates — valid in any environment.
        Term::new(env, self.term)
    }
}

impl<'a> Env<'a> {
    /// Returns `true` if `term` is a pid (`enif_is_pid`).
    pub fn is_pid(self, term: impl AsNifTerm<'a>) -> bool {
        unsafe { crate::enif::is_pid(self.as_ptr(), term.as_nif_term()) != 0 }
    }

    /// The pid of the calling process (`enif_self`).
    pub fn self_pid(self) -> Pid {
        let mut out = NifPid { pid: 0 };
        unsafe { crate::enif::self_(self.as_ptr(), &mut out) };
        Pid { term: out.pid }
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
    pub fn whereis_pid(self, name: impl AsNifTerm<'a>) -> Option<Pid> {
        let mut out = NifPid { pid: 0 };
        if unsafe { crate::enif::whereis_pid(self.as_ptr(), name.as_nif_term(), &mut out) != 0 } {
            Some(Pid { term: out.pid })
        } else {
            None
        }
    }
}

impl<'a> Decoder<'a> for Pid {
    fn decode(term: Term<'a>) -> Result<Self, CodecError> {
        if term.env.is_pid(term) {
            Ok(Pid { term: term.term })
        } else {
            Err(CodecError::WrongType)
        }
    }
}
