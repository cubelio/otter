//! `Encoder`/`Decoder` for Rust `[T]`/`Vec<T>` <-> Erlang lists.
//!
//! Encoding a slice or `Vec` builds a proper list from the element-wise
//! encodings. Decoding requires a *proper* list — an improper tail yields
//! [`CodecError::WrongType`] — and decodes each element into the `Vec`. An
//! element that fails to encode/decode propagates its own error.

use crate::codec::{CodecError, Decoder, Encoder};
use crate::types::{AnyTerm, Env, List};

impl<'id, T: Encoder<'id>> Encoder<'id> for [T] {
    fn encode(&self, env: impl Env<'id>) -> Result<AnyTerm<'id>, CodecError> {
        let mut elements = Vec::with_capacity(self.len());
        for item in self {
            elements.push(item.encode(env)?);
        }
        List::from_terms(env, elements).encode(env)
    }
}

impl<'id, T: Encoder<'id>> Encoder<'id> for Vec<T> {
    fn encode(&self, env: impl Env<'id>) -> Result<AnyTerm<'id>, CodecError> {
        self.as_slice().encode(env)
    }
}

impl<'id, T: Decoder<'id>> Decoder<'id> for Vec<T> {
    fn decode(term: AnyTerm<'id>, env: impl Env<'id>) -> Result<Self, CodecError> {
        let list = List::decode(term, env)?;
        // A proper list has a known length; an improper list yields None. This
        // both rejects improper lists and sizes the Vec in one traversal.
        let len = list.len(env).ok_or(CodecError::WrongType)?;
        let mut out = Vec::with_capacity(len);
        for head in list.iter(env) {
            out.push(T::decode(head, env)?);
        }
        Ok(out)
    }
}
