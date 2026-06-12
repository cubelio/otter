use crate::codec::{CodecError, Decoder, Encoder};
use crate::env::Env;
use crate::sys::NifTerm;
use crate::term::{Term, AsNifTerm};

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
        unsafe { crate::enif::is_identical(self.term, other.term) != 0 }
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
        let c = unsafe { crate::enif::compare(self.term, other.term) };
        c.cmp(&0)
    }
}

impl std::fmt::Debug for Fun<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Fun")
    }
}

impl<'b> Encoder for Fun<'b> {
    fn encode<'a>(&self, env: Env<'a>) -> Term<'a> {
        if self.env.as_ptr() == env.as_ptr() {
            Term::new(env, self.term)
        } else {
            env.make_copy(*self)
        }
    }
}

impl<'a> Env<'a> {
    /// Returns `true` if `term` is a fun (`enif_is_fun`).
    pub fn is_fun(self, term: impl AsNifTerm<'a>) -> bool {
        unsafe { crate::enif::is_fun(self.as_ptr(), term.as_nif_term()) != 0 }
    }
}

impl<'a> Decoder<'a> for Fun<'a> {
    fn decode(term: Term<'a>) -> Result<Self, CodecError> {
        if term.env.is_fun(term) {
            Ok(Fun { term: term.term, env: term.env })
        } else {
            Err(CodecError::WrongType)
        }
    }
}
