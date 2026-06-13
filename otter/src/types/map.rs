use crate::codec::{CodecError, Decoder, Encoder};
use crate::env::Env;
use crate::sys::{NifMapIterator, NifMapIteratorEntry, NifTerm};
use crate::term::{Term, TypedTerm, AsNifTerm};

/// An Erlang map. Immutable — all mutations return a new map.
#[derive(Clone, Copy)]
pub struct Map<'a> {
    pub(crate) term: NifTerm,
    pub(crate) env: Env<'a>,
}

impl<'a> Map<'a> {
    /// Create an empty map.
    pub fn new(env: Env<'a>) -> Map<'a> {
        env.make_new_map()
    }

    /// Number of key-value pairs in the map.
    pub fn size(self) -> usize {
        self.env.get_map_size(self).unwrap_or(0)
    }

    /// Look up `key`. Returns `None` if the key is absent.
    pub fn get(self, key: impl AsNifTerm<'a>) -> Option<TypedTerm<'a>> {
        self.env.get_map_value(self, key)
    }

    /// Return a new map with `key` set to `value` (insert or replace).
    pub fn put(self, key: impl AsNifTerm<'a>, value: impl AsNifTerm<'a>) -> Map<'a> {
        self.env.make_map_put(self, key, value).unwrap()
    }

    /// Return a new map with `key` updated to `value`.
    ///
    /// Returns `None` if the key is not present (unlike `put`, which inserts).
    pub fn update(self, key: impl AsNifTerm<'a>, value: impl AsNifTerm<'a>) -> Option<Map<'a>> {
        self.env.make_map_update(self, key, value)
    }

    /// Return a new map with `key` removed.
    ///
    /// Returns `None` if the key was not present.
    pub fn remove(self, key: impl AsNifTerm<'a>) -> Option<Map<'a>> {
        self.env.make_map_remove(self, key)
    }

    /// Return an iterator over `(key, value)` pairs in unspecified order.
    pub fn iter(self) -> MapIterator<'a> {
        let mut iter: Box<NifMapIterator> = Box::new(unsafe { std::mem::zeroed() });
        self.env.map_iterator_create(self, &mut iter, NifMapIteratorEntry::First);
        MapIterator { iter, env: self.env, exhausted: false }
    }
}

// ---------------------------------------------------------------------------
// MapIterator
// ---------------------------------------------------------------------------

/// Iterator over the key-value pairs of a `Map`.
///
/// `NifMapIterator` must not move after `map_iterator_create`. The `Box`
/// pins it on the heap for the lifetime of the iterator.
pub struct MapIterator<'a> {
    iter: Box<NifMapIterator>,
    env: Env<'a>,
    exhausted: bool,
}

impl<'a> Iterator for MapIterator<'a> {
    type Item = (TypedTerm<'a>, TypedTerm<'a>);

    fn next(&mut self) -> Option<Self::Item> {
        if self.exhausted {
            return None;
        }
        match self.env.map_iterator_get_pair(&mut self.iter) {
            None => {
                self.exhausted = true;
                None
            }
            Some((k, v)) => {
                // Advance for the next call. Return value is informational only
                // — we rely on get_pair returning None to detect exhaustion.
                self.env.map_iterator_next(&mut self.iter);
                Some((k.resolve(), v.resolve()))
            }
        }
    }
}

impl<'a> Drop for MapIterator<'a> {
    fn drop(&mut self) {
        self.env.map_iterator_destroy(&mut self.iter);
    }
}

impl PartialEq for Map<'_> {
    fn eq(&self, other: &Self) -> bool {
        unsafe { crate::enif::is_identical(self.term, other.term) != 0 }
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
        let c = unsafe { crate::enif::compare(self.term, other.term) };
        c.cmp(&0)
    }
}

impl std::fmt::Debug for Map<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Map")
    }
}

impl<'b> Encoder for Map<'b> {
    fn encode<'a>(&self, env: Env<'a>) -> Term<'a> {
        if self.env.as_ptr() == env.as_ptr() {
            Term::new(env, self.term)
        } else {
            env.make_copy(*self)
        }
    }
}

impl<'a> Env<'a> {
    /// Returns `true` if `term` is a map (`enif_is_map`).
    pub fn is_map(self, term: impl AsNifTerm<'a>) -> bool {
        unsafe { crate::enif::is_map(self.as_ptr(), term.as_nif_term()) != 0 }
    }

