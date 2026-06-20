use core::marker::PhantomData;
use std::ffi::{c_char, c_uint};

use crate::types::sealed::Sealed;
use crate::types::{AnyEnv, AnyTerm, Env, Invariant, RawTerm, Term};

/// An Erlang list term.
///
/// At this level we only know `enif_term_type` returned `List`. Call
/// [`node`](List::node) to decompose into [`Node::Nil`] or [`Node::Cell`] with
/// one `enif_get_list_cell` call.
#[derive(Clone, Copy)]
pub struct List<'id> {
    raw_term: RawTerm,
    _id: Invariant<'id>,
}

/// Result of decomposing a [`List`] via [`List::node`].
#[derive(Clone, Copy)]
pub enum Node<'id> {
    /// The empty list `[]`.
    Nil,
    /// A cons cell `[Head | Tail]`. Both are unresolved [`AnyTerm`]s.
    Cell(AnyTerm<'id>, AnyTerm<'id>),
}

/// One `enif_get_list_cell`, returning the raw head/tail words. `None` for `[]`
/// or a non-list.
fn list_cell<'id>(env: impl Env<'id>, term: RawTerm) -> Option<(RawTerm, RawTerm)> {
    let mut head: RawTerm = 0;
    let mut tail: RawTerm = 0;
    (unsafe { enif_ffi::get_list_cell(env.raw_env(), term, &mut head, &mut tail) } != 0)
        .then_some((head, tail))
}

impl<'id> List<'id> {
    #[crate::raw]
    pub(crate) fn from_raw(raw_term: RawTerm) -> Self {
        Self { raw_term, _id: PhantomData }
    }

