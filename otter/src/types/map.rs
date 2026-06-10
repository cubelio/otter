use crate::codec::{CodecError, Decoder, Encoder};
use crate::env::Env;
use crate::sys::{NifMapIterator, NifMapIteratorEntry, NifTerm};
use crate::term::{RawTerm, Term, TermIn};

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
    pub fn get(self, key: impl TermIn) -> Option<Term<'a>> {
        let raw =
            unsafe { crate::wrapper::map::get_map_value(self.env.as_ptr(), self.term, key.as_c_arg()) }?;
        Some(RawTerm::new(self.env, raw).resolve())
    }

    /// Return a new map with `key` set to `value` (insert or replace).
    pub fn put(self, key: impl TermIn, value: impl TermIn) -> Map<'a> {
        let term = unsafe {
            crate::wrapper::map::make_map_put(
                self.env.as_ptr(),
                self.term,
                key.as_c_arg(),
                value.as_c_arg(),
            )
        }
        .unwrap();
        Map { term, env: self.env }
    }

    /// Return a new map with `key` updated to `value`.
    ///
    /// Returns `None` if the key is not present (unlike `put`, which inserts).
    pub fn update(self, key: impl TermIn, value: impl TermIn) -> Option<Map<'a>> {
        let term = unsafe {
            crate::wrapper::map::make_map_update(
                self.env.as_ptr(),
                self.term,
                key.as_c_arg(),
                value.as_c_arg(),
            )
        }?;
        Some(Map { term, env: self.env })
    }

    /// Return a new map with `key` removed.
    ///
    /// Returns `None` if the key was not present.
    pub fn remove(self, key: impl TermIn) -> Option<Map<'a>> {
        let term = unsafe {
            crate::wrapper::map::make_map_remove(self.env.as_ptr(), self.term, key.as_c_arg())
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
    type Item = (Term<'a>, Term<'a>);

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
                let key = RawTerm::new(self.env, k).resolve();
                let val = RawTerm::new(self.env, v).resolve();
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
    fn encode<'a>(&self, env: Env<'a>) -> RawTerm<'a> {
        let term = unsafe { crate::wrapper::term::make_copy(env.as_ptr(), self.term) };
        RawTerm::new(env, term)
    }
}

impl<'a> Decoder<'a> for Map<'a> {
    fn decode(term: Term<'a>) -> Result<Self, CodecError> {
        match term {
            Term::Map(m) => Ok(m),
            _ => Err(CodecError::WrongType),
        }
    }
}
