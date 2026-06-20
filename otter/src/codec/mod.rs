//! `Encoder`, `Decoder`, and `CodecError`.
//!
//! The trait definitions, the otter term-type impls, and the return-position
//! `Result<T, Raised>` impl live here. Conversions for native Rust types are
//! split across submodules by concern.

mod bool;
mod float;
mod integer;
mod string;

use crate::types::{
    AnyTerm, Atom, Binary, Bitstring, Env, Float, Fun, Integer, List, LocalPid, LocalPort, Map, Pid,
    Port, Raised, Reference, Term, Tuple, TupleView, TypedTerm, THE_NON_VALUE,
};

/// Error returned by term type conversion operations.
///
/// otter's internal error type — it never appears in user NIF signatures. The
/// `#[otter::nif]` macro converts codec failures to `badarg` automatically.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodecError {
    /// The term was not the expected type.
    WrongType,
    /// An integer term did not fit the requested Rust integer type.
    IntegerOverflow,
    /// A float value could not be represented as an Erlang float because it is
    /// not finite (NaN or infinity) — an encode-side failure.
    NotFinite,
    /// A finite float term fell outside the finite range of the requested Rust
    /// float type (only `f32`, on decode).
    FloatRange,
    /// A binary's bytes were not valid UTF-8, or a list was not a valid string,
    /// when decoding to a Rust `String`.
    NotUtf8,
    /// The term's type code is one this otter build does not recognize — a term
    /// type added by a newer OTP than otter knows about.
    UnknownTermType,
}

impl std::fmt::Display for CodecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CodecError::WrongType => write!(f, "wrong term type"),
            CodecError::IntegerOverflow => write!(f, "integer overflow"),
            CodecError::NotFinite => write!(f, "float is not finite"),
            CodecError::FloatRange => write!(f, "float out of range"),
            CodecError::NotUtf8 => write!(f, "not valid UTF-8"),
            CodecError::UnknownTermType => write!(f, "unknown term type"),
        }
    }
}

impl std::error::Error for CodecError {}

// ---------------------------------------------------------------------------
// Encoder
// ---------------------------------------------------------------------------

/// Convert a value into an Erlang term of brand `'id`.
///
/// Fallible, mirroring [`Decoder`]: a value outside the Erlang term domain
/// (e.g. a non-finite `f64`) returns `Err(CodecError)`. The `#[otter::nif]`
/// return path turns an `Err` into a `badret` exception — symmetric to the
/// `badarg` a failed [`Decoder`] raises on the way in. Impls that cannot fail —
/// every otter term type encodes by wrapping its word for free — return `Ok`.
///
/// A same-brand term encodes for free; cross-env terms are not encoded — copy
/// them first with [`Term::copy_to`].
pub trait Encoder<'id> {
    fn encode(&self, env: impl Env<'id>) -> Result<AnyTerm<'id>, CodecError>;
}

/// Encode a `Result<T, Raised>` in **return position only**.
///
/// `Ok(v)` encodes `v`. `Err(raised)` carries proof that an exception is already
/// pending on the env, so the non-value marker is returned and the BEAM raises
/// the pending exception on NIF return. Never encode an `Err` mid-term (inside a
/// tuple/list/map) — that diverts the marker into a value position. Always
/// propagate a `Result<T, Raised>` with `?` or `return`.
impl<'id, T: Encoder<'id>> Encoder<'id> for Result<T, Raised<'id>> {
    fn encode(&self, env: impl Env<'id>) -> Result<AnyTerm<'id>, CodecError> {
        match self {
            Ok(v) => v.encode(env),
            Err(_) => Ok(AnyTerm::wrap(THE_NON_VALUE, env)),
        }
    }
}

// ---------------------------------------------------------------------------
// Decoder
// ---------------------------------------------------------------------------

/// Extract a value from an Erlang term of brand `'id`.
///
/// Implemented by otter term types. Takes the raw [`AnyTerm`] plus an env (the
/// term carries only its brand, not its env), so each impl pays exactly the type
/// check it needs. Returns `Err(CodecError)` if the term is not the expected
/// type or the value does not fit.
pub trait Decoder<'id>: Sized {
    fn decode(term: AnyTerm<'id>, env: impl Env<'id>) -> Result<Self, CodecError>;
}

/// The identity decode — any term decodes to itself.
impl<'id> Decoder<'id> for AnyTerm<'id> {
    fn decode(term: AnyTerm<'id>, _env: impl Env<'id>) -> Result<Self, CodecError> {
        Ok(term)
    }
}

// ---------------------------------------------------------------------------
// Built-in type impls
//
// Centralized here because each impl uses only the noun's public surface
// (`Term::raw_term`, the `is_*`/`term_type` predicates, `from_raw`): `encode`
// wraps the same-brand word for free, `decode` checks the type then rewraps.
//
// Two decode idioms appear below, chosen by what the NIF API offers, not by
// taste: types with a dedicated `enif_is_*` predicate (atom, binary, fun, pid,
// port, ref, list, map, tuple) check via `Type::is_*(env, term)`; the three
// with no such predicate (integer, float, bitstring) fall back to comparing
// `env.term_type(term)` against the expected `TermType`.
// ---------------------------------------------------------------------------

