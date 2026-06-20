use core::marker::PhantomData;

use crate::types::sealed::Sealed;
use crate::types::{Env, Invariant, RawTerm, Term};

/// An Erlang float. Always IEEE 754 double precision (heap-allocated in the
/// BEAM even though the value is always `f64`).
#[derive(Clone, Copy)]
pub struct Float<'id> {
    raw_term: RawTerm,
    _id: Invariant<'id>,
}

impl<'id> Float<'id> {
    #[crate::raw]
    pub(crate) fn from_raw(raw_term: RawTerm) -> Self {
        Self { raw_term, _id: PhantomData }
    }

    /// Construct a float term from an `f64` (`enif_make_double`).
    ///
    /// Returns `None` if `val` is not finite (NaN or infinity), which the BEAM
    /// rejects with `badarg`. The check is done in Rust, so a rejected value
    /// never calls into the BEAM and the env is never left with a pending
    /// exception. A caller that wants a `badarg` can raise one from a `CallEnv`.
    pub fn from_f64(env: impl Env<'id>, val: f64) -> Option<Self> {
        if !val.is_finite() {
            return None;
        }
        let raw_term = unsafe { enif_ffi::make_double(env.raw_env(), val) };
        Some(Float { raw_term, _id: PhantomData })
    }

    /// Read back the `f64` (`enif_get_double`). `env` must carry the same brand
    /// as this term.
    pub fn to_f64(self, env: impl Env<'id>) -> f64 {
        let mut val: f64 = 0.0;
        // get_double fails only on a non-float; a Float is always a validated
        // float term and every Erlang float is an f64, so this cannot fail.
        let ok = unsafe { enif_ffi::get_double(env.raw_env(), self.raw_term, &mut val) };
        assert!(ok != 0, "enif_get_double failed on a validated Float");
        val
    }
}

impl<'id> Sealed for Float<'id> {}

impl<'id> Term<'id> for Float<'id> {
    fn raw_term(self) -> RawTerm {
        self.raw_term
    }
}

impl PartialEq for Float<'_> {
    fn eq(&self, other: &Self) -> bool {
        unsafe { enif_ffi::is_identical(self.raw_term, other.raw_term) != 0 }
    }
}

impl Eq for Float<'_> {}

impl PartialOrd for Float<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Float<'_> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let c = unsafe { enif_ffi::compare(self.raw_term, other.raw_term) };
        c.cmp(&0)
    }
}

impl std::fmt::Debug for Float<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Float")
    }
}
