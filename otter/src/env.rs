//! `Env<'a>`, `OwnedTermBuilder`, and `OwnedTerm`.

use std::cell::Cell;
use std::marker::PhantomData;

use crate::sys::{NifEnv, NifTerm};
use crate::term::Term;

// ---------------------------------------------------------------------------
// EnvKind
// ---------------------------------------------------------------------------

/// Distinguishes the context in which an `Env` was created.
///
/// The BEAM uses different internal env types for different contexts. Otter
/// tracks this so higher layers can enforce context-specific restrictions
/// (e.g. only `Load`/`Upgrade` envs may register resource types).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EnvKind {
    /// Standard NIF call environment.
    ProcessBound,
    /// Resource destructor, monitor, or select-stop callback environment.
    Callback,
    /// Load callback environment. Valid for resource type registration.
    Load,
    /// Upgrade callback environment. Valid for resource type registration.
    Upgrade,
    /// Unload callback environment.
    Unload,
    /// Process-independent environment created with `enif_alloc_env`.
    ProcessIndependent,
}

// ---------------------------------------------------------------------------
// Env<'a>
// ---------------------------------------------------------------------------

/// The NIF call environment. Carries a unique per-call lifetime `'a`.
///
/// The lifetime is synthesized in the generated `extern "C"` NIF wrapper by
/// borrowing a stack-allocated `()`. This means `'a` is strictly scoped to
/// one NIF call: the compiler rejects any attempt to store a `TypedTerm<'a>` past
/// the point where the NIF returns.
///
/// `PhantomData<*mut &'a u8>` makes `Env` *invariant* over `'a`. Without
/// invariance, the compiler would allow shortening or extending `'a` via
/// coercion, defeating the lifetime protection.
///
/// `Env` is `Copy` — it is two words on the stack.
#[derive(Clone, Copy)]
pub struct Env<'a> {
    pub kind: EnvKind,
    env: *mut NifEnv,
    // Invariant over 'a. *mut makes it invariant; &'a u8 anchors 'a.
    _id: PhantomData<*mut &'a u8>,
}

impl<'a> Env<'a> {
    /// Construct an `Env` from a raw pointer and a stack-lifetime marker.
    ///
    /// `_marker` must be a reference to a local variable in the `extern "C"`
    /// NIF entry function — this is what ties `'a` to the NIF call's stack
    /// frame. `kind` identifies the context.
    ///
    /// # Safety
    ///
    /// `env` must be a valid `ErlNifEnv` pointer for the entire duration of
    /// `'a`, and must not be freed or cleared while any `Env<'a>` or
    /// `TypedTerm<'a>` derived from it exists.
    #[inline]
    pub(crate) unsafe fn new(
        _marker: &'a (),
        env: *mut NifEnv,
        kind: EnvKind,
    ) -> Env<'a> {
        Env { kind, env, _id: PhantomData }
    }

    /// Return the raw `ErlNifEnv` pointer.
    #[inline]
    pub(crate) fn as_ptr(self) -> *mut NifEnv {
        self.env
    }
}

// Env is not Send or Sync: *mut NifEnv is neither, and Env must not cross
// thread boundaries. The compiler enforces this automatically via PhantomData.

// raise() and raise_badarg() are defined in term.rs after TypedTerm<'a> is declared,
// as an additional impl block on Env<'a>.

// ---------------------------------------------------------------------------
// OwnedTermBuilder / OwnedTerm
// ---------------------------------------------------------------------------

/// Builds a single message term in a process-independent environment, then
/// hands its heap to a process via the `enif_send` steal.
///
/// Terms are built directly on the builder and held as ordinary [`Term`]s.
/// They borrow the builder, so they cannot outlive it. Choose the message with
/// [`set`](Self::set), then [`build`](Self::build) consumes the builder into an
/// [`OwnedTerm`] whose heap is stolen on send — O(1), single-use.
pub struct OwnedTermBuilder {
    env: *mut NifEnv,
    // Borrowing `&self._anchor` gives `env()` its lifetime.
    _anchor: (),
    msg: Cell<NifTerm>,
}

/// A message term that owns its process-independent environment, ready to send.
/// Produced by [`OwnedTermBuilder::build`].
pub struct OwnedTerm {
    pub(crate) env: *mut NifEnv,
    pub(crate) msg: NifTerm,
}

// SAFETY: the BEAM's process-independent envs are designed for cross-thread
// use. Either may be created on one thread and sent from another.
unsafe impl Send for OwnedTermBuilder {}
unsafe impl Send for OwnedTerm {}

impl OwnedTermBuilder {
    /// Allocate a builder over a fresh process-independent environment.
    pub fn new() -> OwnedTermBuilder {
        let env = unsafe { crate::enif::alloc_env() };
        assert!(!env.is_null(), "enif_alloc_env returned null");
        OwnedTermBuilder { env, _anchor: (), msg: Cell::new(crate::enif::THE_NON_VALUE) }
    }

    /// Borrow the environment to build terms. The returned terms borrow the
    /// builder and cannot outlive it.
    pub fn env<'a>(&'a self) -> Env<'a> {
        // SAFETY: self.env is valid for 'a; &self._anchor ties 'a to this borrow.
        unsafe { Env::new(&self._anchor, self.env, EnvKind::ProcessIndependent) }
    }

    /// Choose `t` as the message to send. `t` must have been built in this
    /// builder's environment.
    pub fn set<'a>(&'a self, t: Term<'a>) {
        assert!(t.env.as_ptr() == self.env, "term was built in a different env");
        self.msg.set(t.term);
    }

    /// Consume the builder into a sendable [`OwnedTerm`].
    ///
    /// Panics if no term was [`set`](Self::set).
    pub fn build(self) -> OwnedTerm {
        let msg = self.msg.get();
        assert!(msg != crate::enif::THE_NON_VALUE, "build() called without set()");
        let env = self.env;
        // Transfer env ownership to OwnedTerm; skip OwnedTermBuilder::drop.
        std::mem::forget(self);
        OwnedTerm { env, msg }
    }
}

impl Default for OwnedTermBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for OwnedTermBuilder {
    fn drop(&mut self) {
        unsafe { crate::enif::free_env(self.env) };
    }
}

impl Drop for OwnedTerm {
    fn drop(&mut self) {
        unsafe { crate::enif::free_env(self.env) };
    }
}
