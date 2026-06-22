mod binarybuf;
mod ops;
pub mod terms;
mod typed;

pub use binarybuf::BinaryBuf;
pub use ops::{deserialize, port_command, serialize};
pub use terms::*;
pub use typed::TypedTerm;

// enif types surfaced by the `Env` methods below — `term_type` returns
// `TermType`, `hash` takes `Hash`, `make_unique_integer` takes `UniqueInteger`.
// Re-exported here so callers can name them without the `raw` feature.
pub use enif_ffi::{Hash, TermType, UniqueInteger};

// `BigInt` (the `bigint` feature) re-exported as `otter::types::BigInt` so NIF
// authors share otter's exact `num-bigint` version — the `Encoder`/`Decoder`
// impls are tied to this crate's `BigInt`, so a semver-incompatible copy would
// not satisfy them. Only the type is exposed, not the whole `num_bigint` crate.
#[cfg(feature = "bigint")]
#[cfg_attr(docsrs, doc(cfg(feature = "bigint")))]
pub use num_bigint::BigInt;


use core::marker::PhantomData;
use core::sync::atomic::{AtomicU64, Ordering};

mod sealed {
    pub trait Sealed {}
}

/// The brand marker carried by every env and term: a zero-sized type that is
/// *invariant* over the lifetime `'id`. Invariance is what makes a brand
/// generative — two brands minted by different `for<'id>` entry points can never
/// be unified, so a term cannot leak from the env that produced it.
pub type Invariant<'id> = PhantomData<*mut &'id ()>;

// --- Env ---

pub(crate) type RawEnv = *mut enif_ffi::Env;

/// A handle to a BEAM environment, tagged with the generative brand `'id`.
///
/// `Env` is the central lifetime-safety mechanism. It is a sealed, `Copy` trait
/// implemented by each env *kind* ([`CallEnv`], [`InitEnv`], [`CallbackEnv`],
/// [`DeinitEnv`], [`OwnedEnv`], and the kind-erased [`AnyEnv`]). The brand `'id`
/// is minted fresh and non-escaping for each entry point, so terms branded by
/// one env are rejected at compile time when used with another — see the
/// [crate-level overview](crate#the-mental-model-branded-envs-and-lazy-terms).
///
/// The methods here are the *generic verbs* valid on any env kind. Context-
/// specific verbs live inherent on the kind that carries the context
/// ([`CallEnv::raise`], [`InitEnv::set_option_delay_halt`], …).
pub trait Env<'id>: Copy + sealed::Sealed {
    /// The raw `*mut enif_ffi::Env` pointer this handle wraps.
    fn raw_env(&self) -> RawEnv;

    /// The kind-erased handle terms are built against. Every env kind downcasts
    /// to the same `AnyEnv` for a given brand.
    fn as_any_env(self) -> AnyEnv<'id> {
        AnyEnv { raw_env: self.raw_env(), _id: PhantomData }
    }

    /// The dynamic type of `term` (`enif_term_type`). `None` for a type code
    /// this otter build does not recognize (a newer-OTP type).
    fn term_type(self, term: impl Term<'id>) -> Option<enif_ffi::TermType> {
        let code = unsafe { enif_ffi::term_type(self.raw_env(), term.raw_term()) };
        enif_ffi::TermType::from_raw(code)
    }

    /// Hash a term (`enif_hash`). `algorithm` is `Phash2` (portable) or
    /// `InternalHash` (node-local, faster).
    fn hash(self, algorithm: enif_ffi::Hash, term: impl Term<'id>, salt: u64) -> u64 {
        unsafe { enif_ffi::hash(algorithm, term.raw_term(), salt) }
    }

    /// Tell the scheduler how much of the timeslice this NIF used
    /// (`enif_consume_timeslice`). `true` if the timeslice is exhausted.
    fn consume_timeslice(self, percent: i32) -> bool {
        unsafe { enif_ffi::consume_timeslice(self.raw_env(), percent) != 0 }
    }

    /// Whether the calling process is still alive
    /// (`enif_is_current_process_alive`).
    fn is_current_process_alive(self) -> bool {
        unsafe { enif_ffi::is_current_process_alive(self.raw_env()) != 0 }
    }

    /// Create a unique integer (`enif_make_unique_integer`). `properties` is a
    /// bitmask of `UniqueInteger::POSITIVE` / `MONOTONIC`.
    fn make_unique_integer(self, properties: enif_ffi::UniqueInteger) -> Integer<'id> {
        let raw = unsafe { enif_ffi::make_unique_integer(self.raw_env(), properties) };
        Integer::from_raw(raw)
    }
}

