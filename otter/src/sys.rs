//! Raw C ABI types mirroring `erl_nif.h`.
//!
//! Direct Rust transcriptions of the types defined in `erl_nif.h`. No logic,
//! no safety wrappers — only type definitions and constants. All struct types
//! are `#[repr(C)]` to match the C ABI exactly.
//!
//! Naming convention: `Erl` prefix dropped, `Nif` prefix retained.
//! `ERL_NIF_TERM` → `NifTerm`, `ErlNifEnv` → `NifEnv`, etc.

use std::ffi::{c_char, c_int, c_uint, c_void};

// ---------------------------------------------------------------------------
// Version constants
// ---------------------------------------------------------------------------

/// NIF 0.1 (OTP R13B03).
pub const NIF_MAJOR_VERSION: c_int = 2;
/// NIF 0.1 (OTP R13B03).
#[cfg(not(feature = "nif_2_18"))]
pub const NIF_MINOR_VERSION: c_int = 17;
/// NIF 0.1 (OTP R13B03).
#[cfg(feature = "nif_2_18")]
pub const NIF_MINOR_VERSION: c_int = 18;
/// NIF 2.1 (OTP R14B02).
pub const NIF_VM_VARIANT: &std::ffi::CStr = c"beam.vanilla";
/// NIF 2.14 (OTP 21.0).
pub const NIF_MIN_ERTS_VERSION: &std::ffi::CStr = c"erts-14.0";

// ---------------------------------------------------------------------------
// Core term type
// ---------------------------------------------------------------------------

/// `ERL_NIF_TERM` — a tagged machine word. Opaque to the NIF library.
/// NIF 1.0 (OTP R13B04).
pub type NifTerm = usize;

// ---------------------------------------------------------------------------
// Opaque environment
// ---------------------------------------------------------------------------

/// `ErlNifEnv` — per-call or process-independent NIF environment.
///
/// Always used as `*mut NifEnv`. Never constructed directly.
/// NIF 1.0 (OTP R13B04).
#[repr(C)]
pub struct NifEnv {
    _opaque: [u8; 0],
    _marker: std::marker::PhantomData<(*mut u8, std::marker::PhantomPinned)>,
}

// ---------------------------------------------------------------------------
// Function descriptor
// ---------------------------------------------------------------------------

/// `ErlNifFunc` — describes one NIF: Erlang name, arity, function pointer, flags.
/// NIF 1.0 (OTP R13B04). `flags` field added in NIF 2.7 (OTP 17.3).
#[repr(C)]
pub struct NifFunc {
    pub name:  *const c_char,
    pub arity: c_uint,
    pub fptr:  unsafe extern "C" fn(env: *mut NifEnv, argc: c_int, argv: *const NifTerm) -> NifTerm,
    pub flags: c_uint,
}

/// `NifFunc.flags` value: run on dirty CPU scheduler. NIF 2.7 (OTP 17.3).
pub const NIF_FUNC_DIRTY_CPU: c_uint = 1;
/// `NifFunc.flags` value: run on dirty I/O scheduler. NIF 2.7 (OTP 17.3).
pub const NIF_FUNC_DIRTY_IO: c_uint = 2;

// ---------------------------------------------------------------------------
// Library entry point descriptor
// ---------------------------------------------------------------------------

/// `ErlNifEntry` — the library descriptor returned by `nif_init()`.
/// NIF 1.0 (OTP R13B04), extended in later versions.
#[repr(C)]
pub struct NifEntry {
    pub major:        c_int,
    pub minor:        c_int,
    pub name:         *const c_char,
    pub num_of_funcs: c_int,
    pub funcs:        *mut NifFunc,
    pub load:    Option<unsafe extern "C" fn(*mut NifEnv, *mut *mut c_void, NifTerm) -> c_int>,
    pub reload:  Option<unsafe extern "C" fn(*mut NifEnv, *mut *mut c_void, NifTerm) -> c_int>,
    pub upgrade: Option<unsafe extern "C" fn(*mut NifEnv, *mut *mut c_void, *mut *mut c_void, NifTerm) -> c_int>,
    pub unload:  Option<unsafe extern "C" fn(*mut NifEnv, *mut c_void)>,
    /// Added in NIF 2.1 (OTP R14B02).
    pub vm_variant: *const c_char,
    /// Added in NIF 2.7 (OTP 17.3) — unused, set to 0 or 1.
    pub options: c_uint,
    /// Added in NIF 2.12 (OTP 20.0) — must equal `size_of::<NifResourceTypeInit>()`.
    pub sizeof_resource_type_init: usize,
    /// Added in NIF 2.14 (OTP 21.0) — minimum ERTS version string.
    pub min_erts: *const c_char,
}

