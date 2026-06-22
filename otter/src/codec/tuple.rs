//! `Encoder`/`Decoder` for Rust tuples <-> Erlang tuples.
//!
//! Fixed-arity and element-wise, implemented for arities 1 through 12 (each
//! element type carries its own `Encoder`/`Decoder`). Encoding builds an N-tuple
//! from the encoded elements; decoding requires an Erlang tuple of exactly N
//! elements ([`CodecError::WrongArity`] otherwise) and decodes each in turn.

use std::ffi::c_uint;

use crate::codec::{CodecError, Decoder, Encoder};
use crate::types::{AnyTerm, Env, Term, Tuple};

macro_rules! tuple_codec {
    ($($T:ident),+) => {
        /// Builds an Erlang N-tuple from the encoded elements
        /// (`enif_make_tuple_from_array`, no heap allocation). Fails if any
        /// element fails to encode.
        impl<'id, $($T: Encoder<'id>),+> Encoder<'id> for ($($T,)+) {
            fn encode(&self, env: impl Env<'id>) -> Result<AnyTerm<'id>, CodecError> {
                #[allow(non_snake_case)]
                let ($($T,)+) = self;
                // Build the tuple from a fixed-size stack array of element words
                // and one `enif_make_tuple_from_array` — no heap allocation
                // (`Tuple::from_terms` would `collect` a `Vec`). The array
                // literal infers `[RawTerm; N]` from `raw_term()`.
                let raw = [$( $T.encode(env)?.raw_term() ),+];
                // SAFETY: `env` is live and `raw` holds `raw.len()` contiguous
                // terms of this brand.
                let term = unsafe {
                    enif_ffi::make_tuple_from_array(env.raw_env(), raw.as_ptr(), raw.len() as c_uint)
                };
                Ok(AnyTerm::wrap(term, env))
            }
        }

        /// Requires an Erlang tuple of exactly N elements
        /// ([`WrongArity`](CodecError::WrongArity) otherwise), then decodes each
        /// element in turn; an element's own error propagates.
        impl<'id, $($T: Decoder<'id>),+> Decoder<'id> for ($($T,)+) {
            fn decode(term: AnyTerm<'id>, env: impl Env<'id>) -> Result<Self, CodecError> {
                const ARITY: usize = [$(stringify!($T)),+].len();
                let view = Tuple::decode(term, env)?.with_elements(env);
                if view.len() != ARITY {
                    return Err(CodecError::WrongArity);
                }
                let mut index = 0;
                #[allow(unused_assignments)]
                let result = ($(
                    {
                        let element = $T::decode(view[index], env)?;
                        index += 1;
                        element
                    },
                )+);
                Ok(result)
            }
        }
    };
}

tuple_codec!(A);
tuple_codec!(A, B);
tuple_codec!(A, B, C);
tuple_codec!(A, B, C, D);
tuple_codec!(A, B, C, D, E);
tuple_codec!(A, B, C, D, E, F);
tuple_codec!(A, B, C, D, E, F, G);
tuple_codec!(A, B, C, D, E, F, G, H);
tuple_codec!(A, B, C, D, E, F, G, H, I);
tuple_codec!(A, B, C, D, E, F, G, H, I, J);
tuple_codec!(A, B, C, D, E, F, G, H, I, J, K);
tuple_codec!(A, B, C, D, E, F, G, H, I, J, K, L);