/// The kind-erased environment handle terms are built against. Every concrete
/// env kind ([`CallEnv`], [`InitEnv`], …) downcasts to this via
/// [`Env::as_any_env`].
#[derive(Clone, Copy)]
pub struct AnyEnv<'id> {
    raw_env: RawEnv,
    _id: Invariant<'id>,
}

impl<'id> sealed::Sealed for AnyEnv<'id> {}

impl<'id> Env<'id> for AnyEnv<'id> {
    fn raw_env(&self) -> RawEnv {
        self.raw_env
    }
}

// --- VM-provided env kinds ---
//
// The VM hands a NIF call or callback a raw environment pointer; codegen wraps
// it in the matching concrete kind so context-specific verbs are typed — wrong
// kind is a compile error, no runtime tag (this is the lifted `EnvKind`). Each
// kind is structurally an `AnyEnv`; they differ only as types.
//
// Each `with_*` entry mints the brand through a `for<'id>` closure, so every
// call gets a unique, non-escaping brand (the GhostCell construction); `R` is
// brand-free by construction — exactly the raw `Term`/status word the C ABI
// returns.
//
// # Safety (all `with_*` entries)
// `raw` must be the live environment pointer the VM supplied for this callback,
// used only for the duration of `f`.

/// The process-bound environment handed to a NIF call.
#[derive(Clone, Copy)]
pub struct CallEnv<'id> {
    raw_env: RawEnv,
    _id: Invariant<'id>,
}

impl<'id> sealed::Sealed for CallEnv<'id> {}

impl<'id> Env<'id> for CallEnv<'id> {
    fn raw_env(&self) -> RawEnv {
        self.raw_env
    }
}

impl CallEnv<'_> {
    /// Enter a NIF call with a freshly branded [`CallEnv`]. See the env-kind
    /// note above for the brand guarantee.
    ///
    /// # Safety
    /// `raw` must be the live env pointer the VM supplied for this call.
    pub unsafe fn with_raw<R>(raw: RawEnv, f: impl for<'id> FnOnce(CallEnv<'id>) -> R) -> R {
        f(CallEnv { raw_env: raw, _id: PhantomData })
    }
}

/// The environment handed to the `load` and `upgrade` callbacks.
#[derive(Clone, Copy)]
pub struct InitEnv<'id> {
    raw_env: RawEnv,
    _id: Invariant<'id>,
}

impl<'id> sealed::Sealed for InitEnv<'id> {}

impl<'id> Env<'id> for InitEnv<'id> {
    fn raw_env(&self) -> RawEnv {
        self.raw_env
    }
}

impl InitEnv<'_> {
    /// Enter a `load`/`upgrade` callback with a freshly branded [`InitEnv`].
    ///
    /// # Safety
    /// `raw` must be the live env pointer the VM supplied for this callback.
    pub unsafe fn with_raw<R>(raw: RawEnv, f: impl for<'id> FnOnce(InitEnv<'id>) -> R) -> R {
        f(InitEnv { raw_env: raw, _id: PhantomData })
    }
}

/// The environment handed to resource callbacks (destructor, monitor-down, …).
#[derive(Clone, Copy)]
pub struct CallbackEnv<'id> {
    raw_env: RawEnv,
    _id: Invariant<'id>,
}

impl<'id> sealed::Sealed for CallbackEnv<'id> {}

impl<'id> Env<'id> for CallbackEnv<'id> {
    fn raw_env(&self) -> RawEnv {
        self.raw_env
    }
}