macro_rules! encode_by_wrap {
    ($($t:ty),+ $(,)?) => { $(
        impl<'id> Encoder<'id> for $t {
            fn encode(&self, env: impl Env<'id>) -> Result<AnyTerm<'id>, CodecError> {
                Ok(AnyTerm::wrap(Term::raw_term(*self), env))
            }
        }
    )+ };
}

encode_by_wrap!(
    AnyTerm<'id>, Integer<'id>, Float<'id>, Reference<'id>, Fun<'id>, Tuple<'id>, List<'id>,
    Map<'id>, Binary<'id>, Bitstring<'id>, Pid<'id>, Port<'id>, TupleView<'id>, Atom, LocalPid,
    LocalPort,
);

impl<'id> Encoder<'id> for TypedTerm<'id> {
    fn encode(&self, env: impl Env<'id>) -> Result<AnyTerm<'id>, CodecError> {
        Ok(AnyTerm::wrap((*self).raw_term(), env))
    }
}

// --- Decoders ---

impl<'id> Decoder<'id> for Integer<'id> {
    fn decode(term: AnyTerm<'id>, env: impl Env<'id>) -> Result<Self, CodecError> {
        if env.term_type(term) == Some(enif_ffi::TermType::Integer) {
            Ok(Integer::from_raw(term.raw_term()))
        } else {
            Err(CodecError::WrongType)
        }
    }
}

impl<'id> Decoder<'id> for Float<'id> {
    fn decode(term: AnyTerm<'id>, env: impl Env<'id>) -> Result<Self, CodecError> {
        if env.term_type(term) == Some(enif_ffi::TermType::Float) {
            Ok(Float::from_raw(term.raw_term()))
        } else {
            Err(CodecError::WrongType)
        }
    }
}

impl<'id> Decoder<'id> for Bitstring<'id> {
    fn decode(term: AnyTerm<'id>, env: impl Env<'id>) -> Result<Self, CodecError> {
        // Every binary is a bitstring, so this accepts both byte-aligned and
        // sub-byte; use `Binary` for the byte-aligned refinement.
        if env.term_type(term) == Some(enif_ffi::TermType::Bitstring) {
            Ok(Bitstring::from_raw(term.raw_term()))
        } else {
            Err(CodecError::WrongType)
        }
    }
}

impl<'id> Decoder<'id> for Reference<'id> {
    fn decode(term: AnyTerm<'id>, env: impl Env<'id>) -> Result<Self, CodecError> {
        if Reference::is_ref(env, term) {
            Ok(Reference::from_raw(term.raw_term()))
        } else {
            Err(CodecError::WrongType)
        }
    }
}

impl<'id> Decoder<'id> for Fun<'id> {
    fn decode(term: AnyTerm<'id>, env: impl Env<'id>) -> Result<Self, CodecError> {
        if Fun::is_fun(env, term) {
            Ok(Fun::from_raw(term.raw_term()))
        } else {
            Err(CodecError::WrongType)
        }
    }
}

impl<'id> Decoder<'id> for Tuple<'id> {
    fn decode(term: AnyTerm<'id>, env: impl Env<'id>) -> Result<Self, CodecError> {
        if Tuple::is_tuple(env, term) {
            Ok(Tuple::from_raw(term.raw_term()))
        } else {
            Err(CodecError::WrongType)
        }
    }
}

impl<'id> Decoder<'id> for List<'id> {
    fn decode(term: AnyTerm<'id>, env: impl Env<'id>) -> Result<Self, CodecError> {
        if List::is_list(env, term) {
            Ok(List::from_raw(term.raw_term()))
        } else {
            Err(CodecError::WrongType)
        }
    }
}

impl<'id> Decoder<'id> for Map<'id> {
    fn decode(term: AnyTerm<'id>, env: impl Env<'id>) -> Result<Self, CodecError> {
        if Map::is_map(env, term) {
            Ok(Map::from_raw(term.raw_term()))
        } else {
            Err(CodecError::WrongType)
        }
    }
}

impl<'id> Decoder<'id> for Binary<'id> {
    fn decode(term: AnyTerm<'id>, env: impl Env<'id>) -> Result<Self, CodecError> {
        if Binary::is_binary(env, term) {
            Ok(Binary::from_raw(term.raw_term()))
        } else {
            Err(CodecError::WrongType)
        }
    }
}

impl<'id> Decoder<'id> for Pid<'id> {
    fn decode(term: AnyTerm<'id>, env: impl Env<'id>) -> Result<Self, CodecError> {
        if Pid::is_pid(env, term) {
            Ok(Pid::from_raw(term.raw_term()))
        } else {
            Err(CodecError::WrongType)
        }
    }
}

impl<'id> Decoder<'id> for Port<'id> {
    fn decode(term: AnyTerm<'id>, env: impl Env<'id>) -> Result<Self, CodecError> {
        if Port::is_port(env, term) {
            Ok(Port::from_raw(term.raw_term()))
        } else {
            Err(CodecError::WrongType)
        }
    }
}

impl<'id> Decoder<'id> for Atom {
    fn decode(term: AnyTerm<'id>, env: impl Env<'id>) -> Result<Self, CodecError> {
        if Atom::is_atom(env, term) {
            Ok(Atom::from_raw(term.raw_term()))
        } else {
            Err(CodecError::WrongType)
        }
    }
}

impl<'id> Decoder<'id> for LocalPid {
    fn decode(term: AnyTerm<'id>, env: impl Env<'id>) -> Result<Self, CodecError> {
        // An external pid passes is_pid but is not local; get_local_pid rejects.
        Pid::from_raw(term.raw_term()).to_local(env).ok_or(CodecError::WrongType)
    }
}

impl<'id> Decoder<'id> for LocalPort {
    fn decode(term: AnyTerm<'id>, env: impl Env<'id>) -> Result<Self, CodecError> {
        Port::from_raw(term.raw_term()).to_local(env).ok_or(CodecError::WrongType)
    }
}

impl<'id> Decoder<'id> for TypedTerm<'id> {
    fn decode(term: AnyTerm<'id>, env: impl Env<'id>) -> Result<Self, CodecError> {
        term.resolve(env).ok_or(CodecError::UnknownTermType)
    }
}
