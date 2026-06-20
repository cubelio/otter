//! `Encoder`/`Decoder` for [`num_bigint::BigInt`] — arbitrary-precision Erlang
//! integers, including bignums beyond `i64`/`u64`. Gated behind the `bigint`
//! feature.
//!
//! Both directions go through [`Integer::from_bigint`]/[`Integer::to_bigint`],
//! which use the external term format for the >64-bit cases (the NIF API has no
//! bignum accessor). A non-integer term decodes as [`CodecError::WrongType`];
//! encoding never fails.

use num_bigint::BigInt;

use crate::codec::{CodecError, Decoder, Encoder};
use crate::types::{AnyTerm, Env, Integer};

impl<'id> Encoder<'id> for BigInt {
    fn encode(&self, env: impl Env<'id>) -> Result<AnyTerm<'id>, CodecError> {
        Integer::from_bigint(env, self).encode(env)
    }
}

impl<'id> Decoder<'id> for BigInt {
    fn decode(term: AnyTerm<'id>, env: impl Env<'id>) -> Result<Self, CodecError> {
        Ok(Integer::decode(term, env)?.to_bigint(env))
    }
}
