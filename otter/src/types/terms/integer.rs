use core::marker::PhantomData;

use crate::types::sealed::Sealed;
use crate::types::{Env, Invariant, RawTerm, Term};

/// An Erlang integer. Arbitrary precision — small integers are tagged
/// immediates, large integers (bignums) are heap-allocated on the env.
///
/// Carries only its env's brand `'id`, not the env itself. An integer can be
/// read back only with an env of the same brand (the 1:1 identity guarantee),
/// so the accessors take the env explicitly rather than the term carrying it.
#[derive(Clone, Copy)]
pub struct Integer<'id> {
    raw_term: RawTerm,
    _id: Invariant<'id>,
}

impl<'id> Integer<'id> {
    pub(crate) fn from_raw(raw_term: RawTerm) -> Self {
        Self { raw_term, _id: PhantomData }
    }

    /// Construct an integer term from an `i64` (`enif_make_int64`).
    pub fn from_i64(env: impl Env<'id>, val: i64) -> Self {
        let raw_term = unsafe { enif_ffi::make_int64(env.raw_env(), val) };
        Integer { raw_term, _id: PhantomData }
    }

    /// Construct an integer term from a `u64` (`enif_make_uint64`).
    pub fn from_u64(env: impl Env<'id>, val: u64) -> Self {
        let raw_term = unsafe { enif_ffi::make_uint64(env.raw_env(), val) };
        Integer { raw_term, _id: PhantomData }
    }

    /// Read back an `i64` (`enif_get_int64`). `None` if the term does not fit in
    /// `i64`. `env` must carry the same brand as this term.
    pub fn to_i64(self, env: impl Env<'id>) -> Option<i64> {
        let mut val: i64 = 0;
        (unsafe { enif_ffi::get_int64(env.raw_env(), self.raw_term, &mut val) } != 0).then_some(val)
    }

    /// Read back a `u64` (`enif_get_uint64`). `None` if the term does not fit in
    /// `u64` (including negatives).
    pub fn to_u64(self, env: impl Env<'id>) -> Option<u64> {
        let mut val: u64 = 0;
        (unsafe { enif_ffi::get_uint64(env.raw_env(), self.raw_term, &mut val) } != 0).then_some(val)
    }

    /// Read back an `i128`, covering the combined `i64`/`u64` range. The NIF API
    /// has no 128-bit accessor, so values in `i64::MIN..=i64::MAX` take the
    /// signed path and `i64::MAX+1..=u64::MAX` the unsigned path; anything
    /// outside that range is `None`.
    pub fn to_i128(self, env: impl Env<'id>) -> Option<i128> {
        if let Some(val) = self.to_i64(env) {
            return Some(val as i128);
        }
        self.to_u64(env).map(|val| val as i128)
    }
}

impl<'id> Sealed for Integer<'id> {}

impl<'id> Term<'id> for Integer<'id> {
    fn raw_term(self) -> RawTerm {
        self.raw_term
    }
}

impl PartialEq for Integer<'_> {
    fn eq(&self, other: &Self) -> bool {
        unsafe { enif_ffi::is_identical(self.raw_term, other.raw_term) != 0 }
    }
}

impl Eq for Integer<'_> {}

impl PartialOrd for Integer<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Integer<'_> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let c = unsafe { enif_ffi::compare(self.raw_term, other.raw_term) };
        c.cmp(&0)
    }
}

impl std::fmt::Debug for Integer<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Integer")
    }
}
