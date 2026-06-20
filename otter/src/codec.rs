//! `Encoder`, `Decoder`, and `CodecError`.

use crate::types::{AnyTerm, Env, Raised};

/// The BEAM's non-value marker. Returned from a NIF whose `Result` is `Err`, so
/// the BEAM raises the already-pending exception.
const THE_NON_VALUE: enif_ffi::Term = 0;

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
    /// The term's type code is one this otter build does not recognize — a term
    /// type added by a newer OTP than otter knows about.
    UnknownTermType,
}

impl std::fmt::Display for CodecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CodecError::WrongType => write!(f, "wrong term type"),
            CodecError::IntegerOverflow => write!(f, "integer overflow"),
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
/// Implemented by otter term types and `ResourceArc<T>` — never by native Rust
/// types (conversions are always explicit). A same-brand term encodes by
/// wrapping its word for free; cross-env terms are not encoded — copy them
/// first with [`Term::copy_to`].
pub trait Encoder<'id> {
    fn encode(&self, env: impl Env<'id>) -> AnyTerm<'id>;
}

/// Encode a `Result<T, Raised>` in **return position only**.
///
/// `Ok(v)` encodes `v`. `Err(raised)` carries proof that an exception is already
/// pending on the env, so the non-value marker is returned and the BEAM raises
/// the pending exception on NIF return. Never encode an `Err` mid-term (inside a
/// tuple/list/map) — that diverts the marker into a value position. Always
/// propagate a `Result<T, Raised>` with `?` or `return`.
impl<'id, T: Encoder<'id>> Encoder<'id> for Result<T, Raised<'id>> {
    fn encode(&self, env: impl Env<'id>) -> AnyTerm<'id> {
        match self {
            Ok(v) => v.encode(env),
            Err(_) => AnyTerm::wrap(THE_NON_VALUE, env),
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
