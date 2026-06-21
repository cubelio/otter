//! `Encoder`/`Decoder` for `std::collections::HashMap` <-> Erlang maps.
//!
//! Encoding builds the map in one `enif_make_map_from_arrays` from the encoded
//! keys and values (falling back to incremental insertion only if the encoded
//! keys collide). Decoding iterates the map's pairs and decodes each into the
//! `HashMap`. Both
//! sides are generic over the hasher `S`, so a non-default `BuildHasher` works.
//! Keys and values that fail to encode/decode propagate their own error.

use std::collections::HashMap;
use std::hash::{BuildHasher, Hash};

use crate::codec::{CodecError, Decoder, Encoder};
use crate::types::{AnyTerm, Env, Map, RawTerm, Term};

impl<'id, K: Encoder<'id>, V: Encoder<'id>, S> Encoder<'id> for HashMap<K, V, S> {
    fn encode(&self, env: impl Env<'id>) -> Result<AnyTerm<'id>, CodecError> {
        // Encode keys and values into parallel arrays and build the map in one
        // `enif_make_map_from_arrays`, rather than threading N intermediate map
        // terms through `enif_make_map_put`.
        let mut keys: Vec<RawTerm> = Vec::with_capacity(self.len());
        let mut vals: Vec<RawTerm> = Vec::with_capacity(self.len());
        for (key, value) in self {
            keys.push(key.encode(env)?.raw_term());
            vals.push(value.encode(env)?.raw_term());
        }
        let mut out: RawTerm = 0;
        // SAFETY: `env` is live; `keys`/`vals` are equal-length contiguous arrays
        // of `keys.len()` terms of this brand; `out` receives the map on success.
        let ok = unsafe {
            enif_ffi::make_map_from_arrays(env.raw_env(), keys.as_ptr(), vals.as_ptr(), keys.len(), &mut out)
        };
        if ok != 0 {
            return Ok(AnyTerm::wrap(out, env));
        }
        // `make_map_from_arrays` returns 0 only on duplicate keys. Distinct Rust
        // keys can collide as Erlang terms only with a non-injective `Encoder`;
        // fall back to incremental `put` (last write wins), reusing the terms we
        // already encoded.
        let mut map = Map::new(env);
        for (k, v) in keys.iter().zip(&vals) {
            map = map.put(env, AnyTerm::wrap(*k, env), AnyTerm::wrap(*v, env));
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
