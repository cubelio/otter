//! `Encoder`/`Decoder` for Rust's fixed-width and pointer-width integers.
//!
//! Encoding is infallible — every value fits `enif_make_int64` (signed) or
//! `enif_make_uint64` (unsigned). Decoding range-checks the Erlang integer into
//! the target Rust type and reports [`CodecError::IntegerOverflow`] when it does
//! not fit: a value outside the type's range, a negative decoded into an
//! unsigned type, or a bignum too wide for the 64-bit read.
//!
//! `i128`/`u128` are intentionally absent — the NIF API has no integer primitive
//! wider than 64 bits. Reading larger bignums (via ETF) is tracked separately
//! (enhance-14).

use crate::codec::{CodecError, Decoder, Encoder};
use crate::types::{AnyTerm, Env, Integer};

// Types narrower than 64 bits widen losslessly into the i64/u64 the NIF API
// takes, and narrow back through `try_from` with a range check.
macro_rules! narrow_signed {
    ($($t:ty),+ $(,)?) => { $(
        impl<'id> Encoder<'id> for $t {
            fn encode(&self, env: impl Env<'id>) -> Result<AnyTerm<'id>, CodecError> {
                Integer::from_i64(env, i64::from(*self)).encode(env)
            }
        }

        impl<'id> Decoder<'id> for $t {
            fn decode(term: AnyTerm<'id>, env: impl Env<'id>) -> Result<Self, CodecError> {
                let val = Integer::decode(term, env)?.to_i64(env).ok_or(CodecError::IntegerOverflow)?;
                Self::try_from(val).map_err(|_| CodecError::IntegerOverflow)
            }
        }
    )+ };
}

macro_rules! narrow_unsigned {
    ($($t:ty),+ $(,)?) => { $(
        impl<'id> Encoder<'id> for $t {
            fn encode(&self, env: impl Env<'id>) -> Result<AnyTerm<'id>, CodecError> {
                Integer::from_u64(env, u64::from(*self)).encode(env)
            }
        }

        impl<'id> Decoder<'id> for $t {
            fn decode(term: AnyTerm<'id>, env: impl Env<'id>) -> Result<Self, CodecError> {
                let val = Integer::decode(term, env)?.to_u64(env).ok_or(CodecError::IntegerOverflow)?;
                Self::try_from(val).map_err(|_| CodecError::IntegerOverflow)
            }
        }
    )+ };
}

narrow_signed!(i8, i16, i32);
narrow_unsigned!(u8, u16, u32);

// The exact 64-bit types map straight onto the NIF primitives — no cast, and the
// only decode failure is a bignum that does not fit the 64-bit read.
impl<'id> Encoder<'id> for i64 {
    fn encode(&self, env: impl Env<'id>) -> Result<AnyTerm<'id>, CodecError> {
        Integer::from_i64(env, *self).encode(env)
    }
}

impl<'id> Decoder<'id> for i64 {
    fn decode(term: AnyTerm<'id>, env: impl Env<'id>) -> Result<Self, CodecError> {
        Integer::decode(term, env)?.to_i64(env).ok_or(CodecError::IntegerOverflow)
    }
}

impl<'id> Encoder<'id> for u64 {
    fn encode(&self, env: impl Env<'id>) -> Result<AnyTerm<'id>, CodecError> {
        Integer::from_u64(env, *self).encode(env)
    }
}

impl<'id> Decoder<'id> for u64 {
    fn decode(term: AnyTerm<'id>, env: impl Env<'id>) -> Result<Self, CodecError> {
        Integer::decode(term, env)?.to_u64(env).ok_or(CodecError::IntegerOverflow)
    }
}

// `isize`/`usize` are 64-bit on every otter target, but Rust offers no `From`
// to/from `i64`/`u64` (the width is platform-dependent), so they cast explicitly
// and range-check on the way back in.
impl<'id> Encoder<'id> for isize {
    fn encode(&self, env: impl Env<'id>) -> Result<AnyTerm<'id>, CodecError> {
        Integer::from_i64(env, *self as i64).encode(env)
    }
}

impl<'id> Decoder<'id> for isize {
    fn decode(term: AnyTerm<'id>, env: impl Env<'id>) -> Result<Self, CodecError> {
        let val = Integer::decode(term, env)?.to_i64(env).ok_or(CodecError::IntegerOverflow)?;
        Self::try_from(val).map_err(|_| CodecError::IntegerOverflow)
    }
}

impl<'id> Encoder<'id> for usize {
    fn encode(&self, env: impl Env<'id>) -> Result<AnyTerm<'id>, CodecError> {
        Integer::from_u64(env, *self as u64).encode(env)
    }
}

impl<'id> Decoder<'id> for usize {
    fn decode(term: AnyTerm<'id>, env: impl Env<'id>) -> Result<Self, CodecError> {
        let val = Integer::decode(term, env)?.to_u64(env).ok_or(CodecError::IntegerOverflow)?;
        Self::try_from(val).map_err(|_| CodecError::IntegerOverflow)
    }
}
