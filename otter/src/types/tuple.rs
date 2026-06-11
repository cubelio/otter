use crate::codec::{CodecError, Decoder, Encoder};
use crate::env::Env;
use crate::sys::NifTerm;
use crate::term::{Term, TypedTerm, TermIn};

/// An Erlang tuple.
#[derive(Clone, Copy)]
pub struct Tuple<'a> {
    pub(crate) term: NifTerm,
    pub(crate) env: Env<'a>,
}

impl<'a> Tuple<'a> {
    /// Number of elements (arity) of the tuple.
    pub fn len(self) -> usize {
        unsafe { crate::wrapper::tuple::get_tuple(self.env.as_ptr(), self.term) }
            .map(|(_, arity)| arity)
            .unwrap_or(0)
    }

    /// Returns `true` if the tuple has zero elements.
    pub fn is_empty(self) -> bool {
        self.len() == 0
    }

    /// Return the element at zero-based index `i`.
    ///
    /// Panics if `i >= self.len()`. The pointer returned by `enif_get_tuple`
    /// points into the BEAM heap and is valid for lifetime `'a`.
    pub fn element(self, i: usize) -> TypedTerm<'a> {
        let (ptr, _arity) =
            unsafe { crate::wrapper::tuple::get_tuple(self.env.as_ptr(), self.term) }.unwrap();
        let raw = unsafe { *ptr.add(i) };
        Term::new(self.env, raw).resolve()
    }

    /// Construct a tuple from any iterable of term-like values.
    pub fn from_terms<I, T>(env: Env<'a>, terms: I) -> Tuple<'a>
    where
        I: IntoIterator<Item = T>,
        T: TermIn,
    {
        let raw: Vec<NifTerm> = terms.into_iter().map(|t| t.as_c_arg()).collect();
        let term = unsafe { crate::wrapper::tuple::make_tuple(env.as_ptr(), &raw) };
        Tuple { term, env }
    }
}

impl PartialEq for Tuple<'_> {
    fn eq(&self, other: &Self) -> bool {
        unsafe { crate::wrapper::term::is_identical(self.term, other.term) }
    }
}

impl Eq for Tuple<'_> {}

impl PartialOrd for Tuple<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Tuple<'_> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let c = unsafe { crate::wrapper::term::compare(self.term, other.term) };
        c.cmp(&0)
    }
}

impl std::fmt::Debug for Tuple<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Tuple")
    }
}

impl<'b> Encoder for Tuple<'b> {
    fn encode<'a>(&self, env: Env<'a>) -> Term<'a> {
        let term = unsafe { crate::wrapper::term::make_copy(env.as_ptr(), self.term) };
        Term::new(env, term)
    }
}

impl<'a> Decoder<'a> for Tuple<'a> {
    fn decode(term: TypedTerm<'a>) -> Result<Self, CodecError> {
        match term {
            TypedTerm::Tuple(t) => Ok(t),
            _ => Err(CodecError::WrongType),
        }
    }
}
