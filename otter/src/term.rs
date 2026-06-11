//! `Term<'a>` and `TypedTerm<'a>`.

use crate::codec::{CodecError, Decoder, Encoder};
use crate::env::Env;
use crate::sys::{NifHash, NifTerm, NifTermType, NifUniqueInteger};
use crate::types::{
    Atom, Binary, Bitstring, Float, Fun, Integer, List, Map, Pid, Port, Reference, Tuple,
};
use crate::wrapper;

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
/// all term construction goes through concrete types (`Atom::new`, `Map::new`,
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
    ///
    /// For the `Bitstring` type tag, a second call to `enif_is_binary` is
    /// needed to determine whether the value is a byte-aligned `Binary` or a
    /// sub-byte `Bitstring`.
    pub fn resolve(self) -> TypedTerm<'a> {
        let env_ptr = self.env.as_ptr();
        match unsafe { wrapper::term::term_type(env_ptr, self.term) } {
            NifTermType::Atom => {
                TypedTerm::Atom(Atom::from_raw(self.term))
            }
            NifTermType::Bitstring => {
                if unsafe { wrapper::check::is_binary(env_ptr, self.term) } {
                    TypedTerm::Binary(Binary { term: self.term, env: self.env })
                } else {
                    TypedTerm::Bitstring(Bitstring { term: self.term, env: self.env })
                }
            }
            NifTermType::Float => {
                TypedTerm::Float(Float { term: self.term, env: self.env })
            }
            NifTermType::Fun => {
                TypedTerm::Fun(Fun { term: self.term, env: self.env })
            }
            NifTermType::Integer => {
                TypedTerm::Integer(Integer { term: self.term, env: self.env })
            }
            NifTermType::List => {
                TypedTerm::List(List { term: self.term, env: self.env })
            }
            NifTermType::Map => {
                TypedTerm::Map(Map { term: self.term, env: self.env })
            }
            NifTermType::Pid => {
                TypedTerm::Pid(Pid { term: self.term })
            }
            NifTermType::Port => {
                TypedTerm::Port(Port { term: self.term })
            }
            NifTermType::Reference => {
                TypedTerm::Reference(Reference { term: self.term, env: self.env })
            }
            NifTermType::Tuple => {
                TypedTerm::Tuple(Tuple { term: self.term, env: self.env })
            }
        }
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
/// The `Bitstring` type tag in `ErlNifTermType` maps to two variants here
/// (`Binary` and `Bitstring`) because `enif_is_binary` must be called to
/// distinguish them. Resolving a `Bitstring`-tagged term costs two NIF calls.
#[derive(Clone, Copy)]
pub enum TypedTerm<'a> {
    Atom(Atom),
    Binary(Binary<'a>),
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
            TypedTerm::Binary(v)    => v.term,
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
        unsafe { crate::wrapper::term::is_identical(self.term, other.term) }
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
        let c = unsafe { crate::wrapper::term::compare(self.term, other.term) };
        c.cmp(&0)
    }
}

impl<'a> PartialEq for TypedTerm<'a> {
    fn eq(&self, other: &Self) -> bool {
        unsafe { crate::wrapper::term::is_identical(self.as_raw(), other.as_raw()) }
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
        let c = unsafe { crate::wrapper::term::compare(self.as_raw(), other.as_raw()) };
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
        if unsafe { crate::wrapper::binary::term_to_binary(env.as_ptr(), self.as_raw(), &mut bin) }
        {
            // term_to_binary allocates via alloc_binary; we need to make it into a term.
            let term = unsafe { crate::wrapper::binary::make_binary(env.as_ptr(), &mut bin) };
            Some(Binary { term, env })
        } else {
            None
        }
    }
}

impl<'a> From<Term<'a>> for TypedTerm<'a> {
    fn from(raw: Term<'a>) -> TypedTerm<'a> {
        raw.resolve()
    }
}

impl<'a> From<Atom> for TypedTerm<'a> {
    fn from(v: Atom) -> TypedTerm<'a> { TypedTerm::Atom(v) }
}
impl<'a> From<Binary<'a>> for TypedTerm<'a> {
    fn from(v: Binary<'a>) -> TypedTerm<'a> { TypedTerm::Binary(v) }
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
        let term = unsafe { crate::wrapper::term::make_copy(env.as_ptr(), self.as_raw()) };
        Term::new(env, term)
    }
}

impl<'a> Decoder<'a> for TypedTerm<'a> {
    fn decode(term: TypedTerm<'a>) -> Result<Self, CodecError> {
        Ok(term)
    }
}

