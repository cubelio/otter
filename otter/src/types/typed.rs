//! `TypedTerm` and `resolve` — the typed view of a received term.

use crate::types::sealed::Sealed;
use crate::types::{
    AnyTerm, Atom, Binary, Bitstring, Env, Float, Fun, Integer, List, Map, Pid, Port, RawTerm,
    Reference, Term, Tuple,
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
        Some(match env.term_type(self)? {
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

impl<'id> Sealed for TypedTerm<'id> {}

impl<'id> Term<'id> for TypedTerm<'id> {
    /// Extract the underlying machine word, discarding the variant tag.
    fn raw_term(self) -> RawTerm {
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

impl<'id> From<Atom> for TypedTerm<'id> {
    fn from(v: Atom) -> Self {
        TypedTerm::Atom(v)
    }
}
impl<'id> From<Binary<'id>> for TypedTerm<'id> {
    /// A binary is a byte-aligned bitstring, so it lands in the `Bitstring` variant.
    fn from(v: Binary<'id>) -> Self {
        TypedTerm::Bitstring(Bitstring::from_raw(v.raw_term()))
    }
}
impl<'id> From<Bitstring<'id>> for TypedTerm<'id> {
    fn from(v: Bitstring<'id>) -> Self {
        TypedTerm::Bitstring(v)
    }
}
impl<'id> From<Float<'id>> for TypedTerm<'id> {
    fn from(v: Float<'id>) -> Self {
        TypedTerm::Float(v)
    }
}
impl<'id> From<Fun<'id>> for TypedTerm<'id> {
    fn from(v: Fun<'id>) -> Self {
        TypedTerm::Fun(v)
    }
}
impl<'id> From<Integer<'id>> for TypedTerm<'id> {
    fn from(v: Integer<'id>) -> Self {
        TypedTerm::Integer(v)
    }
}
impl<'id> From<List<'id>> for TypedTerm<'id> {
    fn from(v: List<'id>) -> Self {
        TypedTerm::List(v)
    }
}
impl<'id> From<Map<'id>> for TypedTerm<'id> {
    fn from(v: Map<'id>) -> Self {
        TypedTerm::Map(v)
    }
}
impl<'id> From<Pid<'id>> for TypedTerm<'id> {
    fn from(v: Pid<'id>) -> Self {
        TypedTerm::Pid(v)
    }
}
impl<'id> From<Port<'id>> for TypedTerm<'id> {
    fn from(v: Port<'id>) -> Self {
        TypedTerm::Port(v)
    }
}
impl<'id> From<Reference<'id>> for TypedTerm<'id> {
    fn from(v: Reference<'id>) -> Self {
        TypedTerm::Reference(v)
    }
}
impl<'id> From<Tuple<'id>> for TypedTerm<'id> {
    fn from(v: Tuple<'id>) -> Self {
        TypedTerm::Tuple(v)
    }
}

impl PartialEq for TypedTerm<'_> {
    fn eq(&self, other: &Self) -> bool {
        unsafe { enif_ffi::is_identical(Term::raw_term(*self), Term::raw_term(*other)) != 0 }
    }
}

impl Eq for TypedTerm<'_> {}

impl PartialOrd for TypedTerm<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for TypedTerm<'_> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let c = unsafe { enif_ffi::compare(Term::raw_term(*self), Term::raw_term(*other)) };
        c.cmp(&0)
    }
}
