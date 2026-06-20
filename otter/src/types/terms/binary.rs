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

// ---------------------------------------------------------------------------
// BinaryBuf — owned, growable binary buffer (Vec<u8> model)
// ---------------------------------------------------------------------------

/// An owned, mutable binary buffer backed by `enif_alloc_binary`.
///
/// The RAII owner of a mutable `ErlNifBinary`: it owns its allocation directly
/// (no env, no brand), so it is a plain owned value. `Drop` releases it
/// (`enif_release_binary`). Mirrors `Vec<u8>` — tracks `len` and `capacity`
/// separately, grows via `enif_realloc_binary` — and is what term serialization
/// returns.
///
/// Read the bytes with [`as_bytes`](Self::as_bytes) (zero-copy) or `Deref`;
/// consume it with [`into_binary`](Self::into_binary) to hand the allocation to
/// the BEAM as a `Binary` term. Implements [`std::io::Write`].
pub struct BinaryBuf {
    bin: enif_ffi::Binary,
    len: usize,
    released: bool,
}

impl BinaryBuf {
    /// Create an empty buffer with no allocation.
    pub fn new() -> BinaryBuf {
        BinaryBuf::with_capacity(0)
    }

    /// Create a buffer with preallocated capacity. Panics if allocation fails.
    pub fn with_capacity(capacity: usize) -> BinaryBuf {
        let mut bin: enif_ffi::Binary = unsafe { std::mem::zeroed() };
        let ok = unsafe { enif_ffi::alloc_binary(capacity, &mut bin) != 0 };
        assert!(ok, "enif_alloc_binary failed");
        BinaryBuf { bin, len: 0, released: false }
    }

    /// Take ownership of an already-filled `enif_ffi::Binary` (e.g. from
    /// `enif_term_to_binary`), where the whole allocation is live data.
    pub(crate) fn from_filled(bin: enif_ffi::Binary) -> BinaryBuf {
        BinaryBuf { len: bin.size, bin, released: false }
    }

    /// Number of bytes written.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Returns `true` if no bytes have been written.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Allocated capacity in bytes.
    pub fn capacity(&self) -> usize {
        self.bin.size
    }

    /// View the written bytes as an immutable slice.
    pub fn as_bytes(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.bin.data, self.len) }
    }

    /// View the written bytes as a mutable slice.
    pub fn as_bytes_mut(&mut self) -> &mut [u8] {
        unsafe { std::slice::from_raw_parts_mut(self.bin.data, self.len) }
    }

    /// Resize to `new_len`, filling new bytes with `value` or truncating.
    pub fn resize(&mut self, new_len: usize, value: u8) {
        if new_len > self.len {
            self.reserve(new_len - self.len);
            unsafe {
                std::ptr::write_bytes(self.bin.data.add(self.len), value, new_len - self.len);
            }
        }
        self.len = new_len;
    }

    /// Append a single byte.
    pub fn push(&mut self, byte: u8) {
        self.reserve(1);
        unsafe { *self.bin.data.add(self.len) = byte };
        self.len += 1;
    }

    /// Append a byte slice.
    pub fn extend_from_slice(&mut self, bytes: &[u8]) {
        self.reserve(bytes.len());
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), self.bin.data.add(self.len), bytes.len());
        }
        self.len += bytes.len();
    }

    /// Ensure room for at least `additional` more bytes. Panics if realloc fails.
    pub fn reserve(&mut self, additional: usize) {
        let required = self.len + additional;
        if required <= self.bin.size {
            return;
        }
        let new_cap = required.max(self.bin.size.checked_mul(2).unwrap_or(required));
        let ok = unsafe { enif_ffi::realloc_binary(&mut self.bin, new_cap) != 0 };
        assert!(ok, "enif_realloc_binary failed");
    }

    /// Consume the buffer, handing its allocation to the BEAM as a `Binary`
    /// term (`enif_make_binary`). Shrinks to the exact written length first.
    pub fn into_binary<'id>(mut self, env: impl Env<'id>) -> Binary<'id> {
        if self.len < self.bin.size {
            let ok = unsafe { enif_ffi::realloc_binary(&mut self.bin, self.len) != 0 };
            assert!(ok, "enif_realloc_binary failed on shrink");
        }
        self.released = true;
        let raw_term = unsafe { enif_ffi::make_binary(env.raw_env(), &mut self.bin) };
        Binary { raw_term, _id: PhantomData }
    }
}

impl Default for BinaryBuf {
    fn default() -> Self {
        BinaryBuf::new()
    }
}

impl std::io::Write for BinaryBuf {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Drop for BinaryBuf {
    fn drop(&mut self) {
        if !self.released {
            unsafe { enif_ffi::release_binary(&mut self.bin) };
        }
    }
}

impl std::ops::Deref for BinaryBuf {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        self.as_bytes()
    }
}

impl std::ops::DerefMut for BinaryBuf {
    fn deref_mut(&mut self) -> &mut [u8] {
        self.as_bytes_mut()
    }
}

impl AsRef<[u8]> for BinaryBuf {
    fn as_ref(&self) -> &[u8] {
        self.as_bytes()
    }
}

impl AsMut<[u8]> for BinaryBuf {
    fn as_mut(&mut self) -> &mut [u8] {
        self.as_bytes_mut()
    }
}

impl Extend<u8> for BinaryBuf {
    fn extend<I: IntoIterator<Item = u8>>(&mut self, iter: I) {
        let iter = iter.into_iter();
        let (lower, _) = iter.size_hint();
        self.reserve(lower);
        for byte in iter {
            self.push(byte);
        }
    }
}

impl<'a> Extend<&'a u8> for BinaryBuf {
    fn extend<I: IntoIterator<Item = &'a u8>>(&mut self, iter: I) {
        self.extend(iter.into_iter().copied());
    }
}

impl std::fmt::Debug for BinaryBuf {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BinaryBuf")
            .field("len", &self.len)
            .field("capacity", &self.bin.size)
            .finish()
    }
}
