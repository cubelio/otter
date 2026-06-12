use crate::codec::{CodecError, Decoder, Encoder};
use crate::env::Env;
use crate::sys::{NifTerm, NifTermType};
use crate::term::{Term, AsNifTerm};

/// An Erlang integer. Arbitrary precision — small integers are tagged
/// immediates, large integers (bignums) are heap-allocated.
///
/// The lifetime `'a` covers the bignum case.
#[derive(Clone, Copy)]
pub struct Integer<'a> {
    pub(crate) term: NifTerm,
    pub(crate) env: Env<'a>,
}

impl<'a> Integer<'a> {
    /// Construct an integer term from an `i64`.
    pub fn from_i64(env: Env<'a>, val: i64) -> Integer<'a> {
        env.make_int64(val)
    }

    /// Construct an integer term from a `u64`.
    pub fn from_u64(env: Env<'a>, val: u64) -> Integer<'a> {
        env.make_uint64(val)
    }
}

impl<'a> Env<'a> {
    /// Construct an integer term from an `i64` (`enif_make_int64`).
    pub fn make_int64(self, val: i64) -> Integer<'a> {
        let term = unsafe { crate::enif::make_int64(self.as_ptr(), val) };
        Integer { term, env: self }
    }

    /// Construct an integer term from a `u64` (`enif_make_uint64`).
    pub fn make_uint64(self, val: u64) -> Integer<'a> {
        let term = unsafe { crate::enif::make_uint64(self.as_ptr(), val) };
        Integer { term, env: self }
    }

    /// Extract an `i64` from an integer term (`enif_get_int64`).
    /// `None` if the term is not an integer or does not fit in `i64`.
    pub fn get_int64(self, term: impl AsNifTerm<'a>) -> Option<i64> {
        let mut val: i64 = 0;
        if unsafe { crate::enif::get_int64(self.as_ptr(), term.as_nif_term(), &mut val) != 0 } {
            Some(val)
        } else {
            None
        }
    }

    /// Extract a `u64` from an integer term (`enif_get_uint64`).
    /// `None` if the term is not an integer or does not fit in `u64`.
    pub fn get_uint64(self, term: impl AsNifTerm<'a>) -> Option<u64> {
        let mut val: u64 = 0;
        if unsafe { crate::enif::get_uint64(self.as_ptr(), term.as_nif_term(), &mut val) != 0 } {
            Some(val)
        } else {
            None
        }
    }
}

impl TryFrom<Integer<'_>> for i64 {
    type Error = CodecError;
    /// Returns `IntegerOverflow` if the value does not fit in `i64`.
    fn try_from(int: Integer<'_>) -> Result<i64, CodecError> {
        int.env.get_int64(int).ok_or(CodecError::IntegerOverflow)
    }
}

impl TryFrom<Integer<'_>> for u64 {
    type Error = CodecError;
    /// Returns `IntegerOverflow` if the value does not fit in `u64`
    /// (including negative values).
    fn try_from(int: Integer<'_>) -> Result<u64, CodecError> {
        int.env.get_uint64(int).ok_or(CodecError::IntegerOverflow)
    }
}

impl TryFrom<Integer<'_>> for i128 {
    type Error = CodecError;
    /// Covers the combined range of `i64` and `u64`. The NIF API has no
    /// 128-bit accessor, so values in `i64::MIN..=i64::MAX` use the signed
    /// path and values in `i64::MAX+1..=u64::MAX` use the unsigned path.
    /// Values outside that range return `IntegerOverflow`.
    fn try_from(int: Integer<'_>) -> Result<i128, CodecError> {
        if let Ok(val) = i64::try_from(int) {
            return Ok(val as i128);
        }
        if let Ok(val) = u64::try_from(int) {
            return Ok(val as i128);
        }
        Err(CodecError::IntegerOverflow)
    }
}

impl PartialEq for Integer<'_> {
    fn eq(&self, other: &Self) -> bool {
        unsafe { crate::enif::is_identical(self.term, other.term) != 0 }
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
        let c = unsafe { crate::enif::compare(self.term, other.term) };
        c.cmp(&0)
    }
}

impl std::fmt::Debug for Integer<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Integer")
    }
}

impl<'b> Encoder for Integer<'b> {
    fn encode<'a>(&self, env: Env<'a>) -> Term<'a> {
        if self.env.as_ptr() == env.as_ptr() {
            Term::new(env, self.term)
        } else {
            env.make_copy(*self)
        }
    }
}

impl<'a> Decoder<'a> for Integer<'a> {
    fn decode(term: Term<'a>) -> Result<Self, CodecError> {
        if term.env.term_type(term) == NifTermType::Integer {
            Ok(Integer { term: term.term, env: term.env })
        } else {
            Err(CodecError::WrongType)
        }
    }
}
