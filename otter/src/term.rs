//! `Term<'a>` and `TypedTerm<'a>`.

use crate::codec::{CodecError, Decoder, Encoder};
use crate::env::{Env, EnvKind};
use crate::sys::{NifHash, NifTerm, NifTermType, NifUniqueInteger};
use crate::types::{
    Atom, Binary, Bitstring, Float, Fun, Integer, List, Map, Pid, Port, Reference, Tuple,
};

// ---------------------------------------------------------------------------
// Term
// ---------------------------------------------------------------------------

/// Level 1: the bare `ERL_NIF_TERM` machine word plus its environment.
///
/// Zero work done — no type check, no data extraction. The fastest possible
/// way to hold a received term. Call `resolve()` to pay the cost of one
/// `enif_term_type` call and produce a typed `TypedTerm<'a>`.
///
/// `Term` is a received type. You cannot construct one from scratch —
/// all term construction goes through concrete types (`Atom::intern`, `Map::new`,
/// etc.), which always produce a known type.
#[derive(Clone, Copy)]
pub struct Term<'a> {
    pub(crate) term: NifTerm,
    pub(crate) env: Env<'a>,
}

impl<'a> Term<'a> {
    /// Wrap a raw term pointer. Used internally by NIF argument unpacking.
    #[inline]
    pub(crate) fn new(env: Env<'a>, term: NifTerm) -> Term<'a> {
        Term { term, env }
    }

    /// The environment this term belongs to.
    #[inline]
    pub fn env(self) -> Env<'a> {
        self.env
    }

    /// The underlying machine word. Returned directly from a NIF with zero
    /// additional work.
    #[inline]
    pub fn as_raw(self) -> NifTerm {
        self.term
    }

    /// Resolve to a typed `TypedTerm<'a>` by calling `enif_term_type`.
    /// Exactly one NIF call regardless of variant. Binary-tagged terms
    /// surface as [`TypedTerm::Bitstring`]; call [`Bitstring::is_binary`] or
    /// [`Bitstring::try_into_binary`] to refine.
    ///
    /// `None` if the term's type is one this otter build does not recognize (a
    /// type added by a newer OTP). The term is still a valid [`Term`] — the
    /// caller already holds it — so callers that want to pass it through can
    /// continue to use the original `Term`.
    pub fn resolve(self) -> Option<TypedTerm<'a>> {
        Some(match self.env.term_type(self)? {
            NifTermType::Atom      => TypedTerm::Atom(Atom::from_raw(self.term)),
            NifTermType::Bitstring => TypedTerm::Bitstring(Bitstring { term: self.term, env: self.env }),
            NifTermType::Float     => TypedTerm::Float(Float { term: self.term, env: self.env }),
            NifTermType::Fun       => TypedTerm::Fun(Fun { term: self.term, env: self.env }),
            NifTermType::Integer   => TypedTerm::Integer(Integer { term: self.term, env: self.env }),
            NifTermType::List      => TypedTerm::List(List { term: self.term, env: self.env }),
            NifTermType::Map       => TypedTerm::Map(Map { term: self.term, env: self.env }),
            NifTermType::Pid       => TypedTerm::Pid(Pid { term: self.term }),
            NifTermType::Port      => TypedTerm::Port(Port { term: self.term }),
            NifTermType::Reference => TypedTerm::Reference(Reference { term: self.term, env: self.env }),
            NifTermType::Tuple     => TypedTerm::Tuple(Tuple { term: self.term, env: self.env }),
        })
    }

    /// The raw `enif_term_type` code for this term, including any value a newer
    /// OTP may return that [`resolve`](Self::resolve) maps to `None`.
    #[cfg(feature = "raw")]
    pub fn term_type_raw(self) -> std::ffi::c_int {
        unsafe { crate::enif::term_type(self.env.as_ptr(), self.term) }
    }
}

// ---------------------------------------------------------------------------
// TypedTerm
// ---------------------------------------------------------------------------

