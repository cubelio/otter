use crate::codec::{CodecError, Decoder, Encoder};
use crate::env::Env;
use crate::sys::{NifPort, NifTerm};
use crate::term::{Term, AsNifTerm};

/// An Erlang port identifier.
///
/// `Port` has no lifetime. It is `Copy` and safe to store anywhere.
#[derive(Clone, Copy)]
pub struct Port {
    pub(crate) term: NifTerm,
}

impl Port {
    /// Look up a port by its registered name.
    ///
    /// Returns `None` if no port is registered under `name`.
    /// Wraps `enif_whereis_port`.
    pub fn whereis(env: Env<'_>, name: crate::types::Atom) -> Option<Port> {
        let mut nif_port = NifPort { port_id: 0 };
        if unsafe { crate::wrapper::port::whereis_port(env.as_ptr(), name.term, &mut nif_port) } {
            Some(Port { term: nif_port.port_id })
        } else {
            None
        }
    }

    /// Send a command to this port.
    ///
    /// `caller_env` is the scheduler env making the call; `msg_env` is the
    /// env that owns `msg`. The two need not be the same — BEAM copies `msg`
    /// from `msg_env` into the port's mailbox.
    ///
    /// Returns `true` if the command was accepted.
    /// Wraps `enif_port_command`.
    pub fn command<'a, 'b>(
        self,
        caller_env: Env<'a>,
        msg_env: Env<'b>,
        msg: impl AsNifTerm<'b>,
    ) -> bool {
        let nif_port = NifPort { port_id: self.term };
        unsafe {
            crate::wrapper::port::port_command(
                caller_env.as_ptr(),
                &nif_port,
                msg_env.as_ptr(),
                msg.as_nif_term(),
            )
        }
    }
}

impl PartialEq for Port {
    fn eq(&self, other: &Self) -> bool {
        unsafe { crate::wrapper::term::is_identical(self.term, other.term) }
    }
}

impl Eq for Port {}

impl PartialOrd for Port {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Port {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let c = unsafe { crate::wrapper::term::compare(self.term, other.term) };
        c.cmp(&0)
    }
}

impl std::fmt::Debug for Port {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Port")
    }
}

impl Encoder for Port {
    fn encode<'a>(&self, env: Env<'a>) -> Term<'a> {
        Term::new(env, self.term)
    }
}

impl<'a> Decoder<'a> for Port {
    fn decode(term: Term<'a>) -> Result<Self, CodecError> {
        if unsafe { crate::wrapper::check::is_port(term.env.as_ptr(), term.term) } {
            Ok(Port { term: term.term })
        } else {
            Err(CodecError::WrongType)
        }
    }
}