// ---------------------------------------------------------------------------
// Binary
// ---------------------------------------------------------------------------

/// `ErlNifBinary` — inspected binary: byte count and data pointer.
///
/// Returned by `enif_inspect_binary` and `enif_alloc_binary`.
/// The `ref_bin` and `__spare__` fields are internal to the BEAM.
/// NIF 1.0 (OTP R13B04).
#[repr(C)]
pub struct NifBinary {
    pub size: usize,
    pub data: *mut u8,
    ref_bin: *mut c_void,
    _spare:  [*mut c_void; 2],
}

// ---------------------------------------------------------------------------
// Pid and Port
// ---------------------------------------------------------------------------

/// `ErlNifPid` — local process identifier. NIF 2.0 (OTP R14A).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct NifPid {
    pub pid: NifTerm,
}

/// `ErlNifPort` — port identifier. NIF 2.11 (OTP 19.0).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct NifPort {
    pub port_id: NifTerm,
}

// ---------------------------------------------------------------------------
// Monitor
// ---------------------------------------------------------------------------

/// `ErlNifMonitor` (= `ErlDrvMonitor`) — process monitor handle.
///
/// 32 bytes, opaque. Never inspect the contents directly; pass only by pointer.
/// NIF 2.12 (OTP 20.0).
#[repr(C, align(8))]
#[derive(Clone, Copy)]
pub struct NifMonitor(pub [u8; 32]);

// ---------------------------------------------------------------------------
// Resource type
// ---------------------------------------------------------------------------

/// `ErlNifResourceType` — opaque resource type handle returned by registration.
/// NIF 2.0 (OTP R14A).
#[repr(C)]
pub struct NifResourceType {
    _opaque: [u8; 0],
    _marker: std::marker::PhantomData<(*mut u8, std::marker::PhantomPinned)>,
}

/// `ErlNifResourceTypeInit` — callback table passed to resource type registration.
///
/// `members` must equal the number of callback fields being provided,
/// counting from the start: 1 = dtor only, 2 = dtor+stop, 3 = dtor+stop+down,
/// 4 = dtor+stop+down+dyncall.
/// NIF 2.12 (OTP 20.0). `dyncall` field added in NIF 2.16 (OTP 24.0).
#[repr(C)]
pub struct NifResourceTypeInit {
    pub dtor:    Option<unsafe extern "C" fn(*mut NifEnv, *mut c_void)>,
    pub stop:    Option<unsafe extern "C" fn(*mut NifEnv, *mut c_void, NifEvent, c_int)>,
    pub down:    Option<unsafe extern "C" fn(*mut NifEnv, *mut c_void, *mut NifPid, *mut NifMonitor)>,
    pub members: c_int,
    pub dyncall: Option<unsafe extern "C" fn(*mut NifEnv, *mut c_void, *mut c_void)>,
}

/// `ErlNifResourceFlags` — passed to resource type registration functions.
///
/// Combinable with bitwise OR: `NifResourceFlags::CREATE | NifResourceFlags::TAKEOVER`.
/// NIF 2.0 (OTP R14A).
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct NifResourceFlags(pub c_int);

impl NifResourceFlags {
    /// Create a new resource type.
    pub const CREATE: Self = Self(1);
    /// Take over from an old NIF library during upgrade.
    pub const TAKEOVER: Self = Self(2);
}

impl std::ops::BitOr for NifResourceFlags {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self { Self(self.0 | rhs.0) }
}

// ---------------------------------------------------------------------------
// OS event handle (for enif_select)
// ---------------------------------------------------------------------------

