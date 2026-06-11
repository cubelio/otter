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
        let term = unsafe { crate::wrapper::map::make_new_map(env.as_ptr()) };
        Map { term, env }
    }

    /// Number of key-value pairs in the map.
    pub fn size(self) -> usize {
        unsafe { crate::wrapper::map::get_map_size(self.env.as_ptr(), self.term) }.unwrap_or(0)
    }

    /// Look up `key`. Returns `None` if the key is absent.
    pub fn get(self, key: impl AsNifTerm<'a>) -> Option<TypedTerm<'a>> {
        let raw =
            unsafe { crate::wrapper::map::get_map_value(self.env.as_ptr(), self.term, key.as_nif_term()) }?;
        Some(Term::new(self.env, raw).resolve())
    }

    /// Return a new map with `key` set to `value` (insert or replace).
    pub fn put(self, key: impl AsNifTerm<'a>, value: impl AsNifTerm<'a>) -> Map<'a> {
        let term = unsafe {
            crate::wrapper::map::make_map_put(
                self.env.as_ptr(),
                self.term,
                key.as_nif_term(),
                value.as_nif_term(),
            )
        }
        .unwrap();
        Map { term, env: self.env }
    }

    /// Return a new map with `key` updated to `value`.
    ///
    /// Returns `None` if the key is not present (unlike `put`, which inserts).
    pub fn update(self, key: impl AsNifTerm<'a>, value: impl AsNifTerm<'a>) -> Option<Map<'a>> {
        let term = unsafe {
            crate::wrapper::map::make_map_update(
                self.env.as_ptr(),
                self.term,
                key.as_nif_term(),
                value.as_nif_term(),
            )
        }?;
        Some(Map { term, env: self.env })
    }

    /// Return a new map with `key` removed.
    ///
    /// Returns `None` if the key was not present.
    pub fn remove(self, key: impl AsNifTerm<'a>) -> Option<Map<'a>> {
        let term = unsafe {
            crate::wrapper::map::make_map_remove(self.env.as_ptr(), self.term, key.as_nif_term())
        }?;
        Some(Map { term, env: self.env })
    }

    /// Return an iterator over `(key, value)` pairs in unspecified order.
    pub fn iter(self) -> MapIterator<'a> {
        let mut iter: Box<NifMapIterator> = Box::new(unsafe { std::mem::zeroed() });
        unsafe {
            crate::wrapper::map::map_iterator_create(
                self.env.as_ptr(),
                self.term,
                &mut *iter,
                NifMapIteratorEntry::First,
            );
        }
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
        let pair = unsafe {
            crate::wrapper::map::map_iterator_get_pair(self.env.as_ptr(), &mut *self.iter)
        };
        match pair {
            None => {
                self.exhausted = true;
                None
            }
            Some((k, v)) => {
                // Advance for the next call. Return value is informational only
                // — we rely on get_pair returning None to detect exhaustion.
                unsafe {
                    crate::wrapper::map::map_iterator_next(self.env.as_ptr(), &mut *self.iter);
                }
                let key = Term::new(self.env, k).resolve();
                let val = Term::new(self.env, v).resolve();
                Some((key, val))
            }
        }
    }
}

impl<'a> Drop for MapIterator<'a> {
    fn drop(&mut self) {
        unsafe {
            crate::wrapper::map::map_iterator_destroy(self.env.as_ptr(), &mut *self.iter);
        }
    }
}

impl PartialEq for Map<'_> {
    fn eq(&self, other: &Self) -> bool {
        unsafe { crate::wrapper::term::is_identical(self.term, other.term) }
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
        let c = unsafe { crate::wrapper::term::compare(self.term, other.term) };
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
        let raw = if self.env.as_ptr() == env.as_ptr() {
            self.term
        } else {
            unsafe { crate::wrapper::term::make_copy(env.as_ptr(), self.term) }
        };
        Term::new(env, raw)
    }
}

impl<'a> Decoder<'a> for Map<'a> {
    fn decode(term: TypedTerm<'a>) -> Result<Self, CodecError> {
        match term {
            TypedTerm::Map(m) => Ok(m),
            _ => Err(CodecError::WrongType),
        }
    }
}