    /// Create an empty map (`enif_make_new_map`).
    pub fn make_new_map(self) -> Map<'a> {
        let term = unsafe { crate::enif::make_new_map(self.as_ptr()) };
        Map { term, env: self }
    }

    /// Number of key-value pairs in a map (`enif_get_map_size`).
    /// `None` if `map` is not a map.
    pub fn get_map_size(self, map: impl AsNifTerm<'a>) -> Option<usize> {
        let mut size: usize = 0;
        if unsafe { crate::enif::get_map_size(self.as_ptr(), map.as_nif_term(), &mut size) != 0 } {
            Some(size)
        } else {
            None
        }
    }

    /// Look up `key` in `map` (`enif_get_map_value`). `None` if absent.
    pub fn get_map_value(
        self,
        map: impl AsNifTerm<'a>,
        key: impl AsNifTerm<'a>,
    ) -> Option<TypedTerm<'a>> {
        let mut value: NifTerm = 0;
        if unsafe {
            crate::enif::get_map_value(self.as_ptr(), map.as_nif_term(), key.as_nif_term(), &mut value)
                != 0
        } {
            Some(Term::new(self, value).resolve())
        } else {
            None
        }
    }

    /// Return a new map with `key` set to `value` (`enif_make_map_put`,
    /// insert or replace). `None` if `map` is not a map.
    pub fn make_map_put(
        self,
        map: impl AsNifTerm<'a>,
        key: impl AsNifTerm<'a>,
        value: impl AsNifTerm<'a>,
    ) -> Option<Map<'a>> {
        let mut out: NifTerm = 0;
        if unsafe {
            crate::enif::make_map_put(
                self.as_ptr(),
                map.as_nif_term(),
                key.as_nif_term(),
                value.as_nif_term(),
                &mut out,
            ) != 0
        } {
            Some(Map { term: out, env: self })
        } else {
            None
        }
    }

    /// Return a new map with `key` updated to `value` (`enif_make_map_update`).
    /// `None` if the key is absent or `map` is not a map.
    pub fn make_map_update(
        self,
        map: impl AsNifTerm<'a>,
        key: impl AsNifTerm<'a>,
        value: impl AsNifTerm<'a>,
    ) -> Option<Map<'a>> {
        let mut out: NifTerm = 0;
        if unsafe {
            crate::enif::make_map_update(
                self.as_ptr(),
                map.as_nif_term(),
                key.as_nif_term(),
                value.as_nif_term(),
                &mut out,
            ) != 0
        } {
            Some(Map { term: out, env: self })
        } else {
            None
        }
    }

    /// Return a new map with `key` removed (`enif_make_map_remove`).
    /// `None` if the key was absent or `map` is not a map.
    pub fn make_map_remove(
        self,
        map: impl AsNifTerm<'a>,
        key: impl AsNifTerm<'a>,
    ) -> Option<Map<'a>> {
        let mut out: NifTerm = 0;
        if unsafe {
            crate::enif::make_map_remove(self.as_ptr(), map.as_nif_term(), key.as_nif_term(), &mut out)
                != 0
        } {
            Some(Map { term: out, env: self })
        } else {
            None
        }
    }

    /// Initialise `iter` for iterating over `map` (`enif_map_iterator_create`).
    /// Returns `false` if `map` is not a map. The caller must pair this with
    /// `map_iterator_destroy`.
    pub fn map_iterator_create(
        self,
        map: impl AsNifTerm<'a>,
        iter: &mut NifMapIterator,
        entry: NifMapIteratorEntry,
    ) -> bool {
        unsafe { crate::enif::map_iterator_create(self.as_ptr(), map.as_nif_term(), iter, entry) != 0 }
    }

    /// Destroy a map iterator (`enif_map_iterator_destroy`).
    pub fn map_iterator_destroy(self, iter: &mut NifMapIterator) {
        unsafe { crate::enif::map_iterator_destroy(self.as_ptr(), iter) }
    }

    /// Advance a map iterator (`enif_map_iterator_next`). `false` when
    /// exhausted.
    pub fn map_iterator_next(self, iter: &mut NifMapIterator) -> bool {
        unsafe { crate::enif::map_iterator_next(self.as_ptr(), iter) != 0 }
    }

    /// The current key/value pair of a map iterator
    /// (`enif_map_iterator_get_pair`). `None` if exhausted.
    pub fn map_iterator_get_pair(
        self,
        iter: &mut NifMapIterator,
    ) -> Option<(Term<'a>, Term<'a>)> {
        let mut key: NifTerm = 0;
        let mut value: NifTerm = 0;
        if unsafe {
            crate::enif::map_iterator_get_pair(self.as_ptr(), iter, &mut key, &mut value) != 0
        } {
            Some((Term::new(self, key), Term::new(self, value)))
        } else {
            None
        }
    }
}

impl<'a> Decoder<'a> for Map<'a> {
    fn decode(term: Term<'a>) -> Result<Self, CodecError> {
        if term.env.is_map(term) {
            Ok(Map { term: term.term, env: term.env })
        } else {
            Err(CodecError::WrongType)
        }
    }
}
