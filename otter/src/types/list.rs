use std::ffi::{c_char, c_uint};

use crate::codec::{CodecError, Decoder, Encoder};
use crate::env::Env;
use crate::sys::{NifCharEncoding, NifTerm};
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
        match self.env.get_list_cell(self) {
            Some((head, tail)) => Node::Cell(head, tail),
            None => Node::Nil,
        }
    }

    /// Attempt to collect a list of integer codepoints into a `String`.
    ///
    /// Uses `enif_get_string_length` and `enif_get_string` with UTF-8
    /// encoding. Returns `WrongType` if the list is not a valid string.
    pub fn try_string(self) -> Result<String, CodecError> {
        self.env.get_string(self).ok_or(CodecError::WrongType)
    }

    /// Return the number of elements in a proper list.
    ///
    /// Returns `None` for an improper list (one whose final tail is not `[]`).
    /// Traverses the entire list — O(n).
    pub fn len(self) -> Option<usize> {
        self.env.get_list_length(self)
    }

    /// Reverse a proper list.
    ///
    /// Returns `None` for improper lists (those whose final tail is not `[]`).
    /// Wraps `enif_make_reverse_list`.
    pub fn reverse(self) -> Option<List<'a>> {
        self.env.make_reverse_list(self)
    }

    /// Construct an Erlang string (list of codepoints) from a UTF-8 `&str`.
    ///
    /// Wraps `enif_make_string_len` with `ERL_NIF_UTF8`.
    pub fn from_str(env: Env<'a>, s: &str) -> List<'a> {
        env.make_string(s)
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
        env.make_list(terms)
    }

    /// Construct a cons cell `[head | tail]`.
    ///
    /// `tail` may be a `List`, the nil atom `[]`, or any other term.
    pub fn cons(env: Env<'a>, head: impl AsNifTerm<'a>, tail: impl AsNifTerm<'a>) -> List<'a> {
        env.make_list_cell(head, tail)
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
        let current = Term::new(self.env, self.current);
        if let Some((head, tail)) = self.env.get_list_cell(current) {
            self.current = tail.as_raw();
            Some(head)
        } else {
            // Not a cons cell — this is the terminal.
            self.tail = Some(current.resolve());
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
        unsafe { crate::enif::is_identical(self.term, other.term) != 0 }
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
        let c = unsafe { crate::enif::compare(self.term, other.term) };
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
        if self.env.as_ptr() == env.as_ptr() {
            Term::new(env, self.term)
        } else {
            env.make_copy(*self)
        }
    }
}

impl<'a> Env<'a> {
    /// Returns `true` if `term` is a list, including improper and empty lists
    /// (`enif_is_list`).
    pub fn is_list(self, term: impl AsNifTerm<'a>) -> bool {
        unsafe { crate::enif::is_list(self.as_ptr(), term.as_nif_term()) != 0 }
    }

    /// Decompose a list into its head and tail (`enif_get_list_cell`).
    ///
    /// Returns `None` if `term` is not a non-empty list (i.e. it is `[]` or
    /// not a list). Head and tail are unresolved [`Term`]s in this env.
    pub fn get_list_cell(self, term: impl AsNifTerm<'a>) -> Option<(Term<'a>, Term<'a>)> {
        let mut head: NifTerm = 0;
        let mut tail: NifTerm = 0;
        if unsafe {
            crate::enif::get_list_cell(self.as_ptr(), term.as_nif_term(), &mut head, &mut tail) != 0
        } {
            Some((Term::new(self, head), Term::new(self, tail)))
        } else {
            None
        }
    }

    /// The number of elements in a proper list (`enif_get_list_length`).
    /// `None` for an improper list. Traverses the whole list — O(n).
    pub fn get_list_length(self, term: impl AsNifTerm<'a>) -> Option<usize> {
        let mut len: c_uint = 0;
        if unsafe { crate::enif::get_list_length(self.as_ptr(), term.as_nif_term(), &mut len) != 0 } {
            Some(len as usize)
        } else {
            None
        }
    }

    /// Construct a list from any iterable of term-like values
    /// (`enif_make_list_from_array`). An empty iterator produces `[]`.
    pub fn make_list<I, T>(self, terms: I) -> List<'a>
    where
        I: IntoIterator<Item = T>,
        T: AsNifTerm<'a>,
    {
        let raw: Vec<NifTerm> = terms.into_iter().map(|t| t.as_nif_term()).collect();
        let term = unsafe {
            crate::enif::make_list_from_array(self.as_ptr(), raw.as_ptr(), raw.len() as c_uint)
        };
        List { term, env: self }
    }

    /// Construct a cons cell `[head | tail]` (`enif_make_list_cell`).
    /// `tail` may be a list, `[]`, or any other term (improper list).
    pub fn make_list_cell(
        self,
        head: impl AsNifTerm<'a>,
        tail: impl AsNifTerm<'a>,
    ) -> List<'a> {
        let term = unsafe {
            crate::enif::make_list_cell(self.as_ptr(), head.as_nif_term(), tail.as_nif_term())
        };
        List { term, env: self }
    }

    /// Reverse a proper list (`enif_make_reverse_list`).
    /// `None` for improper lists (final tail not `[]`).
    pub fn make_reverse_list(self, term: impl AsNifTerm<'a>) -> Option<List<'a>> {
        let mut result: NifTerm = 0;
        if unsafe { crate::enif::make_reverse_list(self.as_ptr(), term.as_nif_term(), &mut result) != 0 }
        {
            Some(List { term: result, env: self })
        } else {
            None
        }
    }

    /// Construct an Erlang string (list of codepoints) from a UTF-8 `&str`
    /// (`enif_make_string_len`, `ERL_NIF_UTF8`).
    pub fn make_string(self, s: &str) -> List<'a> {
        let term = unsafe {
            crate::enif::make_string_len(
                self.as_ptr(),
                s.as_ptr() as *const c_char,
                s.len(),
                NifCharEncoding::Utf8,
            )
        };
        List { term, env: self }
    }

    /// Collect a list of integer codepoints into a `String`
    /// (`enif_get_string_length` + `enif_get_string`, `ERL_NIF_UTF8`).
    /// `None` if the term is not a valid string.
    pub fn get_string(self, term: impl AsNifTerm<'a>) -> Option<String> {
        let raw = term.as_nif_term();
        let mut len: c_uint = 0;
        if unsafe {
            crate::enif::get_string_length(self.as_ptr(), raw, &mut len, NifCharEncoding::Utf8) == 0
        } {
            return None;
        }
        let len = len as usize;
        if len == 0 {
            return Some(String::new());
        }
        let mut buf = vec![0u8; len + 1]; // +1 for null terminator
        let ret = unsafe {
            crate::enif::get_string(
                self.as_ptr(),
                raw,
                buf.as_mut_ptr() as *mut c_char,
                buf.len() as c_uint,
                NifCharEncoding::Utf8,
            )
        };
        if ret <= 0 {
            return None;
        }
        buf.truncate(len); // strip null terminator
        // SAFETY: BEAM guarantees valid UTF-8 when encoding is Utf8.
        Some(unsafe { String::from_utf8_unchecked(buf) })
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
