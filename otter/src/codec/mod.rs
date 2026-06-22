//! Conversions between Erlang terms and native Rust types — [`Encoder`],
//! [`Decoder`], and [`CodecError`].
//!
//! Both directions are fallible and symmetric. A failed [`Decoder`] on a NIF
//! argument becomes a `badarg` exception on the way in; a failed [`Encoder`] on
//! a NIF return becomes a `badret` on the way out — the `#[otter::nif]` macro
//! performs both conversions, so [`CodecError`] never appears in a NIF signature.
//!
//! otter's own term types ([`Integer`], [`Binary`], [`Map`], …) implement both
//! traits and **never fail**: encoding wraps the term's machine word for free,
//! decoding pays exactly one type check. The fallible impls are the native-Rust
//! conversions, split across submodules by concern:
//!
//! - **integers** — `i8`…`i64`, `u8`…`u64`, `isize`/`usize`
//!   ([`IntegerOverflow`](CodecError::IntegerOverflow) on a value that does not
//!   fit)
//! - **floats** — `f32`/`f64` ([`NotFinite`](CodecError::NotFinite) on encode,
//!   [`FloatRange`](CodecError::FloatRange) on a too-wide `f32` decode)
//! - **bool** — the atoms `true`/`false`
//! - **`str`/`String`** — encode to a binary, decode from a binary or charlist
//!   ([`NotUtf8`](CodecError::NotUtf8))
//! - **tuples** — arities 1–12 ([`WrongArity`](CodecError::WrongArity))
//! - **`Vec<T>`/`[T]`** — proper Erlang lists
//! - **`HashMap<K, V>`** — Erlang maps
//! - **[`num_bigint::BigInt`]** — arbitrary-precision integers, under the
//!   `bigint` feature
//!
//! # No blanket `TryFrom`
//!
//! A blanket `impl<'a, T: Decoder<'a>> TryFrom<TypedTerm<'a>> for T` cannot be
//! provided: it violates Rust's orphan rules (E0210), because neither `TryFrom`
//! nor the type parameter `T` is local to this crate. Call
//! [`T::decode(term, env)`](Decoder::decode) directly instead.
//!
//! [`num_bigint::BigInt`]: https://docs.rs/num-bigint
//! [`Integer`]: crate::types::Integer
//! [`Binary`]: crate::types::Binary
//! [`Map`]: crate::types::Map

#[cfg(feature = "bigint")]
mod bigint;
mod bool;
mod float;
mod integer;
mod list;
mod map;
mod string;
mod tuple;

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
    /// An Erlang tuple's arity did not match the arity of the Rust tuple type it
    /// was being decoded into.
    WrongArity,
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
            CodecError::WrongArity => write!(f, "wrong tuple arity"),
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
        /// Encodes for free by rewrapping the term's own machine word — an otter
        /// term is already an Erlang term. Never fails.
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

/// Encodes the already-resolved term by rewrapping its word. Never fails.
impl<'id> Encoder<'id> for TypedTerm<'id> {
    fn encode(&self, env: impl Env<'id>) -> Result<AnyTerm<'id>, CodecError> {
        Ok(AnyTerm::wrap((*self).raw_term(), env))
    }
}

// --- Decoders ---
//
// Each checks the term's type, then rewraps the word into the concrete type;
// the only failure is [`CodecError::WrongType`] (or [`UnknownTermType`] for the
// `TypedTerm` resolve). No data is read off the BEAM heap.

/// Accepts an integer term (`enif_term_type == Integer`); rejects anything else
/// as [`WrongType`](CodecError::WrongType).
impl<'id> Decoder<'id> for Integer<'id> {
    fn decode(term: AnyTerm<'id>, env: impl Env<'id>) -> Result<Self, CodecError> {
        if env.term_type(term) == Some(enif_ffi::TermType::Integer) {
            Ok(Integer::from_raw(term.raw_term()))
        } else {
            Err(CodecError::WrongType)
        }
    }
}

/// Accepts a float term (`enif_term_type == Float`); [`WrongType`](CodecError::WrongType)
/// otherwise.
impl<'id> Decoder<'id> for Float<'id> {
    fn decode(term: AnyTerm<'id>, env: impl Env<'id>) -> Result<Self, CodecError> {
        if env.term_type(term) == Some(enif_ffi::TermType::Float) {
            Ok(Float::from_raw(term.raw_term()))
        } else {
            Err(CodecError::WrongType)
        }
    }
}

/// Accepts any bitstring (`enif_term_type == Bitstring`) — both byte-aligned
/// binaries and sub-byte bitstrings. Refine to [`Binary`] for the byte-aligned
/// case. [`WrongType`](CodecError::WrongType) otherwise.
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

/// Accepts a reference (`enif_is_ref`); [`WrongType`](CodecError::WrongType)
/// otherwise.
impl<'id> Decoder<'id> for Reference<'id> {
    fn decode(term: AnyTerm<'id>, env: impl Env<'id>) -> Result<Self, CodecError> {
        if Reference::is_ref(env, term) {
            Ok(Reference::from_raw(term.raw_term()))
        } else {
            Err(CodecError::WrongType)
        }
    }
}