/// `ErlNifEvent` — OS event handle. `c_int` on Unix, `*mut c_void` on Windows.
/// NIF 2.12 (OTP 20.0).
#[cfg(unix)]
pub type NifEvent = c_int;

#[cfg(windows)]
pub type NifEvent = *mut c_void;

// ---------------------------------------------------------------------------
// Map iterator
// ---------------------------------------------------------------------------

/// `ErlNifMapIteratorEntry` — starting position when creating a map iterator.
/// NIF 2.6 (OTP R17).
#[repr(i32)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum NifMapIteratorEntry {
    First = 1,
    Last  = 2,
}

// Internal union variants for NifMapIterator — not public.
#[repr(C)]
#[derive(Clone, Copy)]
struct NifMapIteratorFlat {
    ks: *mut NifTerm,
    vs: *mut NifTerm,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct NifMapIteratorHash {
    wstack: *mut c_void,
    kv:     *mut NifTerm,
}

#[repr(C)]
union NifMapIteratorUnion {
    flat: NifMapIteratorFlat,
    hash: NifMapIteratorHash,
}

/// `ErlNifMapIterator` — map iteration cursor. All fields are internal to the BEAM.
///
/// Initialized by `enif_map_iterator_create`; destroyed by `enif_map_iterator_destroy`.
/// Must not be moved after initialization.
/// NIF 2.6 (OTP R17).
#[repr(C)]
pub struct NifMapIterator {
    pub map: NifTerm,
    size:    usize,
    idx:     usize,
    u:       NifMapIteratorUnion,
    _spare:  [*mut c_void; 2],
}

// ---------------------------------------------------------------------------
// TypedTerm type tag
// ---------------------------------------------------------------------------

/// `ErlNifTermType` — the 11 canonical term types returned by `enif_term_type`.
///
/// Note: the C header defines a sentinel value of -1 to force a default case
/// in switch statements. New term types may be added in future OTP versions;
/// callers must always handle an unknown variant.
/// NIF 2.15 (OTP 22.0).
#[repr(i32)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NifTermType {
    Atom      = 1,
    Bitstring = 2,
    Float     = 3,
    Fun       = 4,
    Integer   = 5,
    List      = 6,
    Map       = 7,
    Pid       = 8,
    Port      = 9,
    Reference = 10,
    Tuple     = 11,
}

impl NifTermType {
    /// Map a raw `enif_term_type` return code to a known variant.
    ///
    /// Returns `None` for any code outside the canonical 1..=11 — the C header
    /// reserves the right to add term types and defines a `-1` sentinel, so an
    /// unrecognized code must never be transmuted into this enum (that would be
    /// undefined behavior). Callers surface the `None` as an unknown term type.
    pub fn from_raw(code: c_int) -> Option<Self> {
        match code {
            1  => Some(Self::Atom),
            2  => Some(Self::Bitstring),
            3  => Some(Self::Float),
            4  => Some(Self::Fun),
            5  => Some(Self::Integer),
            6  => Some(Self::List),
            7  => Some(Self::Map),
            8  => Some(Self::Pid),
            9  => Some(Self::Port),
            10 => Some(Self::Reference),
            11 => Some(Self::Tuple),
            _  => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Character encoding
// ---------------------------------------------------------------------------

/// `ErlNifCharEncoding` — encoding used when reading/writing atom names.
/// NIF 1.0 (OTP R13B04). `Utf8` added in NIF 2.17 (OTP 26.0).
#[repr(i32)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum NifCharEncoding {
    Latin1 = 1,
    /// NIF 2.17 (OTP 26.0).
    Utf8   = 2,
}

// ---------------------------------------------------------------------------
// Time
// ---------------------------------------------------------------------------

/// `ErlNifTime` — time value in BEAM time units. NIF 2.10 (OTP 18.3).
pub type NifTime = i64;

/// `ERL_NIF_TIME_ERROR` — sentinel returned by time functions on error.
/// NIF 2.10 (OTP 18.3).
pub const NIF_TIME_ERROR: NifTime = i64::MIN;

/// `ErlNifTimeUnit` — time unit for `enif_monotonic_time` etc.
/// NIF 2.10 (OTP 18.3).
#[repr(i32)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NifTimeUnit {
    Second      = 0,
    Millisecond = 1,
    Microsecond = 2,
    Nanosecond  = 3,
}

// ---------------------------------------------------------------------------
// Unique integer flags
// ---------------------------------------------------------------------------

/// `ErlNifUniqueInteger` — flags for `enif_make_unique_integer`.
///
/// Combine with bitwise OR: `NifUniqueInteger::POSITIVE | NifUniqueInteger::MONOTONIC`.
/// NIF 2.11 (OTP 19.0).
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct NifUniqueInteger(pub c_int);

impl NifUniqueInteger {
    /// Return a positive integer only.
    pub const POSITIVE: Self = Self(1 << 0);
    /// Return a strictly monotonic integer.
    pub const MONOTONIC: Self = Self(1 << 1);
}

impl std::ops::BitOr for NifUniqueInteger {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self { Self(self.0 | rhs.0) }
}

// ---------------------------------------------------------------------------
// Hash
// ---------------------------------------------------------------------------

/// `ErlNifHash` — hash algorithm for `enif_hash`. NIF 2.12 (OTP 20.0).
#[repr(i32)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NifHash {
    InternalHash = 1,
    Phash2       = 2,
}

