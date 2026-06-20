use core::marker::PhantomData;

use crate::types::sealed::Sealed;
use crate::types::{Env, FreeTerm, Invariant, RawTerm, Term};

/// An Erlang port identifier whose locality is not yet established.
///
/// Like [`Pid`](crate::types::Pid), an external (remote-node) port is a
/// heap-boxed term, so `Port<'id>` is tied to the environment it was read from.
/// To send a command or check liveness, refine it to a [`LocalPort`] with
/// [`to_local`](Port::to_local).
#[derive(Clone, Copy)]
pub struct Port<'id> {
    raw_term: RawTerm,
    _id: Invariant<'id>,
}

impl<'id> Port<'id> {
    #[crate::raw]
    pub(crate) fn from_raw(raw_term: RawTerm) -> Self {
        Self { raw_term, _id: PhantomData }
    }

    /// Refine to a [`LocalPort`] if this port is node-local
    /// (`enif_get_local_port`). `None` for an external (remote-node) port.
    pub fn to_local(self, env: impl Env<'id>) -> Option<LocalPort> {
        let mut out = enif_ffi::Port { port_id: 0 };
        (unsafe { enif_ffi::get_local_port(env.raw_env(), self.raw_term, &mut out) } != 0)
            .then_some(LocalPort { port: out })
    }

    /// Returns `true` if `term` is a port (`enif_is_port`).
    pub fn is_port(env: impl Env<'id>, term: impl Term<'id>) -> bool {
        unsafe { enif_ffi::is_port(env.raw_env(), term.raw_term()) != 0 }
    }
}

/// A node-local Erlang port identifier.
///
/// Holds an internal port id with no heap pointer. `Copy`, carries no brand
/// ([`FreeTerm`] — valid in any env), safe to store anywhere.
#[derive(Clone, Copy)]
pub struct LocalPort {
    pub(crate) port: enif_ffi::Port,
}

impl LocalPort {
    /// Look up a port by its registered name (`enif_whereis_port`).
    /// `None` if no port is registered under `name`.
    pub fn whereis<'id>(env: impl Env<'id>, name: impl Term<'id>) -> Option<LocalPort> {
        let mut out = enif_ffi::Port { port_id: 0 };
        (unsafe { enif_ffi::whereis_port(env.raw_env(), name.raw_term(), &mut out) } != 0)
            .then_some(LocalPort { port: out })
    }

    /// Check if the port is alive (`enif_is_port_alive`).
    pub fn is_alive<'id>(self, env: impl Env<'id>) -> bool {
        unsafe { enif_ffi::is_port_alive(env.raw_env(), &self.port) != 0 }
    }
}

impl PartialEq for Port<'_> {
    fn eq(&self, other: &Self) -> bool {
        unsafe { enif_ffi::is_identical(self.raw_term, other.raw_term) != 0 }
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
        unsafe { enif_ffi::compare(self.raw_term, other.raw_term) }.cmp(&0)
    }
}
impl std::fmt::Debug for Port<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Port")
    }
}

impl PartialEq for LocalPort {
    fn eq(&self, other: &Self) -> bool {
        unsafe { enif_ffi::is_identical(self.port.port_id, other.port.port_id) != 0 }
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
        unsafe { enif_ffi::compare(self.port.port_id, other.port.port_id) }.cmp(&0)
    }
}
impl std::fmt::Debug for LocalPort {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "LocalPort")
    }
}

impl<'id> Sealed for Port<'id> {}
impl<'id> Term<'id> for Port<'id> {
    fn raw_term(self) -> RawTerm {
        self.raw_term
    }
}

// LocalPort holds an immediate id — valid in any env, hence a FreeTerm.
impl Sealed for LocalPort {}
impl<'id> Term<'id> for LocalPort {
    fn raw_term(self) -> RawTerm {
        self.port.port_id
    }
}
impl FreeTerm for LocalPort {}
