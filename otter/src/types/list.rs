use crate::codec::{CodecError, Decoder, Encoder};
use crate::env::Env;
use crate::sys::NifTerm;
use crate::term::{Term, TypedTerm, AsNifTerm};

/// An Erlang list term.
///
/// At this level, we only know `enif_term_type` returned `List`. Call
/// [`node`](List::node) to decompose into [`Node::Nil`] or
/// [`Node::Cell`] with one `enif_get_list_cell` call.
#[derive(Clone, Copy)]
pub struct List<'a> {
    pub(crate) term: NifTerm,
    pub(crate) env: Env<'a>,
}

/// Result of decomposing a [`List`] via [`List::node`].
#[derive(Clone, Copy)]
pub enum Node<'a> {
    /// The empty list `[]`.
    Nil,
    /// A cons cell `[Head | Tail]`. Both are unresolved [`Term`]s.
    Cell(Term<'a>, Term<'a>),
}

impl<'a> List<'a> {
    /// Decompose this list into nil or a cons cell.
    ///
    /// One `enif_get_list_cell` call. Returns [`Node::Nil`] for `[]`,
    /// or [`Node::Cell`] with head and tail as [`Term`]s.
    pub fn node(self) -> Node<'a> {
        let mut head: NifTerm = 0;
        let mut tail: NifTerm = 0;
        if unsafe {
            crate::wrapper::list::get_list_cell(self.env.as_ptr(), self.term, &mut head, &mut tail)
        } {
            Node::Cell(Term::new(self.env, head), Term::new(self.env, tail))
        } else {
            Node::Nil
        }
    }

    /// Attempt to collect a list of integer codepoints into a `String`.
    ///
    /// Uses `enif_get_string_length` and `enif_get_string` with UTF-8
    /// encoding. Returns `WrongType` if the list is not a valid string.
    pub fn try_string(self) -> Result<String, CodecError> {
        let len = unsafe {
            crate::wrapper::list::get_string_length(
                self.env.as_ptr(),
                self.term,
                crate::sys::NifCharEncoding::Utf8,
            )
        }.ok_or(CodecError::WrongType)?;

        if len == 0 {
            return Ok(String::new());
        }

        let mut buf = vec![0u8; len + 1]; // +1 for null terminator
        let ret = unsafe {
            crate::wrapper::list::get_string(
                self.env.as_ptr(),
                self.term,
                &mut buf,
                crate::sys::NifCharEncoding::Utf8,
            )
        };
        if ret <= 0 {
            return Err(CodecError::WrongType);
        }

        buf.truncate(len); // remove null terminator
        // SAFETY: BEAM guarantees valid UTF-8 when encoding is ERL_NIF_UTF8
        Ok(unsafe { String::from_utf8_unchecked(buf) })
    }

    /// Return the number of elements in a proper list.
    ///
    /// Returns `None` for an improper list (one whose final tail is not `[]`).
    /// Traverses the entire list — O(n).
    pub fn len(self) -> Option<usize> {
        unsafe { crate::wrapper::list::get_list_length(self.env.as_ptr(), self.term) }
    }

    /// Reverse a proper list.
    ///
    /// Returns `None` for improper lists (those whose final tail is not `[]`).
    /// Wraps `enif_make_reverse_list`.
    pub fn reverse(self) -> Option<List<'a>> {
        let term = unsafe {
            crate::wrapper::list::make_reverse_list(self.env.as_ptr(), self.term)
        }?;
        Some(List { term, env: self.env })
    }

    /// Construct an Erlang string (list of codepoints) from a UTF-8 `&str`.
    ///
    /// Wraps `enif_make_string_len` with `ERL_NIF_UTF8`.
    pub fn from_str(env: Env<'a>, s: &str) -> List<'a> {
        let term = unsafe {
            crate::wrapper::list::make_string_len(
                env.as_ptr(),
                s.as_bytes(),
                crate::sys::NifCharEncoding::Utf8,
            )
        };
        List { term, env }
    }

    /// Construct a list from any iterable of term-like values.
    ///
    /// Equivalent to `enif_make_list_from_array`. An empty iterator produces the
    /// empty list `[]`.
    pub fn from_terms<I, T>(env: Env<'a>, terms: I) -> List<'a>
    where
        I: IntoIterator<Item = T>,
        T: AsNifTerm<'a>,
    {
        let raw: Vec<NifTerm> = terms.into_iter().map(|t| t.as_nif_term()).collect();
        let term = unsafe { crate::wrapper::list::make_list(env.as_ptr(), &raw) };
        List { term, env }
    }

    /// Construct a cons cell `[head | tail]`.
    ///
    /// `tail` may be a `List`, the nil atom `[]`, or any other term.
    pub fn cons(env: Env<'a>, head: impl AsNifTerm<'a>, tail: impl AsNifTerm<'a>) -> List<'a> {
        let term = unsafe {
            crate::wrapper::list::make_list_cell(env.as_ptr(), head.as_nif_term(), tail.as_nif_term())
        };
        List { term, env }
    }

    /// Return an iterator over the heads of this list.
    ///
    /// Each call to `next()` is one `enif_get_list_cell` call, yielding the
    /// head as an unresolved [`Term`]. Iteration stops when the tail is
    /// not a cons cell.
    ///
    /// After iteration, call [`ListIterator::tail`] to get the terminal
    /// value — `[]` for proper lists, or the improper tail term.
    pub fn iter(self) -> ListIterator<'a> {
        ListIterator {
            current: self.term,
            env: self.env,
            tail: None,
        }
    }
}

// ---------------------------------------------------------------------------
// ListIterator
// ---------------------------------------------------------------------------

/// Iterator over the head elements of a [`List`].
///
/// Yields [`Term`] heads. After `next()` returns `None`, call [`tail`]
/// to inspect the terminal value: `[]` (nil) for proper lists, or the
/// improper tail term.
///
/// [`tail`]: ListIterator::tail
pub struct ListIterator<'a> {
    current: NifTerm,
    env: Env<'a>,
    tail: Option<TypedTerm<'a>>,
}

