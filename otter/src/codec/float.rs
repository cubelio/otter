//! `Encoder`/`Decoder` for Rust's floating-point types.
//!
//! Erlang floats are always finite IEEE-754 doubles. Encoding therefore rejects
//! a non-finite value with [`CodecError::NotFinite`] (surfaced as `badret` on a
//! NIF return) rather than handing NaN/infinity to the BEAM. `f32` widens
//! losslessly to `f64` on the way out; on the way in it range-checks against the
//! finite `f32` range — a value larger than `f32::MAX` would narrow to infinity,
//! so it is rejected with [`CodecError::FloatRange`] (accepting ordinary
//! rounding within range).

use crate::codec::{CodecError, Decoder, Encoder};
use crate::types::{AnyTerm, Env, Float};

impl<'id> Encoder<'id> for f64 {
    fn encode(&self, env: impl Env<'id>) -> Result<AnyTerm<'id>, CodecError> {
        Float::from_f64(env, *self).ok_or(CodecError::NotFinite)?.encode(env)
    }
}

impl<'id> Decoder<'id> for f64 {
    fn decode(term: AnyTerm<'id>, env: impl Env<'id>) -> Result<Self, CodecError> {
        Ok(Float::decode(term, env)?.to_f64(env))
    }
}

impl<'id> Encoder<'id> for f32 {
    fn encode(&self, env: impl Env<'id>) -> Result<AnyTerm<'id>, CodecError> {
        Float::from_f64(env, f64::from(*self)).ok_or(CodecError::NotFinite)?.encode(env)
    }
}

impl<'id> Decoder<'id> for f32 {
    fn decode(term: AnyTerm<'id>, env: impl Env<'id>) -> Result<Self, CodecError> {
        let val = Float::decode(term, env)?.to_f64(env);
        // An Erlang float is always finite, so the only failure is magnitude:
        // narrowing a value beyond f32::MAX would produce infinity.
        if val.abs() > f64::from(f32::MAX) {
            return Err(CodecError::FloatRange);
        }
        Ok(val as f32)
    }
}
