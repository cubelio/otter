//! `Encoder`/`Decoder` for `bool` <-> the atoms `true`/`false`.
//!
//! Erlang has no boolean type; `true` and `false` are ordinary atoms, always
//! present in the global atom table. Encoding yields the matching atom; decoding
//! accepts exactly those two atoms and rejects every other term (including other
//! atoms) as [`CodecError::WrongType`].

use crate::codec::{CodecError, Decoder, Encoder};
use crate::types::{AnyTerm, Atom, Env};

impl<'id> Encoder<'id> for bool {
    fn encode(&self, env: impl Env<'id>) -> Result<AnyTerm<'id>, CodecError> {
        let name = if *self { "true" } else { "false" };
        // `true`/`false` are reserved atoms, always interned, so the lookup
        // never fails.
        Atom::try_existing(env, name).expect("true/false atoms are always present").encode(env)
    }
}

impl<'id> Decoder<'id> for bool {
    fn decode(term: AnyTerm<'id>, env: impl Env<'id>) -> Result<Self, CodecError> {
        let atom = Atom::decode(term, env)?;
        if atom == Atom::try_existing(env, "true").expect("true atom is always present") {
            Ok(true)
        } else if atom == Atom::try_existing(env, "false").expect("false atom is always present") {
            Ok(false)
        } else {
            Err(CodecError::WrongType)
        }
    }
}