impl CallbackEnv<'_> {
    /// Enter a resource callback with a freshly branded [`CallbackEnv`].
    ///
    /// # Safety
    /// `raw` must be the live env pointer the VM supplied for this callback.
    pub unsafe fn with_raw<R>(raw: RawEnv, f: impl for<'id> FnOnce(CallbackEnv<'id>) -> R) -> R {
        f(CallbackEnv { raw_env: raw, _id: PhantomData })
    }
}

/// The environment handed to the `unload` callback.
#[derive(Clone, Copy)]
pub struct DeinitEnv<'id> {
    raw_env: RawEnv,
    _id: Invariant<'id>,
}

impl<'id> sealed::Sealed for DeinitEnv<'id> {}

impl<'id> Env<'id> for DeinitEnv<'id> {
    fn raw_env(&self) -> RawEnv {
        self.raw_env
    }
}

impl DeinitEnv<'_> {
    /// Enter the `unload` callback with a freshly branded [`DeinitEnv`].
    ///
    /// # Safety
    /// `raw` must be the live env pointer the VM supplied for this callback.
    pub unsafe fn with_raw<R>(raw: RawEnv, f: impl for<'id> FnOnce(DeinitEnv<'id>) -> R) -> R {
        f(DeinitEnv { raw_env: raw, _id: PhantomData })
    }
}

fn send_move_(env: RawEnv, pid: &LocalPid, msg_env: &mut OwnedEnvArena, msg: OwnedEnvTerm) -> bool {
    assert!(!msg_env.is_dirty);
    let msg = msg_env.unwrap_term(msg);
    let ok = unsafe { enif_ffi::send(env, &pid.pid, msg_env.env, msg) != 0 };
    msg_env.is_dirty = ok;
    ok
}

fn send_copy_<'a>(env: RawEnv, pid: &LocalPid, msg: impl Term<'a>) -> bool {
    unsafe { enif_ffi::send(env, &pid.pid, std::ptr::null_mut(), msg.raw_term()) != 0 }
}

// Sending (`enif_send`) is a 2×2 of independent choices, exposed as four free
// verbs. They are verbs, not methods: the env and pid are ingredients of the
// operation, not its owner.
//
// * **`_copy` vs `_move`** — how the payload reaches the recipient. `_copy`
//   copies a live term into the recipient's mailbox (`enif_send` with a NULL
//   `msg_env`). `_move` transplants (steals) an entire [`OwnedEnvArena`] heap
//   into the message (`enif_send` with the arena as `msg_env`) — O(1), no copy;
//   the arena is left dirty and must be [`clear`](OwnedEnvArena::clear)ed before
//   reuse.
// * **plain vs `_from`** — who the message is attributed to. The plain verbs
//   send with a NULL `caller_env`, for use from a non-scheduler thread that has
//   no process context. The `_from` verbs take a [`CallingEnv`] (a live NIF call
//   or callback) as the `caller_env`, so the BEAM attributes the message to the
//   calling process (send-trace, seq-trace, reductions, process-aware enqueue).
//
// All four return `true` if the message was delivered (the target was alive),
// `false` otherwise, mirroring `enif_send`.

/// Steal-send `msg` to `pid` from a non-scheduler thread (NULL caller).
///
/// Transplants `msg_env`'s entire heap into the message in O(1) — no copy — then
/// leaves the arena dirty; [`clear`](OwnedEnvArena::clear) it before reusing it.
/// `msg` must have been [`export`](OwnedEnv::export)ed from *this* arena.
///
/// The off-thread counterpart to [`send_move_from`]. Use that instead from
/// inside a NIF to attribute the message to the calling process.
pub fn send_move(pid: &LocalPid, msg_env: &mut OwnedEnvArena, msg: OwnedEnvTerm) -> bool {
    send_move_(std::ptr::null_mut(), pid, msg_env, msg)
}

