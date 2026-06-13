//! `Encoder`, `Decoder`, and `CodecError`.
//!
//! `CodecError` is defined here and used by the type methods.
//! `Encoder` and `Decoder` trait definitions and implementations follow in a
//! subsequent step.

use crate::env::Env;
use crate::term::{Term, Raised};

/// Error returned by term type conversion operations.
///
/// This is otter's internal error type — it never appears in user NIF function
/// signatures. The `#[otter::nif]` macro converts codec failures to
/// `raise_badarg()` automatically.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodecError {
    /// The term was not the expected type.
    WrongType,
    /// An integer term did not fit the requested Rust integer type.
    IntegerOverflow,
    /// The term's type code is one this otter build does not recognize — a
    /// term type added by a newer OTP than otter knows about.
    UnknownTermType,
}

impl std::fmt::Display for CodecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CodecError::WrongType        => write!(f, "wrong term type"),
            CodecError::IntegerOverflow  => write!(f, "integer overflow"),
            CodecError::UnknownTermType  => write!(f, "unknown term type"),
        }
    }
}

impl std::error::Error for CodecError {}

// ---------------------------------------------------------------------------
// Encoder
// ---------------------------------------------------------------------------

/// Convert a value into an Erlang term.
///
/// Implemented by otter term types (`Integer<'a>`, `Binary<'a>`, `Atom`, etc.)
/// and by `ResourceArc<T>`. Native Rust types do not implement this trait —
/// conversions are always explicit via type methods.
///
/// Returns a [`Term<'a>`] tied to the target env's lifetime. Callers that
/// want the typed enum can call `.resolve()`; the NIF wrapper macro calls
/// `.as_raw()` directly and avoids the dispatch.
pub trait Encoder {
    fn encode<'a>(&self, env: Env<'a>) -> Term<'a>;
}

/// Encode a `Result<T, Raised>` as either a returned term or the pending
/// exception.
///
/// `Ok(v)` encodes `v`. `Err(raised)` carries proof that an exception has
/// already been raised on the env (a [`Raised`] can only be produced by an
/// operation that raised), so the marker word is returned directly — the BEAM
/// raises the already-pending exception on NIF return. Nothing is re-raised
/// here, so this is sound even though the env is in the exception state.
///
/// To raise from a NIF, produce a `Raised` via [`Env::raise_exception`] /
/// [`Env::make_badarg`] (e.g. `env.raise_exception(reason)?`) and let it
/// propagate; the error type of a NIF's `Result` must be `Raised`.
impl<'r, T: Encoder> Encoder for Result<T, Raised<'r>> {
    fn encode<'a>(&self, env: Env<'a>) -> Term<'a> {
        match self {
            Ok(v)       => v.encode(env),
            Err(raised) => Term::new(env, raised.raw()),
        }
    }
}

// ---------------------------------------------------------------------------
// Decoder
// ---------------------------------------------------------------------------

/// Extract a value from an Erlang term.
///
/// Implemented by otter term types. Takes a [`Term<'a>`] — the env-bound
/// wrapper around a raw NIF word, with no type tag attached — so each impl
/// pays exactly the type check it needs (one `enif_term_type` /
/// `enif_is_binary` call per decode, no eager resolve discriminator that
/// gets discarded). Returns `Err(CodecError)` if the term is not the
/// expected type or the value does not fit.
pub trait Decoder<'a>: Sized {
    fn decode(term: Term<'a>) -> Result<Self, CodecError>;
}

