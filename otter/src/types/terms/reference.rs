use core::marker::PhantomData;

use crate::types::sealed::Sealed;
use crate::types::{Env, Invariant, RawTerm, Term};

/// An Erlang reference.
#[derive(Clone, Copy)]
pub struct Reference<'id> {
    raw_term: RawTerm,
    _id: Invariant<'id>,
}

impl<'id> Reference<'id> {
    /// Create a new unique reference (`enif_make_ref`).
    pub fn new(env: impl Env<'id>) -> Self {
        let raw_term = unsafe { enif_ffi::make_ref(env.raw_env()) };
        Reference { raw_term, _id: PhantomData }
    }

    /// Returns `true` if `term` is a reference (`enif_is_ref`).
    pub fn is_ref(env: impl Env<'id>, term: impl Term<'id>) -> bool {
        unsafe { enif_ffi::is_ref(env.raw_env(), term.raw_term()) != 0 }
    }
}

impl<'id> Sealed for Reference<'id> {}

impl<'id> Term<'id> for Reference<'id> {
    fn raw_term(self) -> RawTerm {
        self.raw_term
    }
}

impl PartialEq for Reference<'_> {
    fn eq(&self, other: &Self) -> bool {
        unsafe { enif_ffi::is_identical(self.raw_term, other.raw_term) != 0 }
    }
}

impl Eq for Reference<'_> {}

impl PartialOrd for Reference<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Reference<'_> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let c = unsafe { enif_ffi::compare(self.raw_term, other.raw_term) };
        c.cmp(&0)
    }
}

impl std::fmt::Debug for Reference<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Reference")
    }
}