/// Copy-send a live term `msg` to `pid` from a non-scheduler thread (NULL
/// caller).
///
/// Copies `msg` into the recipient's mailbox (`enif_send`, NULL `msg_env`). The
/// off-thread counterpart to [`send_copy_from`]; use that from inside a NIF to
/// attribute the message to the calling process. To steal an owned-env heap
/// instead of copying, use [`send_move`].
pub fn send_copy<'a>(pid: &LocalPid, msg: impl Term<'a>) -> bool {
    send_copy_(std::ptr::null_mut(), pid, msg)
}

/// Steal-send `msg` to `pid` from inside a NIF, attributed to the calling
/// process.
///
/// Like [`send_move`] (transplants `msg_env`'s heap in O(1), leaving the arena
/// dirty), but passes `calling_env` as the `caller_env`, so the BEAM attributes
/// the message to the calling process. `calling_env` must be a live
/// [`CallEnv`]/[`CallbackEnv`]; `msg` must have been
/// [`export`](OwnedEnv::export)ed from *this* arena.
pub fn send_move_from<'id>(calling_env: impl CallingEnv<'id>, pid: &LocalPid, msg_env: &mut OwnedEnvArena, msg: OwnedEnvTerm) -> bool {
    send_move_(calling_env.raw_env(), pid, msg_env, msg)
}

/// Copy-send a live term `msg` to `pid` from inside a NIF, attributed to the
/// calling process.
///
/// Like [`send_copy`] (copies `msg` into the recipient's mailbox), but passes
/// `calling_env` as the `caller_env`, so the BEAM attributes the message to the
/// calling process. `calling_env` must be a live [`CallEnv`]/[`CallbackEnv`];
/// `msg` is a term of any brand. This is the common in-NIF send.
pub fn send_copy_from<'id, 'a>(calling_env: impl CallingEnv<'id>, pid: &LocalPid, msg: impl Term<'a>) -> bool {
    send_copy_(calling_env.raw_env(), pid, msg)
}

/// The env kinds that carry a live process/scheduler context — those you can
/// send a message or issue a port command *from*, with caller attribution
/// (`enif_send`/`enif_port_command` with a non-NULL `caller_env`). Grouping
/// trait for verbs that accept any such env.
///
/// Implemented by [`CallEnv`] and [`CallbackEnv`]. Not [`InitEnv`]/[`DeinitEnv`]
/// (module load/unload — no caller), and not [`OwnedEnv`] (sends with a NULL
/// caller instead). Sealed transitively through [`Env`].
pub trait CallingEnv<'id>: Env<'id> {}

impl<'id> CallingEnv<'id> for CallEnv<'id> {}
impl<'id> CallingEnv<'id> for CallbackEnv<'id> {}

// --- Exceptions ---

/// Proof that an exception is pending on a [`CallEnv`].
///
/// A term-less typestate token: it can only be produced by an operation that
/// raises or detects a pending exception, so holding one means the env is in the
/// pending-exception state in which no further env operation is valid. Propagate
/// it straight out of the NIF with `?`; the generated wrapper returns the
/// non-value word and the BEAM raises the pending exception. (No term is kept —
/// detection via `has_pending_exception` yields no reason term, and the returned
/// word is ignored once an exception is pending.)
pub struct Raised<'id> {
    _id: Invariant<'id>,
}

impl<'id> CallEnv<'id> {
    /// Raise an exception with `reason` (`enif_raise_exception`). Always `Err`,
    /// generic over the success type so it fits any position:
    /// `return env.raise(reason)`.
    pub fn raise<T>(self, reason: impl Term<'id>) -> Result<T, Raised<'id>> {
        unsafe { enif_ffi::raise_exception(self.raw_env(), reason.raw_term()) };
        Err(Raised { _id: PhantomData })
    }

    /// Raise a `badarg` error (`enif_make_badarg`). Always `Err`.
    pub fn badarg<T>(self) -> Result<T, Raised<'id>> {
        unsafe { enif_ffi::make_badarg(self.raw_env()) };
        Err(Raised { _id: PhantomData })
    }

