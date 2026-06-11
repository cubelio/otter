use crate::codec::{CodecError, Decoder, Encoder};
use crate::env::Env;
use crate::sys::NifTerm;
use crate::term::{RawTerm, TypedTerm};

/// An Erlang fun (closure or function reference).
///
/// The NIF API provides no inspection of fun contents. A `Fun` can only be
/// held and passed back to Erlang, or used as an argument to `apply`.
#[derive(Clone, Copy)]
pub struct Fun<'a> {
    pub(crate) term: NifTerm,
    // Env is stored for lifetime tracking only — the NIF API provides no
    // inspection functions for funs, so `env` is never read directly.
    #[allow(dead_code)]
    pub(crate) env: Env<'a>,
}

impl PartialEq for Fun<'_> {
    fn eq(&self, other: &Self) -> bool {
        unsafe { crate::wrapper::term::is_identical(self.term, other.term) }
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
        let c = unsafe { crate::wrapper::term::compare(self.term, other.term) };
        c.cmp(&0)
    }
}

impl std::fmt::Debug for Fun<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Fun")
    }
}

impl<'b> Encoder for Fun<'b> {
    fn encode<'a>(&self, env: Env<'a>) -> RawTerm<'a> {
        let term = unsafe { crate::wrapper::term::make_copy(env.as_ptr(), self.term) };
        RawTerm::new(env, term)
    }
}

impl<'a> Decoder<'a> for Fun<'a> {
    fn decode(term: TypedTerm<'a>) -> Result<Self, CodecError> {
        match term {
            TypedTerm::Fun(f) => Ok(f),
            _ => Err(CodecError::WrongType),
        }
    }
}