// ---------------------------------------------------------------------------
// Select (I/O event multiplexing)
// ---------------------------------------------------------------------------

/// `ErlNifSelectFlags` — flags for `enif_select`.
///
/// Combine with bitwise OR: `NifSelectFlags::READ | NifSelectFlags::CUSTOM_MSG`.
/// NIF 2.12 (OTP 20.0). `CANCEL` and `CUSTOM_MSG` added in NIF 2.15 (OTP 22.0).
/// `ERROR` added in NIF 2.16 (OTP 24.0).
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct NifSelectFlags(pub c_int);

impl NifSelectFlags {
    /// NIF 2.12 (OTP 20.0).
    pub const READ:       Self = Self(1 << 0);
    /// NIF 2.12 (OTP 20.0).
    pub const WRITE:      Self = Self(1 << 1);
    /// NIF 2.12 (OTP 20.0).
    pub const STOP:       Self = Self(1 << 2);
    /// NIF 2.15 (OTP 22.0).
    pub const CANCEL:     Self = Self(1 << 3);
    /// NIF 2.15 (OTP 22.0).
    pub const CUSTOM_MSG: Self = Self(1 << 4);
    /// NIF 2.16 (OTP 24.0).
    pub const ERROR:      Self = Self(1 << 5);
}

impl std::ops::BitOr for NifSelectFlags {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self { Self(self.0 | rhs.0) }
}

/// Return bits from `enif_select`. NIF 2.12 (OTP 20.0).
pub const NIF_SELECT_STOP_CALLED:     c_int = 1 << 0;
/// NIF 2.12 (OTP 20.0).
pub const NIF_SELECT_STOP_SCHEDULED:  c_int = 1 << 1;
/// NIF 2.12 (OTP 20.0).
pub const NIF_SELECT_INVALID_EVENT:   c_int = 1 << 2;
/// NIF 2.12 (OTP 20.0).
pub const NIF_SELECT_FAILED:          c_int = 1 << 3;
/// NIF 2.15 (OTP 22.0).
pub const NIF_SELECT_READ_CANCELLED:  c_int = 1 << 4;
/// NIF 2.15 (OTP 22.0).
pub const NIF_SELECT_WRITE_CANCELLED: c_int = 1 << 5;
/// NIF 2.16 (OTP 24.0).
pub const NIF_SELECT_ERROR_CANCELLED: c_int = 1 << 6;
/// NIF 2.16 (OTP 24.0).
pub const NIF_SELECT_NOTSUP:          c_int = 1 << 7;

// ---------------------------------------------------------------------------
// binary_to_term options
// ---------------------------------------------------------------------------

/// Safe decoding for `enif_binary_to_term`: reject encoded atoms that don't
/// already exist. NIF 2.11 (OTP 19.0).
pub const NIF_BIN2TERM_SAFE: c_uint = 0x20000000;

