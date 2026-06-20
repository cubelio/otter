use core::marker::PhantomData;
use std::ffi::{c_int, c_uint};

use crate::types::sealed::Sealed;
use crate::types::{AnyTerm, Env, Invariant, RawTerm, Term};

/// An Erlang tuple.
#[derive(Clone, Copy)]
pub struct Tuple<'id> {
    raw_term: RawTerm,
    _id: Invariant<'id>,
}

impl<'id> Tuple<'id> {
    /// Number of elements (arity) of the tuple.
    pub fn len(self, env: impl Env<'id>) -> usize {
        self.get(env).map_or(0, |elems| elems.len())
    }

    /// Returns `true` if the tuple has zero elements.
    pub fn is_empty(self, env: impl Env<'id>) -> bool {
        self.len(env) == 0
    }

    /// Return the element at zero-based index `i` as an unresolved [`AnyTerm`].
    ///
    /// Panics if `i >= self.len()`. The element shares this tuple's brand.
    pub fn element(self, env: impl Env<'id>, i: usize) -> AnyTerm<'id> {
        let elems = self.get(env).expect("Tuple::element on a non-tuple term");
        assert!(
            i < elems.len(),
            "Tuple::element index {i} out of bounds (arity {})",
            elems.len()
        );
        AnyTerm::wrap(elems[i], env)
    }

    /// Construct a tuple from any iterable of terms of this brand
    /// (`enif_make_tuple_from_array`).
    pub fn from_terms<I, T>(env: impl Env<'id>, terms: I) -> Tuple<'id>
    where
        I: IntoIterator<Item = T>,
        T: Term<'id>,
    {
        let raw: Vec<RawTerm> = terms.into_iter().map(|t| t.raw_term()).collect();
        let raw_term = unsafe {
            enif_ffi::make_tuple_from_array(env.raw_env(), raw.as_ptr(), raw.len() as c_uint)
        };
        Tuple { raw_term, _id: PhantomData }
    }

    /// Returns `true` if `term` is a tuple (`enif_is_tuple`).
    pub fn is_tuple(env: impl Env<'id>, term: impl Term<'id>) -> bool {
        unsafe { enif_ffi::is_tuple(env.raw_env(), term.raw_term()) != 0 }
    }

    /// The tuple's elements as a slice into the BEAM heap (`enif_get_tuple`).
    /// `None` if this term is not a tuple. The slice rides this tuple's brand
    /// `'id`, which cannot escape its env's scope, so the borrow is sound.
    fn get(self, env: impl Env<'id>) -> Option<&'id [RawTerm]> {
        let mut arity: c_int = 0;
        let mut array: *const RawTerm = std::ptr::null();
        if unsafe { enif_ffi::get_tuple(env.raw_env(), self.raw_term, &mut arity, &mut array) } == 0 {
            return None;
        }
        // enif_get_tuple may leave `array` null for the empty tuple; never hand
        // a null pointer to from_raw_parts.
        Some(if arity == 0 {
            &[]
        } else {
            unsafe { std::slice::from_raw_parts(array, arity as usize) }
        })
    }
}

impl PartialEq for Tuple<'_> {
    fn eq(&self, other: &Self) -> bool {
        unsafe { enif_ffi::is_identical(self.raw_term, other.raw_term) != 0 }
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
        let c = unsafe { enif_ffi::compare(self.raw_term, other.raw_term) };
        c.cmp(&0)
    }
}

impl std::fmt::Debug for Tuple<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Tuple")
    }
}

impl<'id> Sealed for Tuple<'id> {}

impl<'id> Term<'id> for Tuple<'id> {
    fn raw_term(self) -> RawTerm {
        self.raw_term
    }
}