impl<'a> Iterator for ListIterator<'a> {
    type Item = Term<'a>;

    fn next(&mut self) -> Option<Term<'a>> {
        if self.tail.is_some() {
            return None;
        }
        let mut head: NifTerm = 0;
        let mut tail: NifTerm = 0;
        if unsafe {
            crate::wrapper::list::get_list_cell(self.env.as_ptr(), self.current, &mut head, &mut tail)
        } {
            self.current = tail;
            Some(Term::new(self.env, head))
        } else {
            // Not a cons cell — this is the terminal.
            self.tail = Some(Term::new(self.env, self.current).resolve());
            None
        }
    }
}

impl std::iter::FusedIterator for ListIterator<'_> {}

impl<'a> IntoIterator for List<'a> {
    type Item = Term<'a>;
    type IntoIter = ListIterator<'a>;

    fn into_iter(self) -> ListIterator<'a> {
        self.iter()
    }
}

impl<'a> ListIterator<'a> {
    /// The terminal value of the list walk.
    ///
    /// For proper lists this is `TypedTerm::List` (nil / `[]`).
    /// For improper lists this is whatever term was in the final tail
    /// position (e.g. `TypedTerm::Integer`, `TypedTerm::Atom`, etc.).
    ///
    /// Returns `None` if the iterator has not yet been exhausted.
    pub fn tail(&self) -> Option<TypedTerm<'a>> {
        self.tail
    }
}

impl PartialEq for List<'_> {
    fn eq(&self, other: &Self) -> bool {
        unsafe { crate::wrapper::term::is_identical(self.term, other.term) }
    }
}

impl Eq for List<'_> {}

impl PartialOrd for List<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for List<'_> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let c = unsafe { crate::wrapper::term::compare(self.term, other.term) };
        c.cmp(&0)
    }
}

impl std::fmt::Debug for List<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "List")
    }
}

impl<'b> Encoder for List<'b> {
    fn encode<'a>(&self, env: Env<'a>) -> Term<'a> {
        let raw = if self.env.as_ptr() == env.as_ptr() {
            self.term
        } else {
            unsafe { crate::wrapper::term::make_copy(env.as_ptr(), self.term) }
        };
        Term::new(env, raw)
    }
}

impl<'a> Env<'a> {
    /// Returns `true` if `term` is a list, including improper and empty lists
    /// (`enif_is_list`).
    pub fn is_list(self, term: impl AsNifTerm<'a>) -> bool {
        unsafe { crate::enif::is_list(self.as_ptr(), term.as_nif_term()) != 0 }
    }
}

impl<'a> Decoder<'a> for List<'a> {
    fn decode(term: Term<'a>) -> Result<Self, CodecError> {
        if term.env.is_list(term) {
            Ok(List { term: term.term, env: term.env })
        } else {
            Err(CodecError::WrongType)
        }
    }
}