// ---------------------------------------------------------------------------
// System info
// ---------------------------------------------------------------------------

/// `ErlNifSysInfo` (= `ErlDrvSysInfo`) — BEAM system information.
/// NIF 1.0 (OTP R13B04).
#[repr(C)]
pub struct NifSysInfo {
    pub driver_major_version: c_int,
    pub driver_minor_version: c_int,
    pub erts_version:         *mut c_char,
    pub otp_release:          *mut c_char,
    pub thread_support:       c_int,
    pub smp_support:          c_int,
    pub async_threads:        c_int,
    pub scheduler_threads:    c_int,
    pub nif_major_version:    c_int,
    pub nif_minor_version:    c_int,
    pub dirty_scheduler_support: c_int,
}

// ---------------------------------------------------------------------------
// NIF options
// ---------------------------------------------------------------------------

/// `ErlNifOption` — option key for `enif_set_option`. NIF 2.17 (OTP 26.0).
/// `OnUnloadThread` added in NIF 2.17 (OTP 27.0).
#[repr(i32)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NifOption {
    DelayHalt      = 1,
    OnHalt         = 2,
    /// NIF 2.17 (OTP 27.0).
    OnUnloadThread = 3,
}

// ---------------------------------------------------------------------------
// Thread type (return values from enif_thread_type)
// ---------------------------------------------------------------------------

/// Not a scheduler thread. NIF 2.11 (OTP 19.0).
pub const NIF_THR_UNDEFINED:          c_int = 0;
/// Normal BEAM scheduler thread. NIF 2.11 (OTP 19.0).
pub const NIF_THR_NORMAL_SCHEDULER:   c_int = 1;
/// Dirty CPU scheduler thread. NIF 2.11 (OTP 19.0).
pub const NIF_THR_DIRTY_CPU_SCHEDULER: c_int = 2;
/// Dirty I/O scheduler thread. NIF 2.11 (OTP 19.0).
pub const NIF_THR_DIRTY_IO_SCHEDULER: c_int = 3;

// ---------------------------------------------------------------------------
// Schedule NIF flags
// ---------------------------------------------------------------------------

/// Flags for `enif_schedule_nif`: run on a normal scheduler. NIF 2.7 (OTP 17.3).
pub const NIF_DIRTY_JOB_NORMAL:    c_int = 0;
/// Flags for `enif_schedule_nif`: run on a dirty CPU scheduler. NIF 2.7 (OTP 17.3).
pub const NIF_DIRTY_JOB_CPU_BOUND: c_int = 1;
/// Flags for `enif_schedule_nif`: run on a dirty I/O scheduler. NIF 2.7 (OTP 17.3).
pub const NIF_DIRTY_JOB_IO_BOUND:  c_int = 2;

// ---------------------------------------------------------------------------
// I/O queue and iovec
// ---------------------------------------------------------------------------

/// `ErlNifIOQueue` — opaque I/O queue handle. NIF 2.13 (OTP 20.1).
#[repr(C)]
pub struct NifIOQueue {
    _opaque: [u8; 0],
    _marker: std::marker::PhantomData<(*mut u8, std::marker::PhantomPinned)>,
}

/// `ErlNifIOQueueOpts` — I/O queue creation options. NIF 2.13 (OTP 20.1).
pub type NifIOQueueOpts = c_int;

/// Normal I/O queue mode. NIF 2.13 (OTP 20.1).
pub const NIF_IOQ_NORMAL: NifIOQueueOpts = 1;

/// `SysIOVec` — iovec on Unix. Matches `struct iovec`. NIF 2.13 (OTP 20.1).
#[cfg(unix)]
#[repr(C)]
pub struct SysIOVec {
    pub iov_base: *mut c_void,
    pub iov_len: usize,
}

/// `ErlNifIOVec` — scatter/gather I/O vector. NIF 2.13 (OTP 20.1).
#[repr(C)]
pub struct NifIOVec {
    pub iovcnt: c_int,
    pub size: usize,
    pub iov: *mut SysIOVec,
    ref_bins: *mut *mut c_void,
    flags: c_int,
    small_iov: [SysIOVec; 16],
    small_ref_bin: [*mut c_void; 16],
}
