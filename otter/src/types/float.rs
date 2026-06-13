use crate::codec::{CodecError, Decoder, Encoder};
use crate::env::Env;
use crate::sys::{NifTerm, NifTermType};
use crate::term::{Term, AsNifTerm, Raised};

/// An Erlang float. Always IEEE 754 double precision.
///
/// Floats are heap-allocated in the BEAM even though the value is always `f64`.
#[derive(Clone, Copy)]
pub struct Float<'a> {
    pub(crate) term: NifTerm,
    pub(crate) env: Env<'a>,
}

impl<'a> Float<'a> {
    /// Construct a float term from an `f64`.
    ///
    /// Returns `Err(Raised)` if `val` is not finite: `enif_make_double` raises
    /// `badarg` for NaN and infinities.
    pub fn from_f64(env: Env<'a>, val: f64) -> Result<Float<'a>, Raised<'a>> {
        env.make_double(val)
    }
}

impl<'a> Env<'a> {
    /// Construct a float term from an `f64` (`enif_make_double`).
    ///
    /// Returns `Err(Raised)` if `val` is not finite (NaN or infinity), which
    /// the BEAM rejects with `badarg`.
    pub fn make_double(self, val: f64) -> Result<Float<'a>, Raised<'a>> {
        let term = unsafe { crate::enif::make_double(self.as_ptr(), val) };
        Ok(Float { term: self.check_raised(term)?.as_raw(), env: self })
    }

    /// Extract an `f64` from a float term (`enif_get_double`).
    /// `None` if the term is not a float.
    pub fn get_double(self, term: impl AsNifTerm<'a>) -> Option<f64> {
        let mut val: f64 = 0.0;
        if unsafe { crate::enif::get_double(self.as_ptr(), term.as_nif_term(), &mut val) != 0 } {
            Some(val)
        } else {
            None
        }
    }
}

impl From<Float<'_>> for f64 {
    /// Extract the `f64` value. Infallible — the BEAM only stores `f64`.
    fn from(float: Float<'_>) -> f64 {
        float.env.get_double(float).unwrap_or(0.0)
    }
}

impl PartialEq for Float<'_> {
    fn eq(&self, other: &Self) -> bool {
        unsafe { crate::enif::is_identical(self.term, other.term) != 0 }
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
        let c = unsafe { crate::enif::compare(self.term, other.term) };
        c.cmp(&0)
    }
}

impl std::fmt::Debug for Float<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Float")
    }
}

impl<'b> Encoder for Float<'b> {
    fn encode<'a>(&self, env: Env<'a>) -> Term<'a> {
        if self.env.as_ptr() == env.as_ptr() {
            Term::new(env, self.term)
        } else {
            env.make_copy(*self)
        }
    }
}

impl<'a> Decoder<'a> for Float<'a> {
    fn decode(term: Term<'a>) -> Result<Self, CodecError> {
        if term.env.term_type(term) == Some(NifTermType::Float) {
            Ok(Float { term: term.term, env: term.env })
        } else {
            Err(CodecError::WrongType)
        }
    }
}
