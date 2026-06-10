use std::str::Utf8Error;
use crate::codec::{CodecError, Decoder, Encoder};
use crate::env::Env;
use crate::sys::NifTerm;
use crate::term::{RawTerm, Term};

/// A byte-aligned binary (`enif_is_binary` returned true).
///
/// Data is on the BEAM heap. Nothing is copied until `as_bytes` or
/// `from_bytes` is called.
#[derive(Clone, Copy)]
pub struct Binary<'a> {
    pub(crate) term: NifTerm,
    pub(crate) env: Env<'a>,
}

/// A sub-byte bitstring (`enif_is_binary` returned false).
///
/// The NIF API provides no inspection functions for sub-byte bitstrings.
/// A `Bitstring` can only be held and passed back to Erlang unchanged.
#[derive(Clone, Copy)]
pub struct Bitstring<'a> {
    pub(crate) term: NifTerm,
    // Env is stored for lifetime tracking only — sub-byte bitstrings have no
    // NIF inspection functions, so `env` is never read directly.
    #[allow(dead_code)]
    pub(crate) env: Env<'a>,
}

impl<'a> Binary<'a> {
    /// View the binary data as a byte slice.
    ///
    /// Zero-copy — the slice points directly into the BEAM heap. Valid for
    /// the lifetime `'a` of the environment.
    pub fn as_bytes(self) -> &'a [u8] {
        unsafe {
            let mut bin: crate::sys::NifBinary = std::mem::zeroed();
            crate::wrapper::binary::inspect_binary(self.env.as_ptr(), self.term, &mut bin);
            std::slice::from_raw_parts(bin.data, bin.size)
        }
    }

    /// Number of bytes in the binary.
    pub fn len(self) -> usize {
        self.as_bytes().len()
    }

    /// Returns `true` if the binary contains no bytes.
    pub fn is_empty(self) -> bool {
        self.len() == 0
    }

    /// Attempt to interpret the binary as a UTF-8 string.
    ///
    /// Zero-copy — the returned `&str` points into the BEAM heap with the
    /// full environment lifetime `'a`. Returns `Err(Utf8Error)` if the
    /// bytes are not valid UTF-8.
    ///
    /// Equivalent to `std::str::from_utf8(binary.as_bytes())`. You can also
    /// use `std::str::from_utf8(&binary)` via `Deref`, but the returned
    /// lifetime will be shorter (tied to the borrow, not `'a`).
    pub fn try_str(self) -> Result<&'a str, Utf8Error> {
        std::str::from_utf8(self.as_bytes())
    }

    /// Create a zero-copy sub-binary term from `pos..pos+len`.
    ///
    /// The returned `Binary` is a first-class BEAM term that references a
    /// range within the parent binary's heap data — no bytes are copied.
    ///
    /// Panics if `pos + len` exceeds the binary length.
    pub fn sub(self, pos: usize, len: usize) -> Binary<'a> {
        assert!(
            pos + len <= self.len(),
            "sub-binary out of bounds: pos({}) + len({}) > {}",
            pos, len, self.len()
        );
        let term = unsafe {
            crate::wrapper::binary::make_sub_binary(self.env.as_ptr(), self.term, pos, len)
        };
        Binary { term, env: self.env }
    }

    /// Allocate a new binary on the BEAM heap and copy `data` into it.
    pub fn from_bytes(env: Env<'a>, data: &[u8]) -> Binary<'a> {
        let mut term: NifTerm = 0;
        unsafe {
            let ptr = crate::wrapper::binary::make_new_binary(env.as_ptr(), data.len(), &mut term);
            std::ptr::copy_nonoverlapping(data.as_ptr(), ptr, data.len());
        }
        Binary { term, env }
    }
}

impl<'a> std::ops::Deref for Binary<'a> {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        self.as_bytes()
    }
}

impl<'a> AsRef<[u8]> for Binary<'a> {
    fn as_ref(&self) -> &[u8] {
        self.as_bytes()
    }
}

impl std::fmt::Debug for Binary<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Binary({} bytes)", self.len())
    }
}

impl PartialEq for Binary<'_> {
    fn eq(&self, other: &Self) -> bool {
        unsafe { crate::wrapper::term::is_identical(self.term, other.term) }
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
        let c = unsafe { crate::wrapper::term::compare(self.term, other.term) };
        c.cmp(&0)
    }
}

impl std::fmt::Debug for Bitstring<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Bitstring")
    }
}

impl PartialEq for Bitstring<'_> {
    fn eq(&self, other: &Self) -> bool {
        unsafe { crate::wrapper::term::is_identical(self.term, other.term) }
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
        let c = unsafe { crate::wrapper::term::compare(self.term, other.term) };
        c.cmp(&0)
    }
}

// ---------------------------------------------------------------------------
// BinaryBuilder — growable binary buffer (Vec<u8> model)
// ---------------------------------------------------------------------------

/// A growable binary buffer backed by `enif_alloc_binary`.
///
/// Mirrors `Vec<u8>`: tracks `len` (bytes written) and `capacity` (bytes
/// allocated) separately. Grows automatically via `enif_realloc_binary`
/// when appending would exceed capacity.
///
/// Call [`finish`] to shrink the allocation to the written length and
/// produce an immutable `Binary` term. If dropped without finishing,
/// the buffer is released automatically.
///
/// Implements [`std::io::Write`] for use with `write!` and friends.
///
/// [`finish`]: BinaryBuilder::finish
pub struct BinaryBuilder {
    bin: crate::sys::NifBinary,
    len: usize,
    released: bool,
}

impl BinaryBuilder {
    /// Create an empty builder with no allocation.
    pub fn new() -> BinaryBuilder {
        BinaryBuilder::with_capacity(0)
    }