    /// If the env has a pending exception, return `Err(Raised)`; otherwise
    /// `Ok(term)` (`enif_has_pending_exception`). The safe way to call a `raw`
    /// enif function that may raise: pass its result straight through.
    pub fn check_raised(self, term: AnyTerm<'id>) -> Result<AnyTerm<'id>, Raised<'id>> {
        if unsafe { enif_ffi::has_pending_exception(self.raw_env(), std::ptr::null_mut()) } != 0 {
            Err(Raised { _id: PhantomData })
        } else {
            Ok(term)
        }
    }
}

// --- Term ---

pub(crate) type RawTerm = enif_ffi::Term;

/// The BEAM's non-value marker (`THE_NON_VALUE`). Returned from a NIF whose
/// `Result` raised: the word is ignored once an exception is pending, and the
/// BEAM raises the pending exception on return.
pub(crate) const THE_NON_VALUE: RawTerm = 0;

/// A BEAM term branded to the env `'id` that produced it.
///
/// `Term` is the universal term-input trait: every otter term type implements it,
/// and functions that accept a term take `impl Term<'id>`, so you pass concrete
/// types directly (no `.encode()`) and a term of the wrong brand fails to
/// compile. Sealed — it cannot be implemented outside the crate.
///
/// Env-portable types ([`Atom`], [`LocalPid`], [`LocalPort`]) implement
/// [`FreeTerm`] and so satisfy an `impl Term<'id>` slot for *every* brand;
/// env-bound types carry only their own brand. The brand constraint is
/// load-bearing: the BEAM treats a cross-env term as undefined behavior.
pub trait Term<'id>: sealed::Sealed {
    /// The raw machine word backing this term.
    fn raw_term(self) -> RawTerm;

    /// Copy this term into another environment (`enif_make_copy`), producing a
    /// term branded to the destination. The general cross-env copy — distinct
    /// from same-brand [`Encoder`](crate::codec::Encoder) (which wraps for free)
    /// and from [`OwnedEnvArena`] `copy_out` (the arena exit).
    fn copy_to<'dst>(self, env: impl Env<'dst>) -> AnyTerm<'dst>
    where
        Self: Sized,
    {
        let raw = unsafe { enif_ffi::make_copy(env.raw_env(), self.raw_term()) };
        AnyTerm::wrap(raw, env)
    }
}

/// Marker for env-portable terms: those valid in *any* env, hence branded for
/// every `'id` at once. Implemented by [`Atom`], [`LocalPid`], and [`LocalPort`]
/// — tagged immediates and locality-validated handles with no heap data to
/// outlive an env. A `FreeTerm` satisfies an `impl Term<'id>` argument for any
/// brand.
pub trait FreeTerm: for<'id> Term<'id> {}

/// The bare term word, carrying only the brand `'id` — the env is *not* stored.
///
/// The fastest term representation: no `enif_term_type` call has been made and no
/// data has been read off the BEAM heap. Resolve it to a known type with
/// `resolve(env)` (yielding a [`TypedTerm`]), or decode it directly.
/// `#[repr(transparent)]` over the raw word (the brand is a ZST), so a
/// `&[RawTerm]` can be viewed in place as `&[AnyTerm<'id>]` (used by
/// [`TupleView`]).
#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct AnyTerm<'id> {
    raw_term: RawTerm,
    _id: Invariant<'id>,
}

impl<'id> AnyTerm<'id> {
    #[crate::raw]
    pub(crate) fn wrap(raw_term: RawTerm, _env: impl Env<'id>) -> Self {
        Self { raw_term, _id: PhantomData }
    }
}

impl sealed::Sealed for AnyTerm<'_> {}

impl<'id> Term<'id> for AnyTerm<'id> {
    fn raw_term(self) -> RawTerm {
        self.raw_term
    }
}


/// Process-global source of arena generation stamps. Handed out monotonically,
/// so every `(arena, post-clear state)` gets a value no other arena ever holds.
/// Wrap is unreachable at 2^64 stamps.
static GENERATION: AtomicU64 = AtomicU64::new(1);

fn next_generation() -> u64 {
    GENERATION.fetch_add(1, Ordering::Relaxed)
}

