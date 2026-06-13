use crate::codec::{CodecError, Decoder, Encoder};
use crate::env::Env;
use crate::sys::{NifPort, NifTerm};
use crate::term::{Term, AsNifTerm};

/// An Erlang port identifier whose locality is not yet established.
///
/// Like [`Pid`](crate::types::Pid), an external (remote-node) port is a
/// heap-boxed term, so `Port<'a>` is tied to the environment it was read from.
/// It supports identity, encoding, and forwarding. To send a command or check
/// liveness, refine it to a [`LocalPort`] with [`to_local`](Port::to_local).
#[derive(Clone, Copy)]
pub struct Port<'a> {
    pub(crate) term: NifTerm,
    pub(crate) env: Env<'a>,
}

impl<'a> Port<'a> {
    /// Refine to a [`LocalPort`] if this port is node-local. `None` for an
    /// external (remote-node) port. Wraps `enif_get_local_port`.
    pub fn to_local(self) -> Option<LocalPort> {
        self.env.get_local_port(self).map(|port| LocalPort { port })
    }
}

/// A node-local Erlang port identifier.
///
/// Validated via `enif_get_local_port` / `enif_whereis_port`, so it holds an
/// internal port id with no heap pointer. `Copy`, no lifetime, safe to store.
/// `enif_port_command` and `enif_is_port_alive` require a local port, so those
/// APIs take `&LocalPort`.
#[derive(Clone, Copy)]
pub struct LocalPort {
    pub(crate) port: NifPort,
}

impl LocalPort {
    /// Look up a port by its registered name (`enif_whereis_port`).
    /// Returns `None` if no port is registered under `name`.
    pub fn whereis(env: Env<'_>, name: crate::types::Atom) -> Option<LocalPort> {
        env.whereis_port(name)
    }

    /// Check if the port is alive (`enif_is_port_alive`).
    pub fn is_alive(self, env: Env<'_>) -> bool {
        env.is_port_alive(self.port)
    }
}

impl PartialEq for Port<'_> {
    fn eq(&self, other: &Self) -> bool {
        unsafe { crate::enif::is_identical(self.term, other.term) != 0 }
    }
}
impl Eq for Port<'_> {}
impl PartialOrd for Port<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Port<'_> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        unsafe { crate::enif::compare(self.term, other.term) }.cmp(&0)
    }
}
impl std::fmt::Debug for Port<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Port")
    }
}

impl PartialEq for LocalPort {
    fn eq(&self, other: &Self) -> bool {
        unsafe { crate::enif::is_identical(self.port.port_id, other.port.port_id) != 0 }
    }
}
impl Eq for LocalPort {}
impl PartialOrd for LocalPort {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for LocalPort {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        unsafe { crate::enif::compare(self.port.port_id, other.port.port_id) }.cmp(&0)
    }
}
impl std::fmt::Debug for LocalPort {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "LocalPort")
    }
}

impl Encoder for Port<'_> {
    fn encode<'a>(&self, env: Env<'a>) -> Term<'a> {
        Term::new(env, self.term)
    }
}

impl Encoder for LocalPort {
    fn encode<'a>(&self, env: Env<'a>) -> Term<'a> {
        Term::new(env, self.port.port_id)
    }
}

impl<'a> Env<'a> {
    /// Returns `true` if `term` is a port (`enif_is_port`).
    pub fn is_port(self, term: impl AsNifTerm<'a>) -> bool {
        unsafe { crate::enif::is_port(self.as_ptr(), term.as_nif_term()) != 0 }
    }

    /// Decode a term into a local `NifPort` (`enif_get_local_port`).
    /// `None` if `term` is not a local port.
    pub fn get_local_port(self, term: impl AsNifTerm<'a>) -> Option<NifPort> {
        let mut out = NifPort { port_id: 0 };
        if unsafe { crate::enif::get_local_port(self.as_ptr(), term.as_nif_term(), &mut out) != 0 } {
            Some(out)
        } else {
            None
        }
    }

    /// Whether the port identified by `port` is alive (`enif_is_port_alive`).
    pub fn is_port_alive(self, port: NifPort) -> bool {
        let mut port = port;
        unsafe { crate::enif::is_port_alive(self.as_ptr(), &mut port) != 0 }
    }

    /// Look up a port by its registered name (`enif_whereis_port`).
    /// `None` if no port is registered under `name`.
    pub fn whereis_port(self, name: impl AsNifTerm<'a>) -> Option<LocalPort> {
        let mut out = NifPort { port_id: 0 };
        if unsafe { crate::enif::whereis_port(self.as_ptr(), name.as_nif_term(), &mut out) != 0 } {
            Some(LocalPort { port: out })
        } else {
            None
        }
    }

    /// Send a command to local port `port` (`enif_port_command`).
    ///
    /// `msg_env` owns `msg` and need not be `self` — BEAM copies `msg` from
    /// `msg_env` into the port's mailbox. Returns `true` if the command was
    /// accepted.
    pub fn port_command<'b>(
        self,
        port: &LocalPort,
        msg_env: Env<'b>,
        msg: impl AsNifTerm<'b>,
    ) -> bool {
        unsafe {
            crate::enif::port_command(
                self.as_ptr(),
                &port.port,
                msg_env.as_ptr(),
                msg.as_nif_term(),
            ) != 0
        }
    }
}

impl<'a> Decoder<'a> for Port<'a> {
    fn decode(term: Term<'a>) -> Result<Self, CodecError> {
        if term.env.is_port(term) {
            Ok(Port { term: term.term, env: term.env })
        } else {
            Err(CodecError::WrongType)
        }
    }
}

impl<'a> Decoder<'a> for LocalPort {
    fn decode(term: Term<'a>) -> Result<Self, CodecError> {
        term.env
            .get_local_port(term)
            .map(|port| LocalPort { port })
            .ok_or(CodecError::WrongType)
    }
}
