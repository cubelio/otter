use crate::codec::{CodecError, Decoder, Encoder};
use crate::env::Env;
use crate::sys::{NifTerm, NifTermType};
use crate::term::Term;

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
        let term = unsafe { crate::wrapper::number::make_int64(env.as_ptr(), val) };
        Integer { term, env }
    }

    /// Construct an integer term from a `u64`.
    pub fn from_u64(env: Env<'a>, val: u64) -> Integer<'a> {
        let term = unsafe { crate::wrapper::number::make_uint64(env.as_ptr(), val) };
        Integer { term, env }
    }
}

impl TryFrom<Integer<'_>> for i64 {
    type Error = CodecError;
    /// Returns `IntegerOverflow` if the value does not fit in `i64`.
    fn try_from(int: Integer<'_>) -> Result<i64, CodecError> {
        let mut val: i64 = 0;
        if unsafe { crate::wrapper::number::get_int64(int.env.as_ptr(), int.term, &mut val) } {
            Ok(val)
        } else {
            Err(CodecError::IntegerOverflow)
        }
    }
}

impl TryFrom<Integer<'_>> for u64 {
    type Error = CodecError;
    /// Returns `IntegerOverflow` if the value does not fit in `u64`
    /// (including negative values).
    fn try_from(int: Integer<'_>) -> Result<u64, CodecError> {
        let mut val: u64 = 0;
        if unsafe { crate::wrapper::number::get_uint64(int.env.as_ptr(), int.term, &mut val) } {
            Ok(val)
        } else {
            Err(CodecError::IntegerOverflow)
        }
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
        unsafe { crate::wrapper::term::is_identical(self.term, other.term) }
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
        let c = unsafe { crate::wrapper::term::compare(self.term, other.term) };
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
        let raw = if self.env.as_ptr() == env.as_ptr() {
            self.term
        } else {
            unsafe { crate::wrapper::term::make_copy(env.as_ptr(), self.term) }
        };
        Term::new(env, raw)
    }
}

impl<'a> Decoder<'a> for Integer<'a> {
    fn decode(term: Term<'a>) -> Result<Self, CodecError> {
        if unsafe { crate::wrapper::term::term_type(term.env.as_ptr(), term.term) }
            == NifTermType::Integer
        {
            Ok(Integer { term: term.term, env: term.env })
        } else {
            Err(CodecError::WrongType)
        }
    }
}