/// A portable handle to a term stored in an [`OwnedEnvArena`]. `Copy` and
/// unbranded so it can cross the `run` closure / be carried to a worker thread;
/// its `version` is a globally-unique generation stamp, so it is only valid
/// against the exact arena-generation that produced it.
#[derive(Clone, Copy)]
pub struct OwnedEnvTerm {
    version: u64,
    term: RawTerm,
}

/// A reusable, process-independent environment for building messages outside a
/// NIF call (e.g. on a spawned OS thread).
///
/// Wraps an `enif_alloc_env` heap. Build terms inside [`run`](Self::run) — the
/// closure's branded [`OwnedEnv`] keeps them from escaping — and
/// [`export`](OwnedEnv::export) the one you want to a portable [`OwnedEnvTerm`].
/// Then either copy-send it ([`send_copy`]) or steal-send the whole arena heap in
/// O(1) ([`send_move`], which leaves the arena dirty until you [`clear`](Self::clear)
/// it). The arena is reusable: `clear` wipes the heap and bumps the generation
/// stamp, invalidating every term exported before it.
pub struct OwnedEnvArena {
    env: RawEnv,
    version: u64,
    is_dirty: bool,
}

impl Default for OwnedEnvArena {
    fn default() -> Self {
        Self::new()
    }
}

impl OwnedEnvArena {
    /// Allocate a fresh arena (`enif_alloc_env`). Panics if the VM returns null.
    /// Also available as [`Default`].
    pub fn new() -> Self {
        let env = unsafe { enif_ffi::alloc_env() };
        assert!(!env.is_null(), "enif_alloc_env returned null");
        OwnedEnvArena { env, version: next_generation(), is_dirty: false }
    }

    /// Drop all stored terms and wipe the env heap, in lockstep, for reuse.
    /// Takes a fresh generation stamp so every term stored before the clear is
    /// invalidated.
    pub fn clear(&mut self) {
        unsafe { enif_ffi::clear_env(self.env) };
        self.version = next_generation();
        self.is_dirty = false;
    }

    /// Copy a term from any env into this arena (`enif_make_copy`), returning a
    /// portable [`OwnedEnvTerm`]. The standalone counterpart to
    /// [`OwnedEnv::export`] when you are not inside [`run`](Self::run).
    pub fn copy_in<'a>(&mut self, term: impl Term<'a>) -> OwnedEnvTerm {
        assert!(!self.is_dirty);
        self.wrap_term(unsafe { enif_ffi::make_copy(self.env, term.raw_term()) })
    }

    /// Copy a stored [`OwnedEnvTerm`] out into `env` (`enif_make_copy`),
    /// re-branding it to `env`'s `'id`. Panics if `oterm` does not belong to this
    /// arena-generation.
    pub fn copy_out<'a>(&self, oterm: OwnedEnvTerm, env: impl Env<'a>) -> AnyTerm<'a> {
        assert!(!self.is_dirty);
        let remote_term = unsafe { enif_ffi::make_copy(env.raw_env(), self.unwrap_term(oterm)) };
        AnyTerm { raw_term: remote_term, _id: PhantomData }
    }

    fn wrap_term(&self, raw_term: RawTerm) -> OwnedEnvTerm {
        assert!(!self.is_dirty);
        OwnedEnvTerm { version: self.version, term: raw_term }
    }

    fn unwrap_term(&self, oterm: OwnedEnvTerm) -> RawTerm {
        assert!(!self.is_dirty);
        // The generation stamp is globally unique, so a matching version
        // identifies this exact arena-generation — and since an arena's env
        // pointer is fixed for its life, equal versions imply the same env. The
        // version check alone is therefore sufficient (no env-pointer compare,
        // which could otherwise alias a freed-then-reused env).
        assert!(self.version == oterm.version, "OwnedEnvTerm used with a different arena or after clear");
        oterm.term
    }

    /// Run `f` with a freshly branded [`OwnedEnv`] over this arena. Build terms
    /// inside the closure and [`export`](OwnedEnv::export) any you need to keep;
    /// the brand prevents a live term from escaping in `R`. Panics if the arena
    /// is dirty (steal-sent but not yet [`clear`](Self::clear)ed).
    pub fn run<R>(&mut self, f: impl for<'id> FnOnce(OwnedEnv<'_, 'id>) -> R) -> R {
        assert!(!self.is_dirty);
        f(OwnedEnv { owner: self, _id: PhantomData })
    }
}