    /// Create a builder with preallocated capacity.
    ///
    /// Panics if the allocation fails.
    pub fn with_capacity(capacity: usize) -> BinaryBuilder {
        let mut bin: crate::sys::NifBinary = unsafe { std::mem::zeroed() };
        let ok = unsafe { crate::wrapper::binary::alloc_binary(capacity, &mut bin) };
        assert!(ok, "enif_alloc_binary failed");
        BinaryBuilder { bin, len: 0, released: false }
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
    pub fn as_slice(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.bin.data, self.len) }
    }

    /// View the written bytes as a mutable slice.
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        unsafe { std::slice::from_raw_parts_mut(self.bin.data, self.len) }
    }

    /// Resize the buffer to `new_len`.
    ///
    /// If `new_len > len`, the new bytes are filled with `value`.
    /// If `new_len < len`, the buffer is truncated.
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

    /// Ensure there is room for at least `additional` more bytes.
    ///
    /// Panics if reallocation fails.
    pub fn reserve(&mut self, additional: usize) {
        let required = self.len + additional;
        if required <= self.bin.size {
            return;
        }
        let new_cap = required.max(self.bin.size.checked_mul(2).unwrap_or(required));
        let ok = unsafe { crate::wrapper::binary::realloc_binary(&mut self.bin, new_cap) };
        assert!(ok, "enif_realloc_binary failed");
    }

    /// Finalize the buffer into a `Binary` term.
    ///
    /// Shrinks the allocation to the exact written length, then transfers
    /// ownership to the BEAM.
    pub fn finish<'a>(mut self, env: Env<'a>) -> Binary<'a> {
        if self.len < self.bin.size {
            let ok = unsafe { crate::wrapper::binary::realloc_binary(&mut self.bin, self.len) };
            assert!(ok, "enif_realloc_binary failed on shrink");
        }
        self.released = true;
        let term = unsafe { crate::wrapper::binary::make_binary(env.as_ptr(), &mut self.bin) };
        Binary { term, env }
    }
}

impl Default for BinaryBuilder {
    fn default() -> Self {
        BinaryBuilder::new()
    }
}

impl std::io::Write for BinaryBuilder {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Drop for BinaryBuilder {
    fn drop(&mut self) {
        if !self.released {
            unsafe { crate::wrapper::binary::release_binary(&mut self.bin) };
        }
    }
}

impl std::ops::Deref for BinaryBuilder {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        self.as_slice()
    }
}

impl std::ops::DerefMut for BinaryBuilder {
    fn deref_mut(&mut self) -> &mut [u8] {
        self.as_mut_slice()
    }
}

impl AsRef<[u8]> for BinaryBuilder {
    fn as_ref(&self) -> &[u8] {
        self.as_slice()
    }
}

impl AsMut<[u8]> for BinaryBuilder {
    fn as_mut(&mut self) -> &mut [u8] {
        self.as_mut_slice()
    }
}

impl Extend<u8> for BinaryBuilder {
    fn extend<I: IntoIterator<Item = u8>>(&mut self, iter: I) {
        let iter = iter.into_iter();
        let (lower, _) = iter.size_hint();
        self.reserve(lower);
        for byte in iter {
            self.push(byte);
        }
    }
}

impl<'a> Extend<&'a u8> for BinaryBuilder {
    fn extend<I: IntoIterator<Item = &'a u8>>(&mut self, iter: I) {
        self.extend(iter.into_iter().copied());
    }
}

impl std::fmt::Debug for BinaryBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BinaryBuilder")
            .field("len", &self.len)
            .field("capacity", &self.bin.size)
            .finish()
    }
}

impl Binary<'_> {
    /// Deserialize a term from the external binary format.
    ///
    /// If `safe` is true, encoded atoms that don't already exist in the atom
    /// table are rejected. Returns `None` on decode failure.
    ///
    /// Wraps `enif_binary_to_term`.
    pub fn to_term<'a>(&self, env: Env<'a>, safe: bool) -> Option<Term<'a>> {
        let bytes = self.as_bytes();
        let opts = if safe { crate::sys::NIF_BIN2TERM_SAFE } else { 0 };
        let mut term: NifTerm = 0;
        let consumed = unsafe {
            crate::wrapper::binary::binary_to_term(
                env.as_ptr(),
                bytes.as_ptr(),
                bytes.len(),
                &mut term,
                opts,
            )
        };
        if consumed == 0 {
            None
        } else {
            Some(crate::term::RawTerm::new(env, term).resolve())
        }
    }
}

impl<'b> Encoder for Binary<'b> {
    fn encode<'a>(&self, env: Env<'a>) -> RawTerm<'a> {
        let term = unsafe { crate::wrapper::term::make_copy(env.as_ptr(), self.term) };
        RawTerm::new(env, term)
    }
}

impl<'a> Decoder<'a> for Binary<'a> {
    fn decode(term: Term<'a>) -> Result<Self, CodecError> {
        match term {
            Term::Binary(b) => Ok(b),
            _ => Err(CodecError::WrongType),
        }
    }
}

impl<'b> Encoder for Bitstring<'b> {
    fn encode<'a>(&self, env: Env<'a>) -> RawTerm<'a> {
        let term = unsafe { crate::wrapper::term::make_copy(env.as_ptr(), self.term) };
        RawTerm::new(env, term)
    }
}

impl<'a> Decoder<'a> for Bitstring<'a> {
    fn decode(term: Term<'a>) -> Result<Self, CodecError> {
        match term {
            Term::Bitstring(b) => Ok(b),
            _ => Err(CodecError::WrongType),
        }
    }
}
