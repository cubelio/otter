use core::marker::PhantomData;

use crate::types::sealed::Sealed;
use crate::types::{AnyEnv, AnyTerm, Env, Invariant, RawTerm, Term};

/// An Erlang map. Immutable — all mutations return a new map.
#[derive(Clone, Copy)]
pub struct Map<'id> {
    raw_term: RawTerm,
    _id: Invariant<'id>,
}

impl<'id> Map<'id> {
    pub(crate) fn from_raw(raw_term: RawTerm) -> Self {
        Self { raw_term, _id: PhantomData }
    }

    /// Create an empty map (`enif_make_new_map`).
    pub fn new(env: impl Env<'id>) -> Map<'id> {
        let raw_term = unsafe { enif_ffi::make_new_map(env.raw_env()) };
        Map { raw_term, _id: PhantomData }
    }

    /// Number of key-value pairs (`enif_get_map_size`).
    pub fn size(self, env: impl Env<'id>) -> usize {
        let mut size: usize = 0;
        // get_map_size fails only on a non-map; a Map is always a validated map
        // term, so this cannot fail (and 0 is a real size — never fake it).
        let ok = unsafe { enif_ffi::get_map_size(env.raw_env(), self.raw_term, &mut size) };
        assert!(ok != 0, "enif_get_map_size failed on a validated Map");
        size
    }

    /// Look up `key` (`enif_get_map_value`). `None` if absent.
    pub fn get(self, env: impl Env<'id>, key: impl Term<'id>) -> Option<AnyTerm<'id>> {
        let mut value: RawTerm = 0;
        (unsafe {
            enif_ffi::get_map_value(env.raw_env(), self.raw_term, key.raw_term(), &mut value)
        } != 0)
            .then(|| AnyTerm::wrap(value, env))
    }

    /// Return a new map with `key` set to `value` (`enif_make_map_put`, insert
    /// or replace).
    pub fn put(self, env: impl Env<'id>, key: impl Term<'id>, value: impl Term<'id>) -> Map<'id> {
        let mut out: RawTerm = 0;
        let ok = unsafe {
            enif_ffi::make_map_put(env.raw_env(), self.raw_term, key.raw_term(), value.raw_term(), &mut out)
        };
        assert!(ok != 0, "make_map_put on a valid map failed");
        Map { raw_term: out, _id: PhantomData }
    }

    /// Return a new map with `key` updated to `value` (`enif_make_map_update`).
    /// `None` if the key is absent.
    pub fn update(
        self,
        env: impl Env<'id>,
        key: impl Term<'id>,
        value: impl Term<'id>,
    ) -> Option<Map<'id>> {
        let mut out: RawTerm = 0;
        (unsafe {
            enif_ffi::make_map_update(env.raw_env(), self.raw_term, key.raw_term(), value.raw_term(), &mut out)
        } != 0)
            .then_some(Map { raw_term: out, _id: PhantomData })
    }

    /// Return a new map with `key` removed (`enif_make_map_remove`). Removing a
    /// key the map does not contain returns it unchanged.
    pub fn remove(self, env: impl Env<'id>, key: impl Term<'id>) -> Map<'id> {
        let mut out: RawTerm = 0;
        // make_map_remove fails only on a non-map (an absent key yields the map
        // unchanged, not a failure), so this cannot fail on a validated Map.
        let ok = unsafe {
            enif_ffi::make_map_remove(env.raw_env(), self.raw_term, key.raw_term(), &mut out)
        };
        assert!(ok != 0, "make_map_remove on a valid map failed");
        Map { raw_term: out, _id: PhantomData }
    }

    /// Returns `true` if `term` is a map (`enif_is_map`).
    pub fn is_map(env: impl Env<'id>, term: impl Term<'id>) -> bool {
        unsafe { enif_ffi::is_map(env.raw_env(), term.raw_term()) != 0 }
    }

    /// Iterate `(key, value)` pairs in unspecified order.
    pub fn iter(self, env: impl Env<'id>) -> MapIterator<'id> {
        let mut iter: Box<enif_ffi::MapIterator> = Box::new(unsafe { std::mem::zeroed() });
        // create fails only on a non-map; on a validated Map it cannot fail.
        // Proceeding on a failed create would leave the iterator zeroed and the
        // first get_pair would read an uninitialized cursor.
        let ok = unsafe {
            enif_ffi::map_iterator_create(
                env.raw_env(),
                self.raw_term,
                &mut *iter,
                enif_ffi::MapIteratorEntry::First,
            )
        };
        assert!(ok != 0, "enif_map_iterator_create failed on a validated Map");
        MapIterator { iter, env: env.as_any_env(), exhausted: false }
    }
}

