use core::marker::PhantomData;

use crate::types::sealed::Sealed;
use crate::types::{Env, Invariant, RawTerm, Term};

/// An Erlang fun (closure or function reference).
///
/// The NIF API provides no inspection of fun contents. A `Fun` can only be
/// held and passed back to Erlang, or used as an argument to `apply`.
#[derive(Clone, Copy)]
pub struct Fun<'id> {
    raw_term: RawTerm,
    _id: Invariant<'id>,
}

impl<'id> Fun<'id> {
    pub(crate) fn from_raw(raw_term: RawTerm) -> Self {
        Self { raw_term, _id: PhantomData }
    }

    /// Returns `true` if `term` is a fun (`enif_is_fun`).
    pub fn is_fun(env: impl Env<'id>, term: impl Term<'id>) -> bool {
        unsafe { enif_ffi::is_fun(env.raw_env(), term.raw_term()) != 0 }
    }
}

impl<'id> Sealed for Fun<'id> {}

impl<'id> Term<'id> for Fun<'id> {
    fn raw_term(self) -> RawTerm {
        self.raw_term
    }
}

impl PartialEq for Fun<'_> {
    fn eq(&self, other: &Self) -> bool {
        unsafe { enif_ffi::is_identical(self.raw_term, other.raw_term) != 0 }
    }
}

impl Eq for Fun<'_> {}

impl PartialOrd for Fun<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Fun<'_> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let c = unsafe { enif_ffi::compare(self.raw_term, other.raw_term) };
        c.cmp(&0)
    }
}

impl std::fmt::Debug for Fun<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Fun")
    }
}
