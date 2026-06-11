//! `Env<'a>` and `OwnedEnv`.

use std::marker::PhantomData;

use crate::sys::{NifEnv, NifPid};
use crate::term::Term;
use crate::types::Pid;
use crate::wrapper;

// ---------------------------------------------------------------------------
// EnvKind
// ---------------------------------------------------------------------------

/// Distinguishes the context in which an `Env` was created.
///
/// The BEAM uses different internal env types for different contexts. Otter
/// tracks this so higher layers can enforce context-specific restrictions
/// (e.g. only `Init` envs may register resource types).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EnvKind {
    /// Standard NIF call environment.
    ProcessBound,
    /// Resource destructor, monitor, or select-stop callback environment.
    Callback,
    /// Load callback environment. Only valid for resource type registration.
    Init,
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
/// one NIF call: the compiler rejects any attempt to store a `Term<'a>` past
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
    /// `Term<'a>` derived from it exists.
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

// raise() and raise_badarg() are defined in term.rs after Term<'a> is declared,
// as an additional impl block on Env<'a>.

// ---------------------------------------------------------------------------
// OwnedEnv
// ---------------------------------------------------------------------------

/// A process-independent NIF environment for constructing and sending terms
/// from outside a NIF call (e.g. from a spawned OS thread).
///
/// Use [`send`] to build a term and dispatch it to an Erlang process in one
/// step. The env is cleared automatically after each send, ready for reuse.
///
/// [`send`]: OwnedEnv::send
pub struct OwnedEnv {
    env: *mut NifEnv,
}

// SAFETY: The BEAM's process-independent envs are designed for cross-thread
// use. OwnedEnv may be created on one thread and used on another.
unsafe impl Send for OwnedEnv {}

impl OwnedEnv {
    /// Allocate a new process-independent environment.
    pub fn new() -> OwnedEnv {
        let env = unsafe { wrapper::env::alloc_env() };
        assert!(!env.is_null(), "enif_alloc_env returned null");
        OwnedEnv { env }
    }

    /// Build a term and send it to `pid`.
    ///
    /// The closure receives a temporary [`Env`] backed by this environment
    /// and must return the term to send. After the closure returns the term
    /// is dispatched to `pid` and the environment is cleared.
    ///
    /// Returns `true` if the send succeeded (the target process was alive).
    ///
    /// After `enif_send` returns, this environment is cleared regardless of
    /// whether the send succeeded — the BEAM invalidates `msg_env` on every
    /// call.
    ///
    /// # Note
    ///
    /// When calling from outside a NIF call (e.g. an OS thread), `enif_send`
    /// is called with a null caller environment, which is the correct usage
    /// for non-scheduler threads.
    pub fn send<F>(&mut self, pid: &Pid, f: F) -> bool
    where
        F: FnOnce(Env<'_>) -> Term<'_>,
    {
        let marker = ();
        // SAFETY: self.env is valid; marker ties the lifetime to this frame.
        let env = unsafe { Env::new(&marker, self.env, EnvKind::ProcessIndependent) };
        let term = f(env).as_raw();
        let nif_pid = NifPid { pid: pid.term };
        // null caller_env = sending from outside a NIF call / scheduler thread.
        let ok = unsafe {
            wrapper::env::send(std::ptr::null_mut(), &nif_pid, self.env, term)
        };
        // enif_send always invalidates msg_env; clear our state to match.
        self.clear();
        ok
    }

    /// Clear the environment, invalidating all terms built in it.
    ///
    /// After clearing the env can be reused to build new terms. Normally
    /// you do not need to call this directly — [`send`] clears automatically.
    ///
    /// [`send`]: OwnedEnv::send
    pub fn clear(&mut self) {
        unsafe { wrapper::env::clear_env(self.env) };
    }
}

impl Default for OwnedEnv {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for OwnedEnv {
    fn drop(&mut self) {
        unsafe { wrapper::env::free_env(self.env) };
    }
}
