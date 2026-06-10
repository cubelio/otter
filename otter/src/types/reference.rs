use crate::codec::{CodecError, Decoder, Encoder};
use crate::env::Env;
use crate::sys::NifTerm;
use crate::term::{RawTerm, Term};

/// An Erlang reference.
#[derive(Clone, Copy)]
pub struct Reference<'a> {
    pub(crate) term: NifTerm,
    pub(crate) env: Env<'a>,
}

impl<'a> Reference<'a> {
    /// Create a new unique reference.
    ///
    /// Wraps `enif_make_ref`.
    pub fn new(env: Env<'a>) -> Reference<'a> {
        let term = unsafe { crate::wrapper::term::make_ref(env.as_ptr()) };
        Reference { term, env }
    }
}

impl PartialEq for Reference<'_> {
    fn eq(&self, other: &Self) -> bool {
        unsafe { crate::wrapper::term::is_identical(self.term, other.term) }
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
        let c = unsafe { crate::wrapper::term::compare(self.term, other.term) };
        c.cmp(&0)
    }
}

impl std::fmt::Debug for Reference<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Reference")
    }
}

impl<'b> Encoder for Reference<'b> {
    fn encode<'a>(&self, env: Env<'a>) -> RawTerm<'a> {
        let term = unsafe { crate::wrapper::term::make_copy(env.as_ptr(), self.term) };
        RawTerm::new(env, term)
    }
}

impl<'a> Decoder<'a> for Reference<'a> {
    fn decode(term: Term<'a>) -> Result<Self, CodecError> {
        match term {
            Term::Reference(r) => Ok(r),
            _ => Err(CodecError::WrongType),
        }
    }
}