/// Level 2: typed enum. One `enif_term_type` call has been made.
///
/// The correct variant is known. Data is still on the BEAM heap — nothing
/// has been extracted.
///
/// Mirrors BEAM's `ErlNifTermType` exactly: byte-aligned binaries and
/// sub-byte bitstrings share the [`Bitstring`](Self::Bitstring) variant
/// because BEAM treats every binary as a bitstring. Refine with
/// [`Bitstring::is_binary`] / [`Bitstring::try_into_binary`] if you need
/// the byte-aligned distinction.
#[derive(Clone, Copy)]
pub enum TypedTerm<'a> {
    Atom(Atom),
    Bitstring(Bitstring<'a>),
    Float(Float<'a>),
    Fun(Fun<'a>),
    Integer(Integer<'a>),
    List(List<'a>),
    Map(Map<'a>),
    Pid(Pid),
    Port(Port),
    Reference(Reference<'a>),
    Tuple(Tuple<'a>),
}

impl<'a> TypedTerm<'a> {
    /// Extract the underlying machine word. Discards the variant tag.
    /// Use this when returning a `TypedTerm` from a NIF at the C boundary.
    pub fn as_raw(self) -> NifTerm {
        match self {
            TypedTerm::Atom(v)      => v.term,
            TypedTerm::Bitstring(v) => v.term,
            TypedTerm::Float(v)     => v.term,
            TypedTerm::Fun(v)       => v.term,
            TypedTerm::Integer(v)   => v.term,
            TypedTerm::List(v)      => v.term,
            TypedTerm::Map(v)       => v.term,
            TypedTerm::Pid(v)       => v.term,
            TypedTerm::Port(v)      => v.term,
            TypedTerm::Reference(v) => v.term,
            TypedTerm::Tuple(v)     => v.term,
        }
    }
}

impl<'a> PartialEq for Term<'a> {
    fn eq(&self, other: &Self) -> bool {
        unsafe { crate::enif::is_identical(self.term, other.term) != 0 }
    }
}

impl<'a> Eq for Term<'a> {}

impl<'a> PartialOrd for Term<'a> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<'a> Ord for Term<'a> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let c = unsafe { crate::enif::compare(self.term, other.term) };
        c.cmp(&0)
    }
}

impl<'a> PartialEq for TypedTerm<'a> {
    fn eq(&self, other: &Self) -> bool {
        unsafe { crate::enif::is_identical(self.as_raw(), other.as_raw()) != 0 }
    }
}

impl<'a> Eq for TypedTerm<'a> {}

impl<'a> PartialOrd for TypedTerm<'a> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<'a> Ord for TypedTerm<'a> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let c = unsafe { crate::enif::compare(self.as_raw(), other.as_raw()) };
        c.cmp(&0)
    }
}

impl<'a> TypedTerm<'a> {
    /// Serialize this term to the external binary format.
    ///
    /// Returns `None` if serialization fails (should not happen for valid terms).
    ///
    /// Wraps `enif_term_to_binary`.
    pub fn to_binary(self, env: Env<'a>) -> Option<Binary<'a>> {
        let mut bin: crate::sys::NifBinary = unsafe { std::mem::zeroed() };
        if env.term_to_binary(self, &mut bin) {
            // term_to_binary allocates via alloc_binary; make_binary takes ownership.
            Some(env.make_binary(&mut bin))
        } else {
            None
        }
    }
}

impl<'a> TryFrom<Term<'a>> for TypedTerm<'a> {
    type Error = CodecError;
    /// Fails with [`CodecError::UnknownTermType`] if the term's type is one
    /// this otter build does not recognize (a type added by a newer OTP).
    fn try_from(raw: Term<'a>) -> Result<TypedTerm<'a>, CodecError> {
        raw.resolve().ok_or(CodecError::UnknownTermType)
    }
}

impl<'a> From<Atom> for TypedTerm<'a> {
    fn from(v: Atom) -> TypedTerm<'a> { TypedTerm::Atom(v) }
}
impl<'a> From<Binary<'a>> for TypedTerm<'a> {
    fn from(v: Binary<'a>) -> TypedTerm<'a> {
        TypedTerm::Bitstring(Bitstring { term: v.term, env: v.env })
    }
}
impl<'a> From<Bitstring<'a>> for TypedTerm<'a> {
    fn from(v: Bitstring<'a>) -> TypedTerm<'a> { TypedTerm::Bitstring(v) }
}
impl<'a> From<Float<'a>> for TypedTerm<'a> {
    fn from(v: Float<'a>) -> TypedTerm<'a> { TypedTerm::Float(v) }
}
impl<'a> From<Fun<'a>> for TypedTerm<'a> {
    fn from(v: Fun<'a>) -> TypedTerm<'a> { TypedTerm::Fun(v) }
}
impl<'a> From<Integer<'a>> for TypedTerm<'a> {
    fn from(v: Integer<'a>) -> TypedTerm<'a> { TypedTerm::Integer(v) }
}
impl<'a> From<List<'a>> for TypedTerm<'a> {
    fn from(v: List<'a>) -> TypedTerm<'a> { TypedTerm::List(v) }
}
impl<'a> From<Map<'a>> for TypedTerm<'a> {
    fn from(v: Map<'a>) -> TypedTerm<'a> { TypedTerm::Map(v) }
}
impl<'a> From<Pid> for TypedTerm<'a> {
    fn from(v: Pid) -> TypedTerm<'a> { TypedTerm::Pid(v) }
}
impl<'a> From<Port> for TypedTerm<'a> {
    fn from(v: Port) -> TypedTerm<'a> { TypedTerm::Port(v) }
}
impl<'a> From<Reference<'a>> for TypedTerm<'a> {
    fn from(v: Reference<'a>) -> TypedTerm<'a> { TypedTerm::Reference(v) }
}
impl<'a> From<Tuple<'a>> for TypedTerm<'a> {
    fn from(v: Tuple<'a>) -> TypedTerm<'a> { TypedTerm::Tuple(v) }
}

impl<'b> Encoder for TypedTerm<'b> {
    fn encode<'a>(&self, env: Env<'a>) -> Term<'a> {
        match self {
            TypedTerm::Atom(v)      => v.encode(env),
            TypedTerm::Bitstring(v) => v.encode(env),
            TypedTerm::Float(v)     => v.encode(env),
            TypedTerm::Fun(v)       => v.encode(env),
            TypedTerm::Integer(v)   => v.encode(env),
            TypedTerm::List(v)      => v.encode(env),
            TypedTerm::Map(v)       => v.encode(env),
            TypedTerm::Pid(v)       => v.encode(env),
            TypedTerm::Port(v)      => v.encode(env),
            TypedTerm::Reference(v) => v.encode(env),
            TypedTerm::Tuple(v)     => v.encode(env),
        }
    }
}

impl<'a> Decoder<'a> for Term<'a> {
    fn decode(term: Term<'a>) -> Result<Self, CodecError> {
        Ok(term)
    }
}

impl<'a> Decoder<'a> for TypedTerm<'a> {
    fn decode(term: Term<'a>) -> Result<Self, CodecError> {
        TypedTerm::try_from(term)
    }
}

