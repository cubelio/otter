//! `Encoder`, `Decoder`, and `CodecError`.
//!
//! `CodecError` is defined here and used by the type methods.
//! `Encoder` and `Decoder` trait definitions and implementations follow in a
//! subsequent step.

use crate::env::Env;
use crate::term::Term;

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
    /// An integer in a list was not a valid Unicode codepoint.
    /// Returned by `List::try_string`.
    InvalidCodepoint,
}

impl std::fmt::Display for CodecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CodecError::WrongType        => write!(f, "wrong term type"),
            CodecError::IntegerOverflow  => write!(f, "integer overflow"),
            CodecError::InvalidCodepoint => write!(f, "invalid Unicode codepoint"),
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

/// Encode a `Result` as either a returned term or a raised exception.
///
/// `Ok(v)` encodes `v` and returns the resulting term. `Err(e)` encodes `e`,
/// passes the term to `enif_raise_exception`, and returns the resulting
/// exception term. The BEAM treats the latter as a class-`error` raise of
/// the encoded reason; the `Ok`/`Err` discrimination happens through normal
/// trait dispatch on the user's return type (no name matching anywhere).
///
/// This is the *only* implicit raise behavior in the codec — there is no
/// "looks like a Result" inference. A user type that wants Result-shaped
/// raise semantics must write its own `Encoder` impl.
impl<T: Encoder, E: Encoder> Encoder for Result<T, E> {
    fn encode<'a>(&self, env: Env<'a>) -> Term<'a> {
        match self {
            Ok(v)  => v.encode(env),
            Err(e) => env.raise(e.encode(env)),
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

