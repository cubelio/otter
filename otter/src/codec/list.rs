//! `Encoder`/`Decoder` for Rust `[T]`/`Vec<T>` <-> Erlang lists.
//!
//! Encoding a slice or `Vec` builds a proper list from the element-wise
//! encodings. Decoding requires a *proper* list — an improper tail yields
//! [`CodecError::WrongType`] — and decodes each element into the `Vec`. An
//! element that fails to encode/decode propagates its own error.

use std::ffi::c_uint;

use crate::codec::{CodecError, Decoder, Encoder};
use crate::types::{AnyTerm, Env, List, RawTerm, Term};

/// Builds a proper Erlang list from the element-wise encodings in one
/// `enif_make_list_from_array`. Fails if any element fails to encode.
impl<'id, T: Encoder<'id>> Encoder<'id> for [T] {
    fn encode(&self, env: impl Env<'id>) -> Result<AnyTerm<'id>, CodecError> {
        // Collect the encoded element words directly and build the list in one
        // `enif_make_list_from_array`. Going through `List::from_terms` would
        // re-collect a second `Vec` of the same words, so we inline it here to
        // keep a single allocation for the whole list.
        let mut raw: Vec<RawTerm> = Vec::with_capacity(self.len());
        for item in self {
            raw.push(item.encode(env)?.raw_term());
        }
        // SAFETY: `env` is live for this call, and `raw` holds `raw.len()`
        // contiguous terms of this brand.
        let term =
            unsafe { enif_ffi::make_list_from_array(env.raw_env(), raw.as_ptr(), raw.len() as c_uint) };
        Ok(AnyTerm::wrap(term, env))
    }
}

/// Encodes as a proper list, via the `[T]` impl.
impl<'id, T: Encoder<'id>> Encoder<'id> for Vec<T> {
    fn encode(&self, env: impl Env<'id>) -> Result<AnyTerm<'id>, CodecError> {
        self.as_slice().encode(env)
    }
}

/// Requires a *proper* list, decoding each element into the `Vec`. An improper
/// tail is [`WrongType`](CodecError::WrongType); an element's own error
/// propagates.
impl<'id, T: Decoder<'id>> Decoder<'id> for Vec<T> {
    fn decode(term: AnyTerm<'id>, env: impl Env<'id>) -> Result<Self, CodecError> {
        let list = List::decode(term, env)?;
        // Single traversal: walk the cons cells with `enif_get_list_cell`,
        // decoding each head, then confirm the terminal is `[]` — an improper
        // list is rejected. (The old path additionally walked the whole list up
        // front via `enif_get_list_length` just to size the `Vec`.)
        let mut out = Vec::new();
        let mut iter = list.iter(env);
        for head in &mut iter {
            out.push(T::decode(head, env)?);
        }
        let tail = iter.tail().expect("ListIterator yields its terminal once exhausted");
        // SAFETY: `env` is live and `tail` is a term of this brand.
        if unsafe { enif_ffi::is_empty_list(env.raw_env(), tail.raw_term()) } != 0 {
            Ok(out)
        } else {
            Err(CodecError::WrongType)
        }
    }
}
