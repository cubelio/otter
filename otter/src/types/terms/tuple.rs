use core::marker::PhantomData;
use std::ffi::{c_int, c_uint};

use crate::types::sealed::Sealed;
use crate::types::{AnyTerm, Env, Invariant, RawTerm, Term};

/// An Erlang tuple.
///
/// The lean form: a one-word term handle, like every other otter term. Reading
/// its elements is an explicit step — [`with_elements`](Tuple::with_elements)
/// performs the single `enif_get_tuple` and yields a [`TupleView`], the
/// collection-like form. Keeping the two separate means a tuple that is only
/// passed through (matched in `TypedTerm`, re-encoded, compared) never pays for
/// the element fetch.
#[derive(Clone, Copy)]
pub struct Tuple<'id> {
    raw_term: RawTerm,
    _id: Invariant<'id>,
}

impl<'id> Tuple<'id> {
    pub(crate) fn from_raw(raw_term: RawTerm) -> Self {
        Self { raw_term, _id: PhantomData }
    }

    /// Resolve the tuple's elements (one `enif_get_tuple`) into a [`TupleView`].
    ///
    /// A `Tuple` is only ever built from a term already confirmed to be a tuple,
    /// so `enif_get_tuple` cannot fail — a zero return is an internal contract
    /// violation, not a user-input case, hence the assert.
    pub fn with_elements(self, env: impl Env<'id>) -> TupleView<'id> {
        let mut arity: c_int = 0;
        let mut array: *const RawTerm = std::ptr::null();
        let ok = unsafe { enif_ffi::get_tuple(env.raw_env(), self.raw_term, &mut arity, &mut array) };
        assert!(ok != 0, "enif_get_tuple failed on a validated Tuple");
        // enif_get_tuple may leave `array` null for the empty tuple; never hand
        // a null pointer to from_raw_parts.
        let raw_elements = if arity == 0 {
            &[][..]
        } else {
            unsafe { std::slice::from_raw_parts(array, arity as usize) }
        };
        TupleView { raw_term: self.raw_term, raw_elements, _id: PhantomData }
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

// ---------------------------------------------------------------------------
// TupleView — a tuple with its elements resolved
// ---------------------------------------------------------------------------

/// A [`Tuple`] whose elements have been resolved, produced by
/// [`Tuple::with_elements`].
///
/// It is still a tuple [`Term`] (it carries the original term word, so it
/// encodes and compares like one), and additionally behaves as a fixed-size
/// collection of its elements: [`len`](Self::len)/[`is_empty`](Self::is_empty),
/// `tuple[i]` indexing, and iteration, all yielding [`AnyTerm<'id>`] — the
/// unresolved element terms. `enif_get_tuple` returns a pointer into the boxed
/// tuple's own heap storage (it hands back `tuple_val(t)+1`, confirmed in the
/// ERTS source); the process heap holds that at a fixed address for the
/// duration of a NIF call (no GC mid-NIF), so the cached slice is valid for
/// this view's brand `'id` and the type stays `Copy`.
#[derive(Clone, Copy)]
pub struct TupleView<'id> {
    raw_term: RawTerm,
    raw_elements: &'id [RawTerm],
    _id: Invariant<'id>,
}

impl<'id> TupleView<'id> {
    /// The elements as a slice of [`AnyTerm`]. Zero-cost: `AnyTerm` is
    /// `#[repr(transparent)]` over `RawTerm`, so the stored raw-word slice is
    /// reinterpreted in place under this view's brand.
    fn elements(self) -> &'id [AnyTerm<'id>] {
        // SAFETY: AnyTerm<'id> is repr(transparent) over RawTerm and carries
        // only a ZST brand marker, so [RawTerm] and [AnyTerm<'id>] are layout-
        // identical; every raw word is a valid term of this tuple's brand.
        unsafe { std::mem::transmute::<&'id [RawTerm], &'id [AnyTerm<'id>]>(self.raw_elements) }
    }

    /// Number of elements (arity) of the tuple.
    pub fn len(self) -> usize {
        self.raw_elements.len()
    }

    /// Returns `true` if the tuple has zero elements.
    pub fn is_empty(self) -> bool {
        self.raw_elements.is_empty()
    }
}

impl<'id> std::ops::Index<usize> for TupleView<'id> {
    type Output = AnyTerm<'id>;

    /// The element at zero-based index `i`. Panics if `i >= self.len()`.
    fn index(&self, i: usize) -> &AnyTerm<'id> {
        &self.elements()[i]
    }
}

impl<'id> IntoIterator for TupleView<'id> {
    type Item = AnyTerm<'id>;
    type IntoIter = std::iter::Copied<std::slice::Iter<'id, AnyTerm<'id>>>;

    fn into_iter(self) -> Self::IntoIter {
        self.elements().iter().copied()
    }
}

impl std::fmt::Debug for TupleView<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TupleView")
    }
}

impl<'id> Sealed for TupleView<'id> {}

impl<'id> Term<'id> for TupleView<'id> {
    fn raw_term(self) -> RawTerm {
        self.raw_term
    }
}