impl<'b> Encoder for Term<'b> {
    fn encode<'a>(&self, env: Env<'a>) -> Term<'a> {
        if self.env.as_ptr() == env.as_ptr() {
            Term::new(env, self.term)
        } else {
            env.make_copy(*self)
        }
    }
}

// ---------------------------------------------------------------------------
// AsNifTerm — sealed, lifetime-bound trait for "anything that wraps a NIF term"
// ---------------------------------------------------------------------------

mod sealed {
    pub trait Sealed {}
}

/// A type whose underlying NIF term is valid in environment `'a`.
///
/// Used as the bound on polymorphic term arguments. The lifetime parameter
/// ties the term to a specific env: an `impl AsNifTerm<'a>` argument will
/// only accept terms whose env matches the call site's `'a`, so cross-env
/// terms are rejected at compile time. BEAM treats cross-env terms as
/// undefined behavior, so this check is load-bearing for soundness.
///
/// Env-portable types (`Atom`, `Pid`, `Port`) implement `AsNifTerm<'a>` for
/// every `'a` — BEAM treats them as stable across envs. Env-bound types
/// (`Term<'a>`, `TypedTerm<'a>`, `Binary<'a>`, etc.) implement it only for
/// their own lifetime.
///
/// This trait is sealed — it cannot be implemented outside the crate.
pub trait AsNifTerm<'a>: sealed::Sealed {
    /// Extract the underlying NIF term word.
    #[doc(hidden)]
    fn as_nif_term(&self) -> NifTerm;
}

