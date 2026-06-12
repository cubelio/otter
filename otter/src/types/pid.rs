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
        let mut nif_pid = NifPid { pid: 0 };
        unsafe { crate::wrapper::pid::self_pid(env.as_ptr(), &mut nif_pid) };
        Pid { term: nif_pid.pid }
    }

    /// Check if the process identified by this pid is alive.
    ///
    /// Wraps `enif_is_process_alive`.
    pub fn is_alive(self, env: Env<'_>) -> bool {
        let mut nif_pid = NifPid { pid: self.term };
        unsafe { crate::wrapper::pid::is_process_alive(env.as_ptr(), &mut nif_pid) }
    }

    /// Look up a process by its registered name.
    ///
    /// Returns `None` if no process is registered under `name`.
    /// Wraps `enif_whereis_pid`.
    pub fn whereis(env: Env<'_>, name: crate::types::Atom) -> Option<Pid> {
        let mut nif_pid = NifPid { pid: 0 };
        if unsafe { crate::wrapper::pid::whereis_pid(env.as_ptr(), name.term, &mut nif_pid) } {
            Some(Pid { term: nif_pid.pid })
        } else {
            None
        }
    }

    /// Convert to a `NifPid` for use with lower-level NIF operations (e.g.
    /// `enif_send`). Returns `None` for distributed (non-local) pids.
    pub fn as_nif_pid(self, env: Env<'_>) -> Option<NifPid> {
        let mut nif_pid = NifPid { pid: 0 };
        if unsafe { crate::wrapper::pid::get_local_pid(env.as_ptr(), self.term, &mut nif_pid) } {
            Some(nif_pid)
        } else {
            None
        }
    }
}

impl PartialEq for Pid {
    fn eq(&self, other: &Self) -> bool {
        unsafe { crate::wrapper::term::is_identical(self.term, other.term) }
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
        let c = unsafe { crate::wrapper::term::compare(self.term, other.term) };
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
