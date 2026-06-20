//! `TypedTerm` and `resolve` — the typed view of a received term.

use crate::types::{
    AnyTerm, Atom, Bitstring, Env, Float, Fun, Integer, List, Map, Pid, Port, RawTerm, Reference,
    Term, Tuple,
};

/// Typed enum produced by [`AnyTerm::resolve`]. One `enif_term_type` call has
/// been made; the data is still on the BEAM heap.
///
/// Mirrors BEAM's `ErlNifTermType`: byte-aligned binaries and sub-byte
/// bitstrings share the [`Bitstring`](Self::Bitstring) variant (BEAM treats
/// every binary as a bitstring). Refine with [`Bitstring::to_binary`].
#[derive(Clone, Copy)]
pub enum TypedTerm<'id> {
    Atom(Atom),
    Bitstring(Bitstring<'id>),
    Float(Float<'id>),
    Fun(Fun<'id>),
    Integer(Integer<'id>),
    List(List<'id>),
    Map(Map<'id>),
    Pid(Pid<'id>),
    Port(Port<'id>),
    Reference(Reference<'id>),
    Tuple(Tuple<'id>),
}

impl<'id> AnyTerm<'id> {
    /// Resolve to a typed [`TypedTerm`] (`enif_term_type`). Exactly one NIF call.
    /// `None` for a type code this otter build does not recognize (a newer-OTP
    /// type); the original `AnyTerm` is still usable.
    pub fn resolve(self, env: impl Env<'id>) -> Option<TypedTerm<'id>> {
        let raw = self.raw_term();
        Some(match env.as_any_env().term_type(self)? {
            enif_ffi::TermType::Atom => TypedTerm::Atom(Atom::from_raw(raw)),
            enif_ffi::TermType::Bitstring => TypedTerm::Bitstring(Bitstring::from_raw(raw)),
            enif_ffi::TermType::Float => TypedTerm::Float(Float::from_raw(raw)),
            enif_ffi::TermType::Fun => TypedTerm::Fun(Fun::from_raw(raw)),
            enif_ffi::TermType::Integer => TypedTerm::Integer(Integer::from_raw(raw)),
            enif_ffi::TermType::List => TypedTerm::List(List::from_raw(raw)),
            enif_ffi::TermType::Map => TypedTerm::Map(Map::from_raw(raw)),
            enif_ffi::TermType::Pid => TypedTerm::Pid(Pid::from_raw(raw)),
            enif_ffi::TermType::Port => TypedTerm::Port(Port::from_raw(raw)),
            enif_ffi::TermType::Reference => TypedTerm::Reference(Reference::from_raw(raw)),
            enif_ffi::TermType::Tuple => TypedTerm::Tuple(Tuple::from_raw(raw)),
        })
    }
}

impl<'id> TypedTerm<'id> {
    /// Extract the underlying machine word, discarding the variant tag.
    pub fn raw_term(self) -> RawTerm {
        match self {
            TypedTerm::Atom(v) => v.raw_term(),
            TypedTerm::Bitstring(v) => v.raw_term(),
            TypedTerm::Float(v) => v.raw_term(),
            TypedTerm::Fun(v) => v.raw_term(),
            TypedTerm::Integer(v) => v.raw_term(),
            TypedTerm::List(v) => v.raw_term(),
            TypedTerm::Map(v) => v.raw_term(),
            TypedTerm::Pid(v) => v.raw_term(),
            TypedTerm::Port(v) => v.raw_term(),
            TypedTerm::Reference(v) => v.raw_term(),
            TypedTerm::Tuple(v) => v.raw_term(),
        }
    }
}