impl sealed::Sealed for Atom {}
impl sealed::Sealed for Binary<'_> {}
impl sealed::Sealed for Bitstring<'_> {}
impl sealed::Sealed for Float<'_> {}
impl sealed::Sealed for Fun<'_> {}
impl sealed::Sealed for Integer<'_> {}
impl sealed::Sealed for List<'_> {}
impl sealed::Sealed for Map<'_> {}
impl sealed::Sealed for Pid {}
impl sealed::Sealed for Port {}
impl sealed::Sealed for Reference<'_> {}
impl sealed::Sealed for Tuple<'_> {}
impl sealed::Sealed for Term<'_> {}
impl sealed::Sealed for TypedTerm<'_> {}
impl<T: sealed::Sealed + ?Sized> sealed::Sealed for &T {}

// Env-portable: any 'a works.
impl<'a> AsNifTerm<'a> for Atom {
    fn as_nif_term(&self) -> NifTerm { self.term }
}
impl<'a> AsNifTerm<'a> for Pid {
    fn as_nif_term(&self) -> NifTerm { self.term }
}
impl<'a> AsNifTerm<'a> for Port {
    fn as_nif_term(&self) -> NifTerm { self.term }
}

// Env-bound: tied to the type's lifetime.
impl<'a> AsNifTerm<'a> for Binary<'a> {
    fn as_nif_term(&self) -> NifTerm { self.term }
}
impl<'a> AsNifTerm<'a> for Bitstring<'a> {
    fn as_nif_term(&self) -> NifTerm { self.term }
}
impl<'a> AsNifTerm<'a> for Float<'a> {
    fn as_nif_term(&self) -> NifTerm { self.term }
}
impl<'a> AsNifTerm<'a> for Fun<'a> {
    fn as_nif_term(&self) -> NifTerm { self.term }
}
impl<'a> AsNifTerm<'a> for Integer<'a> {
    fn as_nif_term(&self) -> NifTerm { self.term }
}
impl<'a> AsNifTerm<'a> for List<'a> {
    fn as_nif_term(&self) -> NifTerm { self.term }
}
impl<'a> AsNifTerm<'a> for Map<'a> {
    fn as_nif_term(&self) -> NifTerm { self.term }
}
impl<'a> AsNifTerm<'a> for Reference<'a> {
    fn as_nif_term(&self) -> NifTerm { self.term }
}
impl<'a> AsNifTerm<'a> for Tuple<'a> {
    fn as_nif_term(&self) -> NifTerm { self.term }
}
impl<'a> AsNifTerm<'a> for Term<'a> {
    fn as_nif_term(&self) -> NifTerm { self.term }
}
impl<'a> AsNifTerm<'a> for TypedTerm<'a> {
    fn as_nif_term(&self) -> NifTerm { self.as_raw() }
}

impl<'a, T: AsNifTerm<'a> + ?Sized> AsNifTerm<'a> for &T {
    fn as_nif_term(&self) -> NifTerm { (**self).as_nif_term() }
}

// ---------------------------------------------------------------------------
// Raised — proof that an exception is pending on the environment
// ---------------------------------------------------------------------------

/// Proof that an exception has been raised on the environment.
///
/// A `Raised` can only be produced by an operation that actually raises — see
/// [`Env::raise_exception`], [`Env::make_badarg`], [`Env::check_raised`], or a
/// fallible builder such as [`Env::make_double`]. Because it can only exist
/// *after* a raise, holding one means the environment is in a pending-exception
/// state in which no further environment operation is valid.
///
/// The intended use is to propagate it straight out of the NIF with `?`: the
/// generated wrapper returns it directly and the BEAM raises the pending
/// exception. Do **not** perform further work on the env while holding one.
pub struct Raised<'a> {
    marker: Term<'a>,
}

impl<'a> Raised<'a> {
    #[inline]
    pub(crate) fn new(marker: Term<'a>) -> Raised<'a> {
        Raised { marker }
    }

    /// The machine word to return from the NIF. With the exception already
    /// pending, returning it triggers the raise.
    #[inline]
    pub(crate) fn raw(&self) -> NifTerm {
        self.marker.as_raw()
    }
}

// ---------------------------------------------------------------------------
// Env: type/copy/raise operations
// ---------------------------------------------------------------------------

impl<'a> Env<'a> {
    /// The dynamic type of `term` (`enif_term_type`).
    ///
    /// `None` if the BEAM returns a term-type code this otter build does not
    /// recognize (a type added by a newer OTP). For the raw code, enable the
    /// `raw` feature and use `Term::term_type_raw`.
    pub fn term_type(self, term: impl AsNifTerm<'a>) -> Option<NifTermType> {
        let code = unsafe { crate::enif::term_type(self.as_ptr(), term.as_nif_term()) };
        NifTermType::from_raw(code)
    }