/// Iterator over the key-value pairs of a [`Map`].
///
/// Ownership/mutation model (verified against ERTS, `erl_nif.c`
/// `enif_map_iterator_*`, OTP 26 and 27):
///
/// - A flatmap iterator owns nothing — `ks`/`vs` are interior pointers into the
///   map term's heap arrays; `create` is pure setup and `destroy` is a no-op.
/// - A hashmap iterator owns one heap block: `create` does
///   `erts_alloc(ErtsDynamicWStack)` and `destroy` frees it (plus a second,
///   grown buffer if the traversal stack outgrew its inline default).
/// - `next`/`prev` mutate the cursor and the shared work-stack **in place**.
///
/// The load-bearing invariant is therefore **exactly one owner ever calls
/// `destroy`**: a bitwise copy would share the `wstack` pointer and double-free
/// on the second drop (and corrupt the shared stack via independent `next`).
/// That is why `MapIterator` is non-`Copy` and owns the single `Drop`.
///
/// The struct holds **no** self-references (every pointer targets the
/// separately-allocated wstack or the map's own heap, never `&iter`), so moving
/// it is sound on the targeted releases and the `Box` is not strictly required
/// today. It is kept as a forward-compat pin: the struct is documented "all
/// fields internal and may change" with reserved `__spare__[2]` slots, so a
/// future ERTS could introduce a self-referential field that would make moves
/// unsound. See issue robust-12 for the full investigation.
pub struct MapIterator<'id> {
    iter: Box<enif_ffi::MapIterator>,
    env: AnyEnv<'id>,
    exhausted: bool,
}

impl<'id> Iterator for MapIterator<'id> {
    type Item = (AnyTerm<'id>, AnyTerm<'id>);

    fn next(&mut self) -> Option<Self::Item> {
        if self.exhausted {
            return None;
        }
        let mut key: RawTerm = 0;
        let mut value: RawTerm = 0;
        if unsafe {
            enif_ffi::map_iterator_get_pair(self.env.raw_env(), &mut *self.iter, &mut key, &mut value)
        } != 0
        {
            // Advance for the next call; exhaustion is detected by get_pair.
            unsafe { enif_ffi::map_iterator_next(self.env.raw_env(), &mut *self.iter) };
            Some((AnyTerm::wrap(key, self.env), AnyTerm::wrap(value, self.env)))
        } else {
            self.exhausted = true;
            None
        }
    }
}

impl Drop for MapIterator<'_> {
    fn drop(&mut self) {
        unsafe { enif_ffi::map_iterator_destroy(self.env.raw_env(), &mut *self.iter) };
    }
}

impl PartialEq for Map<'_> {
    fn eq(&self, other: &Self) -> bool {
        unsafe { enif_ffi::is_identical(self.raw_term, other.raw_term) != 0 }
    }
}

impl Eq for Map<'_> {}

impl PartialOrd for Map<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Map<'_> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let c = unsafe { enif_ffi::compare(self.raw_term, other.raw_term) };
        c.cmp(&0)
    }
}

impl std::fmt::Debug for Map<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Map")
    }
}

impl<'id> Sealed for Map<'id> {}

impl<'id> Term<'id> for Map<'id> {
    fn raw_term(self) -> RawTerm {
        self.raw_term
    }
}