/// Accepts a fun (`enif_is_fun`); [`WrongType`](CodecError::WrongType) otherwise.
impl<'id> Decoder<'id> for Fun<'id> {
    fn decode(term: AnyTerm<'id>, env: impl Env<'id>) -> Result<Self, CodecError> {
        if Fun::is_fun(env, term) {
            Ok(Fun::from_raw(term.raw_term()))
        } else {
            Err(CodecError::WrongType)
        }
    }
}

/// Accepts a tuple of any arity (`enif_is_tuple`);
/// [`WrongType`](CodecError::WrongType) otherwise.
impl<'id> Decoder<'id> for Tuple<'id> {
    fn decode(term: AnyTerm<'id>, env: impl Env<'id>) -> Result<Self, CodecError> {
        if Tuple::is_tuple(env, term) {
            Ok(Tuple::from_raw(term.raw_term()))
        } else {
            Err(CodecError::WrongType)
        }
    }
}

/// Accepts a list — including the empty list, and improper lists (`enif_is_list`).
/// [`WrongType`](CodecError::WrongType) otherwise.
impl<'id> Decoder<'id> for List<'id> {
    fn decode(term: AnyTerm<'id>, env: impl Env<'id>) -> Result<Self, CodecError> {
        if List::is_list(env, term) {
            Ok(List::from_raw(term.raw_term()))
        } else {
            Err(CodecError::WrongType)
        }
    }
}

/// Accepts a map (`enif_is_map`); [`WrongType`](CodecError::WrongType) otherwise.
impl<'id> Decoder<'id> for Map<'id> {
    fn decode(term: AnyTerm<'id>, env: impl Env<'id>) -> Result<Self, CodecError> {
        if Map::is_map(env, term) {
            Ok(Map::from_raw(term.raw_term()))
        } else {
            Err(CodecError::WrongType)
        }
    }
}

/// Accepts a byte-aligned binary (`enif_is_binary`); a sub-byte bitstring is
/// rejected as [`WrongType`](CodecError::WrongType) — decode to [`Bitstring`]
/// for those.
impl<'id> Decoder<'id> for Binary<'id> {
    fn decode(term: AnyTerm<'id>, env: impl Env<'id>) -> Result<Self, CodecError> {
        if Binary::is_binary(env, term) {
            Ok(Binary::from_raw(term.raw_term()))
        } else {
            Err(CodecError::WrongType)
        }
    }
}

/// Accepts a pid of any locality (`enif_is_pid`); to require a node-local pid,
/// decode to [`LocalPid`]. [`WrongType`](CodecError::WrongType) otherwise.
impl<'id> Decoder<'id> for Pid<'id> {
    fn decode(term: AnyTerm<'id>, env: impl Env<'id>) -> Result<Self, CodecError> {
        if Pid::is_pid(env, term) {
            Ok(Pid::from_raw(term.raw_term()))
        } else {
            Err(CodecError::WrongType)
        }
    }
}

/// Accepts a port of any locality (`enif_is_port`); to require a node-local
/// port, decode to [`LocalPort`]. [`WrongType`](CodecError::WrongType) otherwise.
impl<'id> Decoder<'id> for Port<'id> {
    fn decode(term: AnyTerm<'id>, env: impl Env<'id>) -> Result<Self, CodecError> {
        if Port::is_port(env, term) {
            Ok(Port::from_raw(term.raw_term()))
        } else {
            Err(CodecError::WrongType)
        }
    }
}

/// Accepts an atom (`enif_is_atom`); [`WrongType`](CodecError::WrongType)
/// otherwise.
impl<'id> Decoder<'id> for Atom {
    fn decode(term: AnyTerm<'id>, env: impl Env<'id>) -> Result<Self, CodecError> {
        if Atom::is_atom(env, term) {
            Ok(Atom::from_raw(term.raw_term()))
        } else {
            Err(CodecError::WrongType)
        }
    }
}

/// Accepts a node-local pid (`enif_get_local_pid`); an external (remote-node)
/// pid is rejected as [`WrongType`](CodecError::WrongType).
impl<'id> Decoder<'id> for LocalPid {
    fn decode(term: AnyTerm<'id>, env: impl Env<'id>) -> Result<Self, CodecError> {
        // An external pid passes is_pid but is not local; get_local_pid rejects.
        Pid::from_raw(term.raw_term()).to_local(env).ok_or(CodecError::WrongType)
    }
}

/// Accepts a node-local port; an external port is rejected as
/// [`WrongType`](CodecError::WrongType).
impl<'id> Decoder<'id> for LocalPort {
    fn decode(term: AnyTerm<'id>, env: impl Env<'id>) -> Result<Self, CodecError> {
        Port::from_raw(term.raw_term()).to_local(env).ok_or(CodecError::WrongType)
    }
}

/// Resolves the term to its [`TypedTerm`] variant (one `enif_term_type` call).
/// A type code this otter build does not recognize — a term type from a newer
/// OTP — is [`UnknownTermType`](CodecError::UnknownTermType).
impl<'id> Decoder<'id> for TypedTerm<'id> {
    fn decode(term: AnyTerm<'id>, env: impl Env<'id>) -> Result<Self, CodecError> {
        term.resolve(env).ok_or(CodecError::UnknownTermType)
    }
}
