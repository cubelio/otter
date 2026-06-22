//! `Encoder`/`Decoder` for Rust `str`/`String`.
//!
//! Asymmetric, matching how strings actually travel in Erlang. Decoding a
//! `String` accepts either a binary (read as UTF-8) or a charlist (a list of
//! codepoints); encoding a `str`/`String` always produces a binary, the modern
//! convention. A binary whose bytes are not valid UTF-8, or a list that is not a
//! valid string, is rejected with [`CodecError::NotUtf8`]; a term that is neither
//! a binary nor a list is [`CodecError::WrongType`].

use crate::codec::{CodecError, Decoder, Encoder};
use crate::types::{AnyTerm, Binary, Env, List};

/// Encodes the UTF-8 bytes as a binary (`enif_make_new_binary`), the modern
/// convention. Infallible.
impl<'id> Encoder<'id> for str {
    fn encode(&self, env: impl Env<'id>) -> Result<AnyTerm<'id>, CodecError> {
        Binary::from_bytes(env, self.as_bytes()).encode(env)
    }
}

/// Encodes as a binary, via [`str`]'s impl. Infallible.
impl<'id> Encoder<'id> for String {
    fn encode(&self, env: impl Env<'id>) -> Result<AnyTerm<'id>, CodecError> {
        self.as_str().encode(env)
    }
}

/// Accepts a binary (read as UTF-8) or a charlist (list of codepoints). Invalid
/// UTF-8 / a non-string list is [`NotUtf8`](CodecError::NotUtf8); a term that is
/// neither binary nor list is [`WrongType`](CodecError::WrongType).
impl<'id> Decoder<'id> for String {
    fn decode(term: AnyTerm<'id>, env: impl Env<'id>) -> Result<Self, CodecError> {
        if let Ok(bin) = Binary::decode(term, env) {
            return bin.try_str(env).map(str::to_owned).map_err(|_| CodecError::NotUtf8);
        }
        if let Ok(list) = List::decode(term, env) {
            return list.try_string(env).ok_or(CodecError::NotUtf8);
        }
        Err(CodecError::WrongType)
    }
}
