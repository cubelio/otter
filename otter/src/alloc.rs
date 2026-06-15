//! An opt-in `#[global_allocator]` backed by the BEAM allocator.
//!
//! [`EnifAlloc`] routes every Rust heap allocation in the NIF library through
//! `enif_alloc`/`enif_free`. This matters for hot code upgrade: `enif_free` is
//! the one free path that is valid across two independently compiled builds of
//! the library (a stable C ABI), whereas Rust's default allocator is
//! build-private. Routing allocations through the VM allocator is therefore a
//! building block for carrying state across the upgrade boundary safely (see
//! `docs/UPGRADE.md`).
//!
//! ## Why direct-linked, not `dlsym`
//!
//! The rest of otter resolves `enif_*` symbols at run time with `dlsym` (see
//! [`crate::enif`]). A global allocator cannot: it may be called for the very
//! first Rust allocation, before any initialization code runs, so it must not
//! depend on a resolution step that itself allocates. Instead this module
//! **direct-links** the two functions it needs as `extern "C"`. The BEAM
//! exports them, and the dynamic linker binds them when the NIF `.so` is
//! `dlopen`'d (`RTLD_NOW`) — before `nif_init`, before any Rust code. There is
//! no catch-22.
//!
//! ## Inert until you opt in
//!
//! [`EnifAlloc`] is always available, but it is referenced — and therefore
//! pulls in the direct-linked `enif_alloc`/`enif_free` symbols — only when you
//! install it as the global allocator with [`enif_global_allocator!`]. Until
//! then dead-code elimination drops it, so otter still links into ordinary,
//! non-BEAM binaries (its own `cargo test`, doc builds, etc.).
//!
//! Once you *do* install it, those two symbols are undefined in the object file
//! and resolved only when the BEAM loads the `.so` — so a crate that invokes
//! the macro links **only** as a NIF cdylib hosted by the BEAM, never as an
//! ordinary executable.
//!
//! [`enif_global_allocator!`]: crate::enif_global_allocator

use std::alloc::{GlobalAlloc, Layout};
use std::ffi::c_void;

// Direct-linked, not resolved via `dlsym` — see module docs.
unsafe extern "C" {
    fn enif_alloc(size: usize) -> *mut c_void;
    fn enif_free(ptr: *mut c_void);
}

/// One machine word, used to stash the allocation's base pointer just below
/// the aligned pointer handed to the caller.
const HEADER: usize = std::mem::size_of::<*mut u8>();

/// A [`GlobalAlloc`] backed by the BEAM allocator (`enif_alloc`/`enif_free`).
///
/// Install it in the final NIF cdylib with [`enif_global_allocator!`], or by
/// hand: `#[global_allocator] static A: EnifAlloc = EnifAlloc;`.
///
/// [`enif_global_allocator!`]: crate::enif_global_allocator
pub struct EnifAlloc;

// `enif_alloc`, like `malloc`, takes no alignment argument and guarantees only
// max-fundamental alignment (8 bytes on Linux x86-64). To honor an arbitrary
// `Layout::align`, we over-allocate and stash the original base pointer in the
// machine word just below the aligned pointer we hand out, then recover it in
// `dealloc` — the pointer passed to `enif_free` must be exactly the one
// `enif_alloc` returned.
unsafe impl GlobalAlloc for EnifAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let align = layout.align();
        // One word for the base header, plus slack to bump up to an aligned
        // boundary. Reject a size that would overflow `usize` rather than wrap.
        let total = match layout
            .size()
            .checked_add(align)
            .and_then(|n| n.checked_add(HEADER))
        {
            Some(n) => n,
            None => return std::ptr::null_mut(),
        };
        // SAFETY: `enif_alloc` is a BEAM-exported C function, bound by the
        // dynamic linker at load time.
        let base = unsafe { enif_alloc(total) } as *mut u8;
        if base.is_null() {
            return std::ptr::null_mut();
        }
        // First address at least HEADER past `base` and aligned to `align`
        // (align is always a power of two, per `Layout`).
        let aligned = (base as usize + HEADER + align - 1) & !(align - 1);
        let user = aligned as *mut u8;
        // Stash `base` in the word immediately below `user`.
        // SAFETY: `user - HEADER >= base` by construction, so this write lands
        // inside the block we just allocated.
        unsafe { user.cast::<*mut u8>().sub(1).write(base) };
        user
    }

    unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
        // SAFETY: `ptr` came from `alloc`, which stored the base one word below it.
        let base = unsafe { ptr.cast::<*mut u8>().sub(1).read() };
        // SAFETY: `base` is the exact pointer `enif_alloc` returned.
        unsafe { enif_free(base as *mut c_void) };
    }

    // `realloc` is intentionally left as the default (alloc + copy + dealloc) so
    // every reallocation stays on the enif path and through the header scheme;
    // `enif_realloc` cannot be used here because it does not preserve the
    // base-relative offset the header scheme depends on.
}

/// Install [`EnifAlloc`] as the `#[global_allocator]` of the current crate.
///
/// Invoke this once in your NIF cdylib (where a `#[global_allocator]` is legal):
///
/// ```ignore
/// otter::enif_global_allocator!();
/// ```
///
/// Invoking this makes the crate link only as a BEAM-hosted cdylib (see the
/// [`alloc`](crate::alloc) module docs).
#[macro_export]
macro_rules! enif_global_allocator {
    () => {
        #[global_allocator]
        static __OTTER_ENIF_GLOBAL_ALLOC: $crate::alloc::EnifAlloc = $crate::alloc::EnifAlloc;
    };
}
