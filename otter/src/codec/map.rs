//! `Encoder`/`Decoder` for `std::collections::HashMap` <-> Erlang maps.
//!
//! Encoding inserts each entry's encoded key and value into a fresh map.
//! Decoding iterates the map's pairs and decodes each into the `HashMap`. Both
//! sides are generic over the hasher `S`, so a non-default `BuildHasher` works.
//! Keys and values that fail to encode/decode propagate their own error.

use std::collections::HashMap;
use std::hash::{BuildHasher, Hash};

use crate::codec::{CodecError, Decoder, Encoder};
use crate::types::{AnyTerm, Env, Map};

impl<'id, K: Encoder<'id>, V: Encoder<'id>, S> Encoder<'id> for HashMap<K, V, S> {
    fn encode(&self, env: impl Env<'id>) -> Result<AnyTerm<'id>, CodecError> {
        let mut map = Map::new(env);
        for (key, value) in self {
            map = map.put(env, key.encode(env)?, value.encode(env)?);
        }
        map.encode(env)
    }
}

impl<'id, K, V, S> Decoder<'id> for HashMap<K, V, S>
where
    K: Decoder<'id> + Eq + Hash,
    V: Decoder<'id>,
    S: BuildHasher + Default,
{
    fn decode(term: AnyTerm<'id>, env: impl Env<'id>) -> Result<Self, CodecError> {
        let map = Map::decode(term, env)?;
        let mut out = HashMap::with_capacity_and_hasher(map.size(env), S::default());
        for (key, value) in map.iter(env) {
            out.insert(K::decode(key, env)?, V::decode(value, env)?);
        }
        Ok(out)
    }
}
