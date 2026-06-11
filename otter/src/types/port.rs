use crate::codec::{CodecError, Decoder, Encoder};
use crate::env::Env;
use crate::sys::{NifPort, NifTerm};
use crate::term::{Term, TypedTerm, TermIn};

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
    /// Returns `true` if the command was accepted.
    /// Wraps `enif_port_command`.
    pub fn command(self, env: Env<'_>, msg: impl TermIn) -> bool {
        let nif_port = NifPort { port_id: self.term };
        unsafe {
            crate::wrapper::port::port_command(
                env.as_ptr(),
                &nif_port,
                std::ptr::null_mut(),
                msg.as_c_arg(),
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
    fn decode(term: TypedTerm<'a>) -> Result<Self, CodecError> {
        match term {
            TypedTerm::Port(p) => Ok(p),
            _ => Err(CodecError::WrongType),
        }
    }
}