    /// Decompose this list into nil or a cons cell (one `enif_get_list_cell`).
    pub fn node(self, env: impl Env<'id>) -> Node<'id> {
        match list_cell(env, self.raw_term) {
            Some((head, tail)) => Node::Cell(AnyTerm::wrap(head, env), AnyTerm::wrap(tail, env)),
            None => Node::Nil,
        }
    }

    /// Attempt to collect a list of integer codepoints into a `String`
    /// (`enif_get_string_length` + `enif_get_string`, UTF-8). `None` if the list
    /// is not a valid string.
    pub fn try_string(self, env: impl Env<'id>) -> Option<String> {
        let mut len: c_uint = 0;
        if unsafe {
            enif_ffi::get_string_length(env.raw_env(), self.raw_term, &mut len, enif_ffi::CharEncoding::Utf8)
        } == 0
        {
            return None;
        }
        let len = len as usize;
        if len == 0 {
            return Some(String::new());
        }
        let mut buf = vec![0u8; len + 1]; // +1 for null terminator
        let ret = unsafe {
            enif_ffi::get_string(
                env.raw_env(),
                self.raw_term,
                buf.as_mut_ptr() as *mut c_char,
                buf.len() as c_uint,
                enif_ffi::CharEncoding::Utf8,
            )
        };
        if ret <= 0 {
            return None;
        }
        buf.truncate(len); // strip null terminator
        // SAFETY: BEAM guarantees valid UTF-8 when encoding is Utf8.
        Some(unsafe { String::from_utf8_unchecked(buf) })
    }

    /// The number of elements in a proper list (`enif_get_list_length`).
    /// `None` for an improper list. Traverses the whole list — O(n).
    pub fn len(self, env: impl Env<'id>) -> Option<usize> {
        let mut len: c_uint = 0;
        (unsafe { enif_ffi::get_list_length(env.raw_env(), self.raw_term, &mut len) } != 0)
            .then_some(len as usize)
    }

    /// Returns `true` if this is the empty list `[]` (one `enif_get_list_cell`).
    pub fn is_empty(self, env: impl Env<'id>) -> bool {
        matches!(self.node(env), Node::Nil)
    }

    /// Reverse a proper list (`enif_make_reverse_list`). `None` for improper
    /// lists (final tail not `[]`).
    pub fn reverse(self, env: impl Env<'id>) -> Option<List<'id>> {
        let mut result: RawTerm = 0;
        (unsafe { enif_ffi::make_reverse_list(env.raw_env(), self.raw_term, &mut result) } != 0)
            .then_some(List { raw_term: result, _id: PhantomData })
    }

    /// Construct an Erlang string (list of codepoints) from a UTF-8 `&str`
    /// (`enif_make_string_len`, UTF-8).
    pub fn from_str(env: impl Env<'id>, s: &str) -> List<'id> {
        let raw_term = unsafe {
            enif_ffi::make_string_len(
                env.raw_env(),
                s.as_ptr() as *const c_char,
                s.len(),
                enif_ffi::CharEncoding::Utf8,
            )
        };
        List { raw_term, _id: PhantomData }
    }

    /// Construct a list from any iterable of terms of this brand
    /// (`enif_make_list_from_array`). An empty iterator produces `[]`.
    pub fn from_terms<I, T>(env: impl Env<'id>, terms: I) -> List<'id>
    where
        I: IntoIterator<Item = T>,
        T: Term<'id>,
    {
        let raw: Vec<RawTerm> = terms.into_iter().map(|t| t.raw_term()).collect();
        let raw_term = unsafe {
            enif_ffi::make_list_from_array(env.raw_env(), raw.as_ptr(), raw.len() as c_uint)
        };
        List { raw_term, _id: PhantomData }
    }

    /// Construct a cons cell `[head | tail]` (`enif_make_list_cell`).
    /// `tail` may be a list, `[]`, or any other term (improper list).
    pub fn cons(env: impl Env<'id>, head: impl Term<'id>, tail: impl Term<'id>) -> List<'id> {
        let raw_term =
            unsafe { enif_ffi::make_list_cell(env.raw_env(), head.raw_term(), tail.raw_term()) };
        List { raw_term, _id: PhantomData }
    }

    /// Returns `true` if `term` is a list, including improper and empty lists
    /// (`enif_is_list`).
    pub fn is_list(env: impl Env<'id>, term: impl Term<'id>) -> bool {
        unsafe { enif_ffi::is_list(env.raw_env(), term.raw_term()) != 0 }
    }

    /// Iterate the heads of this list, each yielded as an unresolved
    /// [`AnyTerm`]. Iteration stops when the tail is not a cons cell; call
    /// [`ListIterator::tail`] afterward for the terminal value.
    pub fn iter(self, env: impl Env<'id>) -> ListIterator<'id> {
        ListIterator { current: self.raw_term, env: env.as_any_env(), tail: None }
    }
}

/// Iterator over the head elements of a [`List`]. Yields [`AnyTerm`] heads.
pub struct ListIterator<'id> {
    current: RawTerm,
    env: AnyEnv<'id>,
    tail: Option<AnyTerm<'id>>,
}

impl<'id> Iterator for ListIterator<'id> {
    type Item = AnyTerm<'id>;

    fn next(&mut self) -> Option<AnyTerm<'id>> {
        if self.tail.is_some() {
            return None;
        }
        match list_cell(self.env, self.current) {
            Some((head, tail)) => {
                self.current = tail;
                Some(AnyTerm::wrap(head, self.env))
            }
            None => {
                // Not a cons cell — this is the terminal value.
                self.tail = Some(AnyTerm::wrap(self.current, self.env));
                None
            }
        }
    }
}

impl std::iter::FusedIterator for ListIterator<'_> {}

impl<'id> ListIterator<'id> {
    /// The terminal value of the walk: `[]` for proper lists, or the improper
    /// tail. `None` until the iterator is exhausted.
    pub fn tail(&self) -> Option<AnyTerm<'id>> {
        self.tail
    }
}

impl PartialEq for List<'_> {
    fn eq(&self, other: &Self) -> bool {
        unsafe { enif_ffi::is_identical(self.raw_term, other.raw_term) != 0 }
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
        let c = unsafe { enif_ffi::compare(self.raw_term, other.raw_term) };
        c.cmp(&0)
    }
}

impl std::fmt::Debug for List<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "List")
    }
}

impl<'id> Sealed for List<'id> {}

impl<'id> Term<'id> for List<'id> {
    fn raw_term(self) -> RawTerm {
        self.raw_term
    }
}
