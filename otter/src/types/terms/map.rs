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
    /// Create an empty map (`enif_make_new_map`).
    pub fn new(env: impl Env<'id>) -> Map<'id> {
        let raw_term = unsafe { enif_ffi::make_new_map(env.raw_env()) };
        Map { raw_term, _id: PhantomData }
    }

    /// Number of key-value pairs (`enif_get_map_size`).
    pub fn size(self, env: impl Env<'id>) -> usize {
        let mut size: usize = 0;
        if unsafe { enif_ffi::get_map_size(env.raw_env(), self.raw_term, &mut size) } != 0 {
            size
        } else {
            0
        }
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

    /// Return a new map with `key` removed (`enif_make_map_remove`).
    /// `None` if the key was absent.
    pub fn remove(self, env: impl Env<'id>, key: impl Term<'id>) -> Option<Map<'id>> {
        let mut out: RawTerm = 0;
        (unsafe {
            enif_ffi::make_map_remove(env.raw_env(), self.raw_term, key.raw_term(), &mut out)
        } != 0)
            .then_some(Map { raw_term: out, _id: PhantomData })
    }

    /// Returns `true` if `term` is a map (`enif_is_map`).
    pub fn is_map(env: impl Env<'id>, term: impl Term<'id>) -> bool {
        unsafe { enif_ffi::is_map(env.raw_env(), term.raw_term()) != 0 }
    }

    /// Iterate `(key, value)` pairs in unspecified order.
    pub fn iter(self, env: impl Env<'id>) -> MapIterator<'id> {
        let mut iter: Box<enif_ffi::MapIterator> = Box::new(unsafe { std::mem::zeroed() });
        unsafe {
            enif_ffi::map_iterator_create(
                env.raw_env(),
                self.raw_term,
                &mut *iter,
                enif_ffi::MapIteratorEntry::First,
            )
        };
        MapIterator { iter, env: env.as_any_env(), exhausted: false }
    }
}

/// Iterator over the key-value pairs of a [`Map`].
///
/// `enif_ffi::MapIterator` must not move after creation; the `Box` pins it for
/// the iterator's lifetime.
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
