//! [`Binary`] (byte-aligned) and [`Bitstring`] (possibly sub-byte) — Erlang
//! binary data, viewed zero-copy into the BEAM heap. Also the binary side of
//! external-term-format (de)serialization.

use core::marker::PhantomData;
use std::str::Utf8Error;

use crate::types::sealed::Sealed;
use crate::types::{AnyTerm, Env, Invariant, RawTerm, Term};

/// A byte-aligned binary (`enif_is_binary` returned true).
///
/// Data is on the BEAM heap. Nothing is copied until `as_bytes`/`from_bytes` is
/// called. The byte accessors take an `impl Env<'id>` (this brand) because the
/// term carries only its brand, not its env.
#[derive(Clone, Copy)]
pub struct Binary<'id> {
    raw_term: RawTerm,
    _id: Invariant<'id>,
}

/// An Erlang bitstring (`enif_term_type` returned `Bitstring`).
///
/// In BEAM every binary is a bitstring; a `Bitstring` may be byte-aligned or
/// sub-byte. Call [`is_binary`](Self::is_binary) / [`to_binary`](Self::to_binary)
/// to refine. The NIF API offers no inspection of the sub-byte case.
#[derive(Clone, Copy)]
pub struct Bitstring<'id> {
    raw_term: RawTerm,
    _id: Invariant<'id>,
}

impl<'id> Bitstring<'id> {
    #[crate::raw]
    pub(crate) fn from_raw(raw_term: RawTerm) -> Self {
        Self { raw_term, _id: PhantomData }
    }

    /// Returns `true` if this bitstring is byte-aligned (`enif_is_binary`).
    pub fn is_binary(self, env: impl Env<'id>) -> bool {
        unsafe { enif_ffi::is_binary(env.raw_env(), self.raw_term) != 0 }
    }

    /// View as a [`Binary`] if byte-aligned, else `None`.
    pub fn to_binary(self, env: impl Env<'id>) -> Option<Binary<'id>> {
        self.is_binary(env)
            .then_some(Binary { raw_term: self.raw_term, _id: PhantomData })
    }
}

impl<'id> Binary<'id> {
    #[crate::raw]
    pub(crate) fn from_raw(raw_term: RawTerm) -> Self {
        Self { raw_term, _id: PhantomData }
    }

    /// View the binary data as a byte slice (`enif_inspect_binary`).
    ///
    /// Zero-copy — the slice points into the BEAM heap and rides this binary's
    /// brand `'id`, which cannot escape its env's scope.
    pub fn as_bytes(self, env: impl Env<'id>) -> &'id [u8] {
        let mut bin: enif_ffi::Binary = unsafe { std::mem::zeroed() };
        let ok = unsafe { enif_ffi::inspect_binary(env.raw_env(), self.raw_term, &mut bin) };
        assert!(ok != 0, "inspect_binary failed on a validated Binary");
        unsafe { std::slice::from_raw_parts(bin.data, bin.size) }
    }

    /// Number of bytes in the binary.
    pub fn len(self, env: impl Env<'id>) -> usize {
        self.as_bytes(env).len()
    }

    /// Returns `true` if the binary contains no bytes.
    pub fn is_empty(self, env: impl Env<'id>) -> bool {
        self.len(env) == 0
    }

    /// Interpret the binary as a UTF-8 string (zero-copy). `Err` if the bytes
    /// are not valid UTF-8.
    pub fn try_str(self, env: impl Env<'id>) -> Result<&'id str, Utf8Error> {
        std::str::from_utf8(self.as_bytes(env))
    }

    /// Create a zero-copy sub-binary term spanning `pos..pos+len`
    /// (`enif_make_sub_binary`). Panics if out of bounds.
    pub fn sub(self, env: impl Env<'id>, pos: usize, len: usize) -> Binary<'id> {
        let total = self.len(env);
        assert!(
            pos.checked_add(len).is_some_and(|end| end <= total),
            "sub-binary out of bounds: pos({pos}) + len({len}) > {total}"
        );
        let raw_term = unsafe { enif_ffi::make_sub_binary(env.raw_env(), self.raw_term, pos, len) };
        Binary { raw_term, _id: PhantomData }
    }

    /// Allocate a new binary on the BEAM heap and copy `data` into it
    /// (`enif_make_new_binary`).
    pub fn from_bytes(env: impl Env<'id>, data: &[u8]) -> Binary<'id> {
        let mut raw_term: RawTerm = 0;
        unsafe {
            let ptr = enif_ffi::make_new_binary(env.raw_env(), data.len(), &mut raw_term);
            // enif_make_new_binary cannot return null: tracing the ERTS source
            // (verified across OTP 26 and 27) it allocates via HAlloc or
            // erts_bin_nrml_alloc, both of which abort the VM on OOM rather than
            // returning null. The assert pins that invariant so the copy below
            // never runs against a null destination.
            assert!(!ptr.is_null(), "enif_make_new_binary returned null");
            std::ptr::copy_nonoverlapping(data.as_ptr(), ptr, data.len());
        }
        Binary { raw_term, _id: PhantomData }
    }

    /// Returns `true` if `term` is a byte-aligned binary (`enif_is_binary`).
    /// Sub-byte bitstrings return `false`.
    pub fn is_binary(env: impl Env<'id>, term: impl Term<'id>) -> bool {
        unsafe { enif_ffi::is_binary(env.raw_env(), term.raw_term()) != 0 }
    }

    /// Deserialize a term from this binary's external-term-format contents
    /// (`enif_binary_to_term`). If `safe`, encoded atoms not already in the atom
    /// table are rejected. `None` on decode failure.
    pub fn deserialize(self, env: impl Env<'id>, safe: bool) -> Option<AnyTerm<'id>> {
        let bytes = self.as_bytes(env);
        let opts = if safe { enif_ffi::BIN2TERM_SAFE } else { 0 };
        let mut term: RawTerm = 0;
        let consumed = unsafe {
            enif_ffi::binary_to_term(env.raw_env(), bytes.as_ptr(), bytes.len(), &mut term, opts)
        };
        (consumed != 0).then(|| AnyTerm::wrap(term, env))
    }
}

impl std::fmt::Debug for Binary<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Binary")
    }
}

impl PartialEq for Binary<'_> {
    fn eq(&self, other: &Self) -> bool {
        unsafe { enif_ffi::is_identical(self.raw_term, other.raw_term) != 0 }
    }
}
impl Eq for Binary<'_> {}
impl PartialOrd for Binary<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Binary<'_> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        unsafe { enif_ffi::compare(self.raw_term, other.raw_term) }.cmp(&0)
    }
}

impl std::fmt::Debug for Bitstring<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Bitstring")
    }
}
impl PartialEq for Bitstring<'_> {
    fn eq(&self, other: &Self) -> bool {
        unsafe { enif_ffi::is_identical(self.raw_term, other.raw_term) != 0 }
    }
}
impl Eq for Bitstring<'_> {}
impl PartialOrd for Bitstring<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Bitstring<'_> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        unsafe { enif_ffi::compare(self.raw_term, other.raw_term) }.cmp(&0)
    }
}

impl<'id> Sealed for Binary<'id> {}
impl<'id> Term<'id> for Binary<'id> {
    fn raw_term(self) -> RawTerm {
        self.raw_term
    }
}

impl<'id> Sealed for Bitstring<'id> {}
impl<'id> Term<'id> for Bitstring<'id> {
    fn raw_term(self) -> RawTerm {
        self.raw_term
    }
}
