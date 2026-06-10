use crate::codec::{CodecError, Decoder, Encoder};
use crate::env::Env;
use crate::sys::NifTerm;
use crate::term::{RawTerm, Term};

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
    pub fn from_f64(env: Env<'a>, val: f64) -> Float<'a> {
        let term = unsafe { crate::wrapper::number::make_double(env.as_ptr(), val) };
        Float { term, env }
    }
}

impl From<Float<'_>> for f64 {
    /// Extract the `f64` value. Infallible — the BEAM only stores `f64`.
    fn from(float: Float<'_>) -> f64 {
        let mut val: f64 = 0.0;
        unsafe { crate::wrapper::number::get_double(float.env.as_ptr(), float.term, &mut val) };
        val
    }
}

impl PartialEq for Float<'_> {
    fn eq(&self, other: &Self) -> bool {
        unsafe { crate::wrapper::term::is_identical(self.term, other.term) }
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
        let c = unsafe { crate::wrapper::term::compare(self.term, other.term) };
        c.cmp(&0)
    }
}

impl std::fmt::Debug for Float<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Float")
    }
}

impl<'b> Encoder for Float<'b> {
    fn encode<'a>(&self, env: Env<'a>) -> RawTerm<'a> {
        let term = unsafe { crate::wrapper::term::make_copy(env.as_ptr(), self.term) };
        RawTerm::new(env, term)
    }
}

impl<'a> Decoder<'a> for Float<'a> {
    fn decode(term: Term<'a>) -> Result<Self, CodecError> {
        match term {
            Term::Float(f) => Ok(f),
            _ => Err(CodecError::WrongType),
        }
    }
}
