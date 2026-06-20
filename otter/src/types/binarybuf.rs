//! `BinaryBuf` — owned, growable binary buffer (`Vec<u8>` model).

use crate::types::{Binary, Env};

/// An owned, mutable binary buffer backed by `enif_alloc_binary`.
///
/// The RAII owner of a mutable `ErlNifBinary`: it owns its allocation directly
/// (no env, no brand), so it is a plain owned value — not a term, which is why
/// it lives here rather than under `types::terms`. `Drop` releases it
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
    ///
    /// `bin` MUST be an owned, mutable binary — one from `enif_alloc_binary` /
    /// `enif_term_to_binary` — never a read-only inspected binary. `BinaryBuf`
    /// reallocates via `enif_realloc_binary`, which silently allocates a mutable
    /// *copy* when handed a read-only source (orphaning the original); the
    /// owned-mutable precondition is what makes the in-place grow/shrink sound.
    #[crate::raw]
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
        Binary::from_raw(raw_term)
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