impl Drop for OwnedEnvArena {
    fn drop(&mut self) {
        unsafe { enif_ffi::free_env(self.env) };
    }
}


/// A branded handle to a process-independent environment, handed to the closure
/// by [`OwnedEnvArena::run`]. It is an [`Env`] like any other kind — generic verbs
/// take `impl Env<'id>` — and additionally carries the owned-env-specific verbs
/// `export`/`import` for moving terms across the closure boundary.
#[derive(Clone, Copy)]
pub struct OwnedEnv<'a, 'id> {
    owner: &'a OwnedEnvArena,
    _id: Invariant<'id>,
}

impl<'a, 'id> OwnedEnv<'a, 'id> {
    /// Save a term built in this env to a portable [`OwnedEnvTerm`] that can
    /// outlive the [`run`](OwnedEnvArena::run) closure (e.g. to steal-send later).
    /// The term must already live in this arena's heap.
    pub fn export(self, term: impl Term<'id>) -> OwnedEnvTerm {
        self.owner.wrap_term(term.raw_term())
    }

    /// Recover a branded [`AnyTerm`] from an [`OwnedEnvTerm`] previously
    /// [`export`](Self::export)ed from this same arena-generation. Panics if it
    /// belongs to a different arena or a pre-[`clear`](OwnedEnvArena::clear) state.
    pub fn import(self, oterm: OwnedEnvTerm) -> AnyTerm<'id> {
        AnyTerm { raw_term: self.owner.unwrap_term(oterm), _id: PhantomData }
    }
}

impl<'a, 'id> sealed::Sealed for OwnedEnv<'a, 'id> {}

impl<'a, 'id> Env<'id> for OwnedEnv<'a, 'id> {
    fn raw_env(&self) -> RawEnv {
        self.owner.env
    }
}

#[cfg(test)]
mod brand_tests {
    use super::*;

    // Ties an env and a term to ONE brand: only a term from this env type-checks.
    fn use_in<'id>(_env: impl Env<'id>, t: AnyTerm<'id>) -> RawTerm {
        t.raw_term()
    }

    // Two independently branded envs are distinct: a term from env 1 used with
    // env 2's brand is rejected at compile time — the 1:1 brand↔identity
    // guarantee. The same-brand call compiles; uncomment the cross-brand line to
    // re-verify rejection ("borrowed data escapes outside of closure": the two
    // invariant brands from the nested `for<'id>` closures cannot be unified).
    #[allow(dead_code)]
    fn brands_are_distinct() {
        unsafe {
            CallEnv::with_raw(std::ptr::null_mut(), |e1| {
                CallEnv::with_raw(std::ptr::null_mut(), |e2| {
                    let t1 = AnyTerm::wrap(0, e1);
                    let _ = use_in(e1, t1); // same brand: compiles
                    let _ = e2;
                    // let _ = use_in(e2, t1); // cross brand: rejected
                });
            });
        }
    }
}

#[cfg(test)]
mod owned_env_tests {
    use super::*;

    // Positive: build a term in the scope, save it, return the index. Compiles.
    #[allow(dead_code)]
    fn positive(b: &mut OwnedEnvArena) -> OwnedEnvTerm {
        b.run(|env| {
            let t = AnyTerm::wrap(0, env);
            env.export(t)
        })
    }

    // Escape test — verified to FAIL with "lifetime may not live long enough"
    // (`AnyTerm<'id>` is invariant over `'id`, so the rigid brand can't leak
    // into `run`'s return type `R`). Kept commented so the crate builds; uncomment
    // to re-verify.
    //
    // fn escape(b: &mut OwnedEnvArena) {
    //     let _leaked = b.run(|env| env.import(OwnedEnvTerm { version: b.version, term: 0 }));
    // }
}