impl<'b> Encoder for Term<'b> {
    fn encode<'a>(&self, env: Env<'a>) -> Term<'a> {
        let term = unsafe { crate::wrapper::term::make_copy(env.as_ptr(), self.term) };
        Term::new(env, term)
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
// Env::raise / Env::raise_badarg
// ---------------------------------------------------------------------------

impl<'a> Env<'a> {
    /// Tell the scheduler how much of the current timeslice this NIF has consumed.
    ///
    /// `percent` should be between 1 and 100. Returns `true` if the timeslice
    /// has been exhausted — the NIF should return as soon as possible to allow
    /// the scheduler to run other processes.
    ///
    /// Wraps `enif_consume_timeslice`.
    pub fn consume_timeslice(self, percent: i32) -> bool {
        unsafe { crate::wrapper::term::consume_timeslice(self.as_ptr(), percent) != 0 }
    }

    /// Create a unique integer.
    ///
    /// `properties` is a bitmask of `NifUniqueInteger::POSITIVE` and
    /// `NifUniqueInteger::MONOTONIC`. Use `NifUniqueInteger(0)` for an arbitrary unique integer.
    ///
    /// Wraps `enif_make_unique_integer`.
    pub fn make_unique_integer(self, properties: NifUniqueInteger) -> TypedTerm<'a> {
        let raw = unsafe {
            wrapper::term::make_unique_integer(self.as_ptr(), properties)
        };
        Term::new(self, raw).resolve()
    }

    /// Hash a term using the specified algorithm.
    ///
    /// `algorithm` is `NifHash::Phash2` (portable, consistent across nodes)
    /// or `NifHash::InternalHash` (node-local, faster).
    ///
    /// Wraps `enif_hash`.
    pub fn hash(self, algorithm: NifHash, term: impl AsNifTerm<'a>, salt: u64) -> u64 {
        wrapper::term::hash(algorithm, term.as_nif_term(), salt)
    }

    /// Check if the calling process is still alive.
    ///
    /// Returns `true` if the process that invoked this NIF is still alive.
    /// Wraps `enif_is_current_process_alive`.
    pub fn is_current_process_alive(self) -> bool {
        unsafe { wrapper::pid::is_current_process_alive(self.as_ptr()) }
    }

    /// Raise an exception with the given reason term.
    ///
    /// The returned value must be returned from the NIF function via
    /// `.as_raw()`. It is **not** a valid term — do not inspect or resolve it.
    /// Wraps `enif_raise_exception`.
    pub fn raise(self, reason: impl AsNifTerm<'a>) -> Term<'a> {
        let raw =
            unsafe { wrapper::exception::raise_exception(self.as_ptr(), reason.as_nif_term()) };
        Term::new(self, raw)
    }

    /// Raise a `badarg` error.
    ///
    /// The returned value must be returned from the NIF function via
    /// `.as_raw()`. It is **not** a valid term — do not inspect or resolve it.
    /// Wraps `enif_make_badarg`.
    pub fn raise_badarg(self) -> Term<'a> {
        let raw = unsafe { wrapper::exception::make_badarg(self.as_ptr()) };
        Term::new(self, raw)
    }

    /// Reschedule the current NIF to run `fp` with the given arguments.
    ///
    /// `fun_name` is the name reported to Erlang tracing. `flags` is one of
    /// `sys::NIF_DIRTY_JOB_NORMAL`, `sys::NIF_DIRTY_JOB_CPU_BOUND`, or
    /// `sys::NIF_DIRTY_JOB_IO_BOUND`.
    ///
    /// The return value of this function must be returned directly from
    /// the NIF.
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
    ) -> TypedTerm<'a> {
        let raw = unsafe {
            wrapper::schedule::schedule_nif(
                self.as_ptr(),
                fun_name.as_ptr(),
                flags,
                fp,
                argc,
                argv,
            )
        };
        Term::new(self, raw).resolve()
    }

    /// Set the halt delay in milliseconds. Must be called from the load callback.
    ///
    /// Wraps `enif_set_option(ERL_NIF_OPT_DELAY_HALT, ...)`.
    pub fn set_option_delay_halt(self, delay_ms: u64) -> bool {
        unsafe { wrapper::system::set_option_delay_halt(self.as_ptr(), delay_ms) }
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
        unsafe { wrapper::system::set_option_on_halt(self.as_ptr(), callback) }
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
        unsafe { wrapper::system::set_option_on_unload_thread(self.as_ptr(), callback) }
    }
}