    /// Copy `src` (which may belong to another environment) into this one,
    /// returning a term owned by this env (`enif_make_copy`).
    pub fn make_copy<'b>(self, src: impl AsNifTerm<'b>) -> Term<'a> {
        let raw = unsafe { crate::enif::make_copy(self.as_ptr(), src.as_nif_term()) };
        Term::new(self, raw)
    }

    /// Tell the scheduler how much of the current timeslice this NIF has consumed.
    ///
    /// `percent` should be between 1 and 100. Returns `true` if the timeslice
    /// has been exhausted — the NIF should return as soon as possible to allow
    /// the scheduler to run other processes.
    ///
    /// Wraps `enif_consume_timeslice`.
    pub fn consume_timeslice(self, percent: i32) -> bool {
        unsafe { crate::enif::consume_timeslice(self.as_ptr(), percent) != 0 }
    }

    /// Create a unique integer.
    ///
    /// `properties` is a bitmask of `NifUniqueInteger::POSITIVE` and
    /// `NifUniqueInteger::MONOTONIC`. Use `NifUniqueInteger(0)` for an arbitrary unique integer.
    ///
    /// Wraps `enif_make_unique_integer`.
    pub fn make_unique_integer(self, properties: NifUniqueInteger) -> Integer<'a> {
        let raw = unsafe {
            crate::enif::make_unique_integer(self.as_ptr(), properties)
        };
        debug_assert!(
            matches!(Term::new(self, raw).resolve(), Some(TypedTerm::Integer(_))),
            "enif_make_unique_integer produced a non-integer term",
        );
        Integer { term: raw, env: self }
    }

    /// Hash a term using the specified algorithm.
    ///
    /// `algorithm` is `NifHash::Phash2` (portable, consistent across nodes)
    /// or `NifHash::InternalHash` (node-local, faster).
    ///
    /// Wraps `enif_hash`.
    pub fn hash(self, algorithm: NifHash, term: impl AsNifTerm<'a>, salt: u64) -> u64 {
        unsafe { crate::enif::hash(algorithm, term.as_nif_term(), salt) }
    }

    /// Check if the calling process is still alive.
    ///
    /// Returns `true` if the process that invoked this NIF is still alive.
    /// Wraps `enif_is_current_process_alive`.
    pub fn is_current_process_alive(self) -> bool {
        unsafe { crate::enif::is_current_process_alive(self.as_ptr()) != 0 }
    }

    /// The current logical CPU's execution time since some arbitrary point in
    /// the past, in `erlang:timestamp/0` format (`enif_cpu_time`).
    ///
    /// Returns `Err(Raised)` (`badarg`) if the OS does not support fetching
    /// CPU time.
    pub fn cpu_time(self) -> Result<Term<'a>, Raised<'a>> {
        let raw = unsafe { crate::enif::cpu_time(self.as_ptr()) };
        self.check_raised(raw)
    }

    /// Raise an exception with the given reason term.
    ///
    /// Always returns `Err(Raised)`, generic over the success type so it fits
    /// any position — the idiom is `return env.raise_exception(reason)` (the
    /// success type is inferred from the NIF's return type). Wraps
    /// `enif_raise_exception`.
    pub fn raise_exception<T>(self, reason: impl AsNifTerm<'a>) -> Result<T, Raised<'a>> {
        let marker = unsafe { crate::enif::raise_exception(self.as_ptr(), reason.as_nif_term()) };
        Err(Raised::new(Term::new(self, marker)))
    }

    /// Raise a `badarg` error.
    ///
    /// Always returns `Err(Raised)`, generic over the success type so it fits
    /// any position: `return env.make_badarg()`, a `let`-`else` arm, or
    /// `decode(t).or_else(|_| env.make_badarg())?`. Wraps `enif_make_badarg`.
    pub fn make_badarg<T>(self) -> Result<T, Raised<'a>> {
        let marker = unsafe { crate::enif::make_badarg(self.as_ptr()) };
        Err(Raised::new(Term::new(self, marker)))
    }

    /// Convert the raw result of a possibly-raising operation into a `Result`.
    ///
    /// If the environment has a pending exception, returns `Err(Raised)` — the
    /// exception is already set, so the only valid next step is to propagate it
    /// (with `?`). Otherwise returns `Ok` wrapping `term`.
    ///
    /// This is how to safely call a `raw`-surface enif function that may raise:
    /// pass its returned term straight through `check_raised`.
    pub fn check_raised(self, term: NifTerm) -> Result<Term<'a>, Raised<'a>> {
        if unsafe { crate::enif::has_pending_exception(self.as_ptr(), std::ptr::null_mut()) } != 0 {
            Err(Raised::new(Term::new(self, term)))
        } else {
            Ok(Term::new(self, term))
        }
    }

    /// Reschedule the current NIF to run `fp` with the given arguments.
    ///
    /// `fun_name` is the name reported to Erlang tracing. `flags` is one of
    /// `sys::NIF_DIRTY_JOB_NORMAL`, `sys::NIF_DIRTY_JOB_CPU_BOUND`, or
    /// `sys::NIF_DIRTY_JOB_IO_BOUND`.
    ///
    /// The success value must be returned directly from the NIF. If
    /// `fun_name` cannot be converted to an atom the BEAM raises `badarg`,
    /// surfaced here as `Err(Raised)`.
    ///
    /// # Safety
    ///
    /// `fp` must be a valid NIF function pointer. `argv` must point to
    /// `argc` valid terms.
    ///
    /// Wraps `enif_schedule_nif`.
    pub unsafe fn schedule_nif(
        self,
        fun_name: &std::ffi::CStr,
        flags: i32,
        fp: unsafe extern "C" fn(
            *mut crate::sys::NifEnv,
            std::ffi::c_int,
            *const crate::sys::NifTerm,
        ) -> crate::sys::NifTerm,
        argc: i32,
        argv: *const crate::sys::NifTerm,
    ) -> Result<Term<'a>, Raised<'a>> {
        let raw = unsafe {
            crate::enif::schedule_nif(
                self.as_ptr(),
                fun_name.as_ptr(),
                flags,
                fp,
                argc,
                argv,
            )
        };
        self.check_raised(raw)
    }

    /// Enable delayed halt: the VM waits for currently-running NIF calls to
    /// return before halting. Must be called from the load callback. Returns
    /// `true` on success.
    ///
    /// `ERL_NIF_OPT_DELAY_HALT` is a boolean enable that takes no argument —
    /// there is no halt *duration* to configure.
    ///
    /// Wraps `enif_set_option(ERL_NIF_OPT_DELAY_HALT)`.
    pub fn set_option_delay_halt(self) -> bool {
        assert!(
            self.kind == EnvKind::Init,
            "set_option_delay_halt must be called from the NIF load callback"
        );
        unsafe { crate::enif::set_option_delay_halt(self.as_ptr()) == 0 }
    }

    /// Set the on-halt callback. Must be called from the load callback.
    ///
    /// # Safety
    ///
    /// `callback` must remain valid for the lifetime of the VM.
    ///
    /// Wraps `enif_set_option(ERL_NIF_OPT_ON_HALT, ...)`.
    pub unsafe fn set_option_on_halt(
        self,
        callback: unsafe extern "C" fn(*mut std::ffi::c_void),
    ) -> bool {
        assert!(
            self.kind == EnvKind::Init,
            "set_option_on_halt must be called from the NIF load callback"
        );
        unsafe { crate::enif::set_option_on_halt(self.as_ptr(), callback) == 0 }
    }

    /// Set the on-unload-thread callback. Must be called from the load callback.
    ///
    /// # Safety
    ///
    /// `callback` must remain valid for the lifetime of the VM.
    ///
    /// Wraps `enif_set_option(ERL_NIF_OPT_ON_UNLOAD_THREAD, ...)`.
    pub unsafe fn set_option_on_unload_thread(
        self,
        callback: unsafe extern "C" fn(*mut std::ffi::c_void),
    ) -> bool {
        assert!(
            self.kind == EnvKind::Init,
            "set_option_on_unload_thread must be called from the NIF load callback"
        );
        unsafe { crate::enif::set_option_on_unload_thread(self.as_ptr(), callback) == 0 }
    }
}
