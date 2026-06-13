use std::ffi::{c_int, c_uint};

use crate::codec::{CodecError, Decoder, Encoder};
use crate::env::Env;
use crate::sys::NifTerm;
use crate::term::{Term, AsNifTerm};

/// An Erlang tuple.
#[derive(Clone, Copy)]
pub struct Tuple<'a> {
    pub(crate) term: NifTerm,
    pub(crate) env: Env<'a>,
}

impl<'a> Tuple<'a> {
    /// Number of elements (arity) of the tuple.
    pub fn len(self) -> usize {
        self.env.get_tuple(self).map_or(0, |elems| elems.len())
    }

    /// Returns `true` if the tuple has zero elements.
    pub fn is_empty(self) -> bool {
        self.len() == 0
    }

    /// Return the element at zero-based index `i` as an unresolved [`Term`].
    ///
    /// Panics if `i >= self.len()`. The element points into the BEAM heap and
    /// is valid for lifetime `'a`. Call [`Term::resolve`] or a decoder to type
    /// it.
    pub fn element(self, i: usize) -> Term<'a> {
        let elems = self.env.get_tuple(self).unwrap();
        assert!(
            i < elems.len(),
            "Tuple::element index {i} out of bounds (arity {})",
            elems.len()
        );
        Term::new(self.env, elems[i])
    }

    /// Construct a tuple from any iterable of term-like values.
    pub fn from_terms<I, T>(env: Env<'a>, terms: I) -> Tuple<'a>
    where
        I: IntoIterator<Item = T>,
        T: AsNifTerm<'a>,
    {
        env.make_tuple(terms)
    }
}

impl PartialEq for Tuple<'_> {
    fn eq(&self, other: &Self) -> bool {
        unsafe { crate::enif::is_identical(self.term, other.term) != 0 }
    }
}

impl Eq for Tuple<'_> {}

impl PartialOrd for Tuple<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Tuple<'_> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let c = unsafe { crate::enif::compare(self.term, other.term) };
        c.cmp(&0)
    }
}

impl std::fmt::Debug for Tuple<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Tuple")
    }
}

impl<'b> Encoder for Tuple<'b> {
    fn encode<'a>(&self, env: Env<'a>) -> Term<'a> {
        if self.env.as_ptr() == env.as_ptr() {
            Term::new(env, self.term)
        } else {
            env.make_copy(*self)
        }
    }
}

impl<'a> Env<'a> {
    /// Returns `true` if `term` is a tuple (`enif_is_tuple`).
    pub fn is_tuple(self, term: impl AsNifTerm<'a>) -> bool {
        unsafe { crate::enif::is_tuple(self.as_ptr(), term.as_nif_term()) != 0 }
    }

    /// Decompose a tuple into its elements (`enif_get_tuple`).
    ///
    /// Returns `None` if `term` is not a tuple. The returned slice points into
    /// the BEAM heap and is valid for the environment lifetime `'a`.
    pub fn get_tuple(self, term: impl AsNifTerm<'a>) -> Option<&'a [NifTerm]> {
        let mut arity: c_int = 0;
        let mut array: *const NifTerm = std::ptr::null();
        if unsafe {
            crate::enif::get_tuple(self.as_ptr(), term.as_nif_term(), &mut arity, &mut array) != 0
        } {
            // enif_get_tuple may leave `array` null for the empty tuple; never
            // hand a null pointer to from_raw_parts.
            let elems = if arity == 0 {
                &[][..]
            } else {
                unsafe { std::slice::from_raw_parts(array, arity as usize) }
            };
            Some(elems)
        } else {
            None
        }
    }

    /// Construct a tuple from any iterable of term-like values
    /// (`enif_make_tuple_from_array`).
    pub fn make_tuple<I, T>(self, terms: I) -> Tuple<'a>
    where
        I: IntoIterator<Item = T>,
        T: AsNifTerm<'a>,
    {
        let raw: Vec<NifTerm> = terms.into_iter().map(|t| t.as_nif_term()).collect();
        let term = unsafe {
            crate::enif::make_tuple_from_array(self.as_ptr(), raw.as_ptr(), raw.len() as c_uint)
        };
        Tuple { term, env: self }
    }
}

impl<'a> Decoder<'a> for Tuple<'a> {
    fn decode(term: Term<'a>) -> Result<Self, CodecError> {
        if term.env.is_tuple(term) {
            Ok(Tuple { term: term.term, env: term.env })
        } else {
            Err(CodecError::WrongType)
        }
    }
}
