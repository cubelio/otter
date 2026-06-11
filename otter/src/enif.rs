//! Complete `enif_*` API surface.
//!
//! Minimum: NIF 2.17 (OTP 26). All functions up to and including NIF 2.17
//! are always compiled. The only optional feature is:
//!
//! - `nif_2_18` — OTP 29: `term_size`, `get_atom_cache_index`, `max_atom_cache_index`
//!
//! The `enif_` prefix is dropped from all function names. C macros that
//! delegate to a real enif function (e.g. `enif_make_tuple3`,
//! `enif_select_read`) are exposed as plain Rust functions that call the
//! underlying function.
//!
//! Each wrapper function's doc comment notes the NIF version and OTP release
//! in which the C function was introduced.

#![allow(dead_code)]
use std::ffi::{c_char, c_int, c_uint, c_void};
use std::sync::OnceLock;

use crate::sys::{
    NifBinary, NifCharEncoding, NifEnv, NifEvent, NifHash, NifIOQueue, NifIOQueueOpts, NifIOVec,
    NifMapIterator, NifMapIteratorEntry, NifMonitor, NifOption, NifPid, NifPort,
    NifResourceFlags, NifResourceType, NifResourceTypeInit, NifSelectFlags, NifSysInfo, NifTerm,
    NifTermType, NifTime, NifTimeUnit, NifUniqueInteger, SysIOVec,
};

// ---------------------------------------------------------------------------
// Opaque types not defined in sys/mod.rs
// ---------------------------------------------------------------------------

/// `ErlNifMutex` — opaque mutex handle.
#[repr(C)]
pub(crate) struct NifMutex {
    _opaque: [u8; 0],
    _marker: std::marker::PhantomData<(*mut u8, std::marker::PhantomPinned)>,
}

/// `ErlNifCond` — opaque condition variable handle.
#[repr(C)]
pub(crate) struct NifCond {
    _opaque: [u8; 0],
    _marker: std::marker::PhantomData<(*mut u8, std::marker::PhantomPinned)>,
}

/// `ErlNifRWLock` — opaque read-write lock handle.
#[repr(C)]
pub(crate) struct NifRWLock {
    _opaque: [u8; 0],
    _marker: std::marker::PhantomData<(*mut u8, std::marker::PhantomPinned)>,
}

/// `ErlNifTid` — thread identifier. In C this is `struct ErlDrvTid_ *`.
pub(crate) type NifTid = *mut c_void;

/// `ErlNifTSDKey` — thread-specific data key.
pub(crate) type NifTSDKey = c_int;

/// `ErlNifThreadOpts` — thread creation options.
#[repr(C)]
pub(crate) struct NifThreadOpts {
    pub suggested_stack_size: c_int,
}

// ---------------------------------------------------------------------------
// Function pointer table
// ---------------------------------------------------------------------------

pub(crate) struct EnifFunctions {
    // =====================================================================
    // NIF 0.1 (OTP R13B03) — initial NIF API
    // =====================================================================
    pub is_atom:            unsafe extern "C" fn(*mut NifEnv, NifTerm) -> c_int,
    pub is_binary:          unsafe extern "C" fn(*mut NifEnv, NifTerm) -> c_int,
    pub is_ref:             unsafe extern "C" fn(*mut NifEnv, NifTerm) -> c_int,
    pub inspect_binary:     unsafe extern "C" fn(*mut NifEnv, NifTerm, *mut NifBinary) -> c_int,
    pub alloc_binary:       unsafe extern "C" fn(usize, *mut NifBinary) -> c_int,
    pub get_int:            unsafe extern "C" fn(*mut NifEnv, NifTerm, *mut c_int) -> c_int,
    pub get_ulong:          unsafe extern "C" fn(*mut NifEnv, NifTerm, *mut std::ffi::c_ulong) -> c_int,
    pub get_double:         unsafe extern "C" fn(*mut NifEnv, NifTerm, *mut f64) -> c_int,
    pub get_list_cell:      unsafe extern "C" fn(*mut NifEnv, NifTerm, *mut NifTerm, *mut NifTerm) -> c_int,
    pub get_tuple:          unsafe extern "C" fn(*mut NifEnv, NifTerm, *mut c_int, *mut *const NifTerm) -> c_int,
    pub is_identical:       unsafe extern "C" fn(NifTerm, NifTerm) -> c_int,
    pub compare:            unsafe extern "C" fn(NifTerm, NifTerm) -> c_int,
    pub make_binary:        unsafe extern "C" fn(*mut NifEnv, *mut NifBinary) -> NifTerm,
    pub make_badarg:        unsafe extern "C" fn(*mut NifEnv) -> NifTerm,
    pub make_int:           unsafe extern "C" fn(*mut NifEnv, c_int) -> NifTerm,
    pub make_ulong:         unsafe extern "C" fn(*mut NifEnv, std::ffi::c_ulong) -> NifTerm,
    pub make_double:        unsafe extern "C" fn(*mut NifEnv, f64) -> NifTerm,
    pub make_atom:          unsafe extern "C" fn(*mut NifEnv, *const c_char) -> NifTerm,
    pub make_existing_atom: unsafe extern "C" fn(*mut NifEnv, *const c_char, *mut NifTerm, NifCharEncoding) -> c_int,
    pub make_list_cell:     unsafe extern "C" fn(*mut NifEnv, NifTerm, NifTerm) -> NifTerm,
    pub make_string:        unsafe extern "C" fn(*mut NifEnv, *const c_char, NifCharEncoding) -> NifTerm,
    pub make_ref:           unsafe extern "C" fn(*mut NifEnv) -> NifTerm,

    // =====================================================================
    // NIF 1.0 (OTP R13B04)
    // =====================================================================
    pub priv_data:          unsafe extern "C" fn(*mut NifEnv) -> *mut c_void,
    pub realloc_binary:     unsafe extern "C" fn(*mut NifBinary, usize) -> c_int,
    pub is_fun:             unsafe extern "C" fn(*mut NifEnv, NifTerm) -> c_int,
    pub is_pid:             unsafe extern "C" fn(*mut NifEnv, NifTerm) -> c_int,
    pub is_port:            unsafe extern "C" fn(*mut NifEnv, NifTerm) -> c_int,
    pub get_uint:           unsafe extern "C" fn(*mut NifEnv, NifTerm, *mut c_uint) -> c_int,
    pub get_long:           unsafe extern "C" fn(*mut NifEnv, NifTerm, *mut std::ffi::c_long) -> c_int,
    pub make_uint:          unsafe extern "C" fn(*mut NifEnv, c_uint) -> NifTerm,
    pub make_long:          unsafe extern "C" fn(*mut NifEnv, std::ffi::c_long) -> NifTerm,
    pub make_tuple_from_array: unsafe extern "C" fn(*mut NifEnv, *const NifTerm, c_uint) -> NifTerm,
    pub make_list_from_array: unsafe extern "C" fn(*mut NifEnv, *const NifTerm, c_uint) -> NifTerm,
    pub is_empty_list:      unsafe extern "C" fn(*mut NifEnv, NifTerm) -> c_int,
    pub open_resource_type: unsafe extern "C" fn(
        *mut NifEnv, *const c_char, *const c_char,
        Option<unsafe extern "C" fn(*mut NifEnv, *mut c_void)>,
        NifResourceFlags, *mut NifResourceFlags,
    ) -> *mut NifResourceType,
    pub alloc_resource:     unsafe extern "C" fn(*mut NifResourceType, usize) -> *mut c_void,
    pub release_resource:   unsafe extern "C" fn(*mut c_void),
    pub make_resource:      unsafe extern "C" fn(*mut NifEnv, *mut c_void) -> NifTerm,
    pub get_resource:       unsafe extern "C" fn(*mut NifEnv, NifTerm, *mut NifResourceType, *mut *mut c_void) -> c_int,
    pub sizeof_resource:    unsafe extern "C" fn(*mut c_void) -> usize,
    pub make_new_binary:    unsafe extern "C" fn(*mut NifEnv, usize, *mut NifTerm) -> *mut u8,
    pub mutex_create:       unsafe extern "C" fn(*mut c_char) -> *mut NifMutex,
    pub mutex_destroy:      unsafe extern "C" fn(*mut NifMutex),
    pub mutex_trylock:      unsafe extern "C" fn(*mut NifMutex) -> c_int,
    pub mutex_lock:         unsafe extern "C" fn(*mut NifMutex),
    pub mutex_unlock:       unsafe extern "C" fn(*mut NifMutex),
    pub cond_create:        unsafe extern "C" fn(*mut c_char) -> *mut NifCond,
    pub cond_destroy:       unsafe extern "C" fn(*mut NifCond),
    pub cond_signal:        unsafe extern "C" fn(*mut NifCond),
    pub cond_broadcast:     unsafe extern "C" fn(*mut NifCond),
    pub cond_wait:          unsafe extern "C" fn(*mut NifCond, *mut NifMutex),
    pub rwlock_create:      unsafe extern "C" fn(*mut c_char) -> *mut NifRWLock,
    pub rwlock_destroy:     unsafe extern "C" fn(*mut NifRWLock),
    pub rwlock_tryrlock:    unsafe extern "C" fn(*mut NifRWLock) -> c_int,
    pub rwlock_rlock:       unsafe extern "C" fn(*mut NifRWLock),
    pub rwlock_runlock:     unsafe extern "C" fn(*mut NifRWLock),
    pub rwlock_tryrwlock:   unsafe extern "C" fn(*mut NifRWLock) -> c_int,
    pub rwlock_rwlock:      unsafe extern "C" fn(*mut NifRWLock),
    pub rwlock_rwunlock:    unsafe extern "C" fn(*mut NifRWLock),
    pub tsd_key_create:     unsafe extern "C" fn(*mut c_char, *mut NifTSDKey) -> c_int,
    pub tsd_key_destroy:    unsafe extern "C" fn(NifTSDKey),
    pub tsd_set:            unsafe extern "C" fn(NifTSDKey, *mut c_void),
    pub tsd_get:            unsafe extern "C" fn(NifTSDKey) -> *mut c_void,
    pub thread_opts_create: unsafe extern "C" fn(*mut c_char) -> *mut NifThreadOpts,
    pub thread_opts_destroy: unsafe extern "C" fn(*mut NifThreadOpts),
    pub thread_create:      unsafe extern "C" fn(
        *mut c_char, *mut NifTid,
        Option<unsafe extern "C" fn(*mut c_void) -> *mut c_void>,
        *mut c_void, *mut NifThreadOpts,
    ) -> c_int,
    pub thread_self:        unsafe extern "C" fn() -> NifTid,
    pub equal_tids:         unsafe extern "C" fn(NifTid, NifTid) -> c_int,
    pub thread_exit:        unsafe extern "C" fn(*mut c_void),
    pub thread_join:        unsafe extern "C" fn(NifTid, *mut *mut c_void) -> c_int,
    pub alloc:              unsafe extern "C" fn(usize) -> *mut c_void,
    pub free:               unsafe extern "C" fn(*mut c_void),
    pub realloc:            unsafe extern "C" fn(*mut c_void, usize) -> *mut c_void,
    pub system_info:        unsafe extern "C" fn(*mut NifSysInfo, usize),
    pub inspect_iolist_as_binary: unsafe extern "C" fn(*mut NifEnv, NifTerm, *mut NifBinary) -> c_int,
    pub make_sub_binary:    unsafe extern "C" fn(*mut NifEnv, NifTerm, usize, usize) -> NifTerm,
    pub get_string:         unsafe extern "C" fn(*mut NifEnv, NifTerm, *mut c_char, c_uint, NifCharEncoding) -> c_int,
    pub get_atom:           unsafe extern "C" fn(*mut NifEnv, NifTerm, *mut c_char, c_uint, NifCharEncoding) -> c_int,

    // =====================================================================
    // NIF 2.0 (OTP R14B)
    // =====================================================================
    pub release_binary:     unsafe extern "C" fn(*mut NifBinary),
    pub is_list:            unsafe extern "C" fn(*mut NifEnv, NifTerm) -> c_int,
    pub is_tuple:           unsafe extern "C" fn(*mut NifEnv, NifTerm) -> c_int,
    pub get_atom_length:    unsafe extern "C" fn(*mut NifEnv, NifTerm, *mut c_uint, NifCharEncoding) -> c_int,
    pub get_list_length:    unsafe extern "C" fn(*mut NifEnv, NifTerm, *mut c_uint) -> c_int,
    pub make_atom_len:      unsafe extern "C" fn(*mut NifEnv, *const c_char, usize) -> NifTerm,
    pub make_existing_atom_len: unsafe extern "C" fn(
        *mut NifEnv, *const c_char, usize, *mut NifTerm, NifCharEncoding,
    ) -> c_int,
    pub make_string_len:    unsafe extern "C" fn(*mut NifEnv, *const c_char, usize, NifCharEncoding) -> NifTerm,
    pub alloc_env:          unsafe extern "C" fn() -> *mut NifEnv,
    pub free_env:           unsafe extern "C" fn(*mut NifEnv),
    pub clear_env:          unsafe extern "C" fn(*mut NifEnv),
    pub send:               unsafe extern "C" fn(*mut NifEnv, *const NifPid, *mut NifEnv, NifTerm) -> c_int,
    pub make_copy:          unsafe extern "C" fn(*mut NifEnv, NifTerm) -> NifTerm,
    pub self_pid:           unsafe extern "C" fn(*mut NifEnv, *mut NifPid) -> *mut NifPid,
    pub get_local_pid:      unsafe extern "C" fn(*mut NifEnv, NifTerm, *mut NifPid) -> c_int,
    pub keep_resource:      unsafe extern "C" fn(*mut c_void),
    pub make_resource_binary: unsafe extern "C" fn(*mut NifEnv, *mut c_void, *const c_void, usize) -> NifTerm,
    // int64/uint64: on 64-bit these are loaded as get_long/make_long.
    // On 32-bit they are separate symbols.
    pub get_i64:            unsafe extern "C" fn(*mut NifEnv, NifTerm, *mut i64) -> c_int,
    pub get_u64:            unsafe extern "C" fn(*mut NifEnv, NifTerm, *mut u64) -> c_int,
    pub make_i64:           unsafe extern "C" fn(*mut NifEnv, i64) -> NifTerm,
    pub make_u64:           unsafe extern "C" fn(*mut NifEnv, u64) -> NifTerm,

    // =====================================================================
    // NIF 2.2 (OTP R14B03)
    // =====================================================================
    pub is_exception:       unsafe extern "C" fn(*mut NifEnv, NifTerm) -> c_int,

    // =====================================================================
    // NIF 2.3 (OTP R15A)
    // =====================================================================
    pub make_reverse_list:  unsafe extern "C" fn(*mut NifEnv, NifTerm, *mut NifTerm) -> c_int,
    pub is_number:          unsafe extern "C" fn(*mut NifEnv, NifTerm) -> c_int,

    // =====================================================================
    // NIF 2.4 (OTP R16B)
    // =====================================================================
    pub dlopen: unsafe extern "C" fn(
        *const c_char,
        Option<unsafe extern "C" fn(*mut c_void, *const c_char)>,
        *mut c_void,
    ) -> *mut c_void,
    pub dlsym: unsafe extern "C" fn(
        *mut c_void, *const c_char,
        Option<unsafe extern "C" fn(*mut c_void, *const c_char)>,
        *mut c_void,
    ) -> *mut c_void,
    pub consume_timeslice:  unsafe extern "C" fn(*mut NifEnv, c_int) -> c_int,

    // =====================================================================
    // NIF 2.6 (OTP 17.0)
    // =====================================================================
    pub is_map:             unsafe extern "C" fn(*mut NifEnv, NifTerm) -> c_int,
    pub get_map_size:       unsafe extern "C" fn(*mut NifEnv, NifTerm, *mut usize) -> c_int,
    pub make_new_map:       unsafe extern "C" fn(*mut NifEnv) -> NifTerm,
    pub make_map_put:       unsafe extern "C" fn(*mut NifEnv, NifTerm, NifTerm, NifTerm, *mut NifTerm) -> c_int,
    pub get_map_value:      unsafe extern "C" fn(*mut NifEnv, NifTerm, NifTerm, *mut NifTerm) -> c_int,
    pub make_map_update:    unsafe extern "C" fn(*mut NifEnv, NifTerm, NifTerm, NifTerm, *mut NifTerm) -> c_int,
    pub make_map_remove:    unsafe extern "C" fn(*mut NifEnv, NifTerm, NifTerm, *mut NifTerm) -> c_int,
    pub map_iterator_create: unsafe extern "C" fn(*mut NifEnv, NifTerm, *mut NifMapIterator, NifMapIteratorEntry) -> c_int,
    pub map_iterator_destroy: unsafe extern "C" fn(*mut NifEnv, *mut NifMapIterator),
    pub map_iterator_is_head: unsafe extern "C" fn(*mut NifEnv, *mut NifMapIterator) -> c_int,
    pub map_iterator_is_tail: unsafe extern "C" fn(*mut NifEnv, *mut NifMapIterator) -> c_int,
    pub map_iterator_next:  unsafe extern "C" fn(*mut NifEnv, *mut NifMapIterator) -> c_int,
    pub map_iterator_prev:  unsafe extern "C" fn(*mut NifEnv, *mut NifMapIterator) -> c_int,
    pub map_iterator_get_pair: unsafe extern "C" fn(*mut NifEnv, *mut NifMapIterator, *mut NifTerm, *mut NifTerm) -> c_int,

    // =====================================================================
    // NIF 2.7 (OTP 17.3)
    // =====================================================================
    pub schedule_nif: unsafe extern "C" fn(
        *mut NifEnv, *const c_char, c_int,
        unsafe extern "C" fn(*mut NifEnv, c_int, *const NifTerm) -> NifTerm,
        c_int, *const NifTerm,
    ) -> NifTerm,

    // =====================================================================
    // NIF 2.8 (OTP 18.0)
    // =====================================================================
    pub has_pending_exception: unsafe extern "C" fn(*mut NifEnv, *mut NifTerm) -> c_int,
    pub raise_exception:    unsafe extern "C" fn(*mut NifEnv, NifTerm) -> NifTerm,

    // =====================================================================
    // NIF 2.9 (OTP 18.2)
    // =====================================================================
    pub getenv:             unsafe extern "C" fn(*const c_char, *mut c_char, *mut usize) -> c_int,

    // =====================================================================
    // NIF 2.10 (OTP 18.3)
    // =====================================================================
    pub monotonic_time:     unsafe extern "C" fn(NifTimeUnit) -> NifTime,
    pub time_offset:        unsafe extern "C" fn(NifTimeUnit) -> NifTime,
    pub convert_time_unit:  unsafe extern "C" fn(NifTime, NifTimeUnit, NifTimeUnit) -> NifTime,

    // =====================================================================
    // NIF 2.11 (OTP 19.0)
    // =====================================================================
    /// Deprecated — use `monotonic_time` + `time_offset`.
    pub now_time:           unsafe extern "C" fn(*mut NifEnv) -> NifTerm,
    /// Deprecated — use OS-level CPU time APIs.
    pub cpu_time:           unsafe extern "C" fn(*mut NifEnv) -> NifTerm,
    pub make_unique_integer: unsafe extern "C" fn(*mut NifEnv, NifUniqueInteger) -> NifTerm,
    pub is_current_process_alive: unsafe extern "C" fn(*mut NifEnv) -> c_int,
    pub is_process_alive:   unsafe extern "C" fn(*mut NifEnv, *mut NifPid) -> c_int,
    pub is_port_alive:      unsafe extern "C" fn(*mut NifEnv, *mut NifPort) -> c_int,
    pub get_local_port:     unsafe extern "C" fn(*mut NifEnv, NifTerm, *mut NifPort) -> c_int,
    pub term_to_binary:     unsafe extern "C" fn(*mut NifEnv, NifTerm, *mut NifBinary) -> c_int,
    pub binary_to_term:     unsafe extern "C" fn(*mut NifEnv, *const u8, usize, *mut NifTerm, c_uint) -> usize,
    pub port_command:       unsafe extern "C" fn(*mut NifEnv, *const NifPort, *mut NifEnv, NifTerm) -> c_int,
    pub thread_type:        unsafe extern "C" fn() -> c_int,
    pub snprintf:           *mut c_void, // variadic

    // =====================================================================
    // NIF 2.12 (OTP 20.0)
    // =====================================================================
    pub select: unsafe extern "C" fn(
        *mut NifEnv, NifEvent, NifSelectFlags,
        *mut c_void, *const NifPid, NifTerm,
    ) -> c_int,
    pub open_resource_type_x: unsafe extern "C" fn(
        *mut NifEnv, *const c_char, *const NifResourceTypeInit,
        NifResourceFlags, *mut NifResourceFlags,
    ) -> *mut NifResourceType,
    pub monitor_process: unsafe extern "C" fn(
        *mut NifEnv, *mut c_void, *const NifPid, *mut NifMonitor,
    ) -> c_int,
    pub demonitor_process: unsafe extern "C" fn(
        *mut NifEnv, *mut c_void, *const NifMonitor,
    ) -> c_int,
    pub compare_monitors:   unsafe extern "C" fn(*const NifMonitor, *const NifMonitor) -> c_int,
    pub hash:               unsafe extern "C" fn(NifHash, NifTerm, u64) -> u64,
    pub whereis_pid:        unsafe extern "C" fn(*mut NifEnv, NifTerm, *mut NifPid) -> c_int,
    pub whereis_port:       unsafe extern "C" fn(*mut NifEnv, NifTerm, *mut NifPort) -> c_int,
    pub ioq_create:         unsafe extern "C" fn(NifIOQueueOpts) -> *mut NifIOQueue,
    pub ioq_destroy:        unsafe extern "C" fn(*mut NifIOQueue),
    pub ioq_enq_binary:     unsafe extern "C" fn(*mut NifIOQueue, *mut NifBinary, usize) -> c_int,
    pub ioq_enqv:           unsafe extern "C" fn(*mut NifIOQueue, *mut NifIOVec, usize) -> c_int,
    pub ioq_size:           unsafe extern "C" fn(*mut NifIOQueue) -> usize,
    pub ioq_deq:            unsafe extern "C" fn(*mut NifIOQueue, usize, *mut usize) -> c_int,
    pub ioq_peek:           unsafe extern "C" fn(*mut NifIOQueue, *mut c_int) -> *mut SysIOVec,
    pub inspect_iovec: unsafe extern "C" fn(
        *mut NifEnv, usize, NifTerm, *mut NifTerm, *mut *mut NifIOVec,
    ) -> c_int,
    pub free_iovec:         unsafe extern "C" fn(*mut NifIOVec),

    // =====================================================================
    // NIF 2.14 (OTP 21.0)
    // =====================================================================
    pub fprintf:            *mut c_void, // variadic — replaces NIF 1.0 version with FILE* support
    pub ioq_peek_head: unsafe extern "C" fn(
        *mut NifEnv, *mut NifIOQueue, *mut usize, *mut NifTerm,
    ) -> c_int,
    pub mutex_name:         unsafe extern "C" fn(*mut NifMutex) -> *mut c_char,
    pub cond_name:          unsafe extern "C" fn(*mut NifCond) -> *mut c_char,
    pub rwlock_name:        unsafe extern "C" fn(*mut NifRWLock) -> *mut c_char,
    pub thread_name:        unsafe extern "C" fn(NifTid) -> *mut c_char,
    pub vfprintf:           *mut c_void, // va_list variant
    pub vsnprintf:          *mut c_void, // va_list variant
    pub make_map_from_arrays: unsafe extern "C" fn(
        *mut NifEnv, *const NifTerm, *const NifTerm, usize, *mut NifTerm,
    ) -> c_int,

    // =====================================================================
    // NIF 2.15 (OTP 22.0)
    // =====================================================================
    pub select_x: unsafe extern "C" fn(
        *mut NifEnv, NifEvent, NifSelectFlags,
        *mut c_void, *const NifPid, NifTerm, *mut NifEnv,
    ) -> c_int,
    pub make_monitor_term:  unsafe extern "C" fn(*mut NifEnv, *const NifMonitor) -> NifTerm,
    pub set_pid_undefined:  unsafe extern "C" fn(*mut NifPid),
    pub is_pid_undefined:   unsafe extern "C" fn(*const NifPid) -> c_int,
    pub term_type:          unsafe extern "C" fn(*mut NifEnv, NifTerm) -> NifTermType,

    // =====================================================================
    // NIF 2.16 (OTP 24.0)
    // =====================================================================
    pub init_resource_type: unsafe extern "C" fn(
        *mut NifEnv, *const c_char, *const NifResourceTypeInit,
        NifResourceFlags, *mut NifResourceFlags,
    ) -> *mut NifResourceType,
    pub dynamic_resource_call: unsafe extern "C" fn(
        *mut NifEnv, NifTerm, NifTerm, NifTerm, *mut c_void,
    ) -> c_int,

    // =====================================================================
    // NIF 2.17 (OTP 26.0)
    // =====================================================================
    pub get_string_length:  unsafe extern "C" fn(*mut NifEnv, NifTerm, *mut c_uint, NifCharEncoding) -> c_int,
    pub make_new_atom:      unsafe extern "C" fn(*mut NifEnv, *const c_char, *mut NifTerm, NifCharEncoding) -> c_int,
    pub make_new_atom_len: unsafe extern "C" fn(
        *mut NifEnv, *const c_char, usize, *mut NifTerm, NifCharEncoding,
    ) -> c_int,
    pub set_option:         *mut c_void, // variadic — transmuted per option variant

    // =====================================================================
    // NIF 2.18 (OTP 29.0)
    // =====================================================================
    #[cfg(feature = "nif_2_18")]
    pub term_size:          unsafe extern "C" fn(NifTerm) -> usize,
    #[cfg(feature = "nif_2_18")]
    pub get_atom_cache_index: unsafe extern "C" fn(*mut NifEnv, NifTerm, *mut c_uint) -> c_int,
    #[cfg(feature = "nif_2_18")]
    pub max_atom_cache_index: unsafe extern "C" fn() -> c_uint,
}

// SAFETY: EnifFunctions holds only C function pointers and raw pointers to
// C functions. The BEAM's enif_* functions are thread-safe by design.
unsafe impl Send for EnifFunctions {}
unsafe impl Sync for EnifFunctions {}

// ---------------------------------------------------------------------------
// Global storage
// ---------------------------------------------------------------------------

static FUNCS: OnceLock<EnifFunctions> = OnceLock::new();

#[inline]
pub(crate) fn funcs() -> &'static EnifFunctions {
    FUNCS
        .get()
        .expect("otter::enif: not initialized — init() was not called")
}

// ---------------------------------------------------------------------------
// Initialization — Unix (dlsym)
// ---------------------------------------------------------------------------

/// Load all `enif_*` function pointers via `dlsym` and store them globally.
///
/// Must be called exactly once from the NIF load entry point before any
/// other function in this module is used.
///
/// Returns `Ok(())` on success, or `Err(name)` with the first symbol that
/// could not be resolved. A failed load leaves `FUNCS` uninitialized so
/// that `funcs()` will panic if called — but the caller should propagate
/// the error to the BEAM (return `false` from the NIF load callback) so
/// the VM stays alive.
///
/// # Safety
///
/// Must be called from the BEAM's NIF loading context.
#[cfg(unix)]
pub(crate) unsafe fn init() -> Result<(), &'static str> {
    // Fast path: already initialized.
    if FUNCS.get().is_some() {
        return Ok(());
    }

    unsafe fn load<T>(name: &[u8]) -> Result<T, &'static str> {
        assert!(
            std::mem::size_of::<T>() == std::mem::size_of::<*mut c_void>(),
            "load<T>: T must be a function pointer"
        );
        let sym = unsafe {
            libc::dlsym(libc::RTLD_DEFAULT, name.as_ptr() as *const c_char)
        };
        if sym.is_null() {
            // Strip trailing NUL for the error message.
            let s = std::str::from_utf8(&name[..name.len() - 1])
                .unwrap_or("<invalid utf8>");
            // SAFETY: these are all &'static byte literals, so the str is 'static.
            return Err(unsafe { &*(s as *const str) });
        }
        Ok(unsafe { std::mem::transmute_copy(&sym) })
    }

    unsafe fn load_raw(name: &[u8]) -> Result<*mut c_void, &'static str> {
        let sym = unsafe {
            libc::dlsym(libc::RTLD_DEFAULT, name.as_ptr() as *const c_char)
        };
        if sym.is_null() {
            let s = std::str::from_utf8(&name[..name.len() - 1])
                .unwrap_or("<invalid utf8>");
            return Err(unsafe { &*(s as *const str) });
        }
        Ok(sym)
    }

    // On 64-bit: sizeof(long) == 8, so the header aliases int64 → long.
    #[cfg(target_pointer_width = "64")]
    let (sym_get_i64, sym_get_u64, sym_make_i64, sym_make_u64) = (
        b"enif_get_long\0".as_ref(),
        b"enif_get_ulong\0".as_ref(),
        b"enif_make_long\0".as_ref(),
        b"enif_make_ulong\0".as_ref(),
    );
    #[cfg(not(target_pointer_width = "64"))]
    let (sym_get_i64, sym_get_u64, sym_make_i64, sym_make_u64) = (
        b"enif_get_int64\0".as_ref(),
        b"enif_get_uint64\0".as_ref(),
        b"enif_make_int64\0".as_ref(),
        b"enif_make_uint64\0".as_ref(),
    );

    let funcs: Result<EnifFunctions, &'static str> = unsafe {
        Ok(EnifFunctions {
                // Memory
                priv_data:          load(b"enif_priv_data\0")?,
                alloc:              load(b"enif_alloc\0")?,
                free:               load(b"enif_free\0")?,
                realloc:            load(b"enif_realloc\0")?,

                // Type checks
                is_atom:            load(b"enif_is_atom\0")?,
                is_binary:          load(b"enif_is_binary\0")?,
                is_ref:             load(b"enif_is_ref\0")?,
                is_fun:             load(b"enif_is_fun\0")?,
                is_pid:             load(b"enif_is_pid\0")?,
                is_port:            load(b"enif_is_port\0")?,
                is_list:            load(b"enif_is_list\0")?,
                is_tuple:           load(b"enif_is_tuple\0")?,
                is_empty_list:      load(b"enif_is_empty_list\0")?,
                is_map:             load(b"enif_is_map\0")?,
                is_number:          load(b"enif_is_number\0")?,
                is_exception:       load(b"enif_is_exception\0")?,
                is_identical:       load(b"enif_is_identical\0")?,
                compare:            load(b"enif_compare\0")?,

                // Binary
                inspect_binary:     load(b"enif_inspect_binary\0")?,
                alloc_binary:       load(b"enif_alloc_binary\0")?,
                realloc_binary:     load(b"enif_realloc_binary\0")?,
                release_binary:     load(b"enif_release_binary\0")?,
                make_binary:        load(b"enif_make_binary\0")?,
                make_new_binary:    load(b"enif_make_new_binary\0")?,
                make_sub_binary:    load(b"enif_make_sub_binary\0")?,
                inspect_iolist_as_binary: load(b"enif_inspect_iolist_as_binary\0")?,
                make_resource_binary: load(b"enif_make_resource_binary\0")?,

                // Integer
                get_int:            load(b"enif_get_int\0")?,
                get_uint:           load(b"enif_get_uint\0")?,
                get_long:           load(b"enif_get_long\0")?,
                get_ulong:          load(b"enif_get_ulong\0")?,
                get_double:         load(b"enif_get_double\0")?,
                make_int:           load(b"enif_make_int\0")?,
                make_uint:          load(b"enif_make_uint\0")?,
                make_long:          load(b"enif_make_long\0")?,
                make_ulong:         load(b"enif_make_ulong\0")?,
                make_double:        load(b"enif_make_double\0")?,
                get_i64:            load(sym_get_i64)?,
                get_u64:            load(sym_get_u64)?,
                make_i64:           load(sym_make_i64)?,
                make_u64:           load(sym_make_u64)?,

                // Atom
                make_atom:          load(b"enif_make_atom\0")?,
                make_existing_atom: load(b"enif_make_existing_atom\0")?,
                make_atom_len:      load(b"enif_make_atom_len\0")?,
                make_existing_atom_len: load(b"enif_make_existing_atom_len\0")?,
                get_atom:           load(b"enif_get_atom\0")?,
                get_atom_length:    load(b"enif_get_atom_length\0")?,

                // List
                get_list_cell:      load(b"enif_get_list_cell\0")?,
                get_list_length:    load(b"enif_get_list_length\0")?,
                make_list_cell:     load(b"enif_make_list_cell\0")?,
                make_list_from_array: load(b"enif_make_list_from_array\0")?,
                make_reverse_list:  load(b"enif_make_reverse_list\0")?,

                // Tuple
                get_tuple:          load(b"enif_get_tuple\0")?,
                make_tuple_from_array: load(b"enif_make_tuple_from_array\0")?,

                // String
                make_string:        load(b"enif_make_string\0")?,
                make_string_len:    load(b"enif_make_string_len\0")?,
                get_string:         load(b"enif_get_string\0")?,

                // Map
                make_new_map:       load(b"enif_make_new_map\0")?,
                get_map_size:       load(b"enif_get_map_size\0")?,
                get_map_value:      load(b"enif_get_map_value\0")?,
                make_map_put:       load(b"enif_make_map_put\0")?,
                make_map_update:    load(b"enif_make_map_update\0")?,
                make_map_remove:    load(b"enif_make_map_remove\0")?,
                map_iterator_create:   load(b"enif_map_iterator_create\0")?,
                map_iterator_destroy:  load(b"enif_map_iterator_destroy\0")?,
                map_iterator_is_head:  load(b"enif_map_iterator_is_head\0")?,
                map_iterator_is_tail:  load(b"enif_map_iterator_is_tail\0")?,
                map_iterator_next:     load(b"enif_map_iterator_next\0")?,
                map_iterator_prev:     load(b"enif_map_iterator_prev\0")?,
                map_iterator_get_pair: load(b"enif_map_iterator_get_pair\0")?,
                make_map_from_arrays:  load(b"enif_make_map_from_arrays\0")?,

                // Ref / Unique integer
                make_ref:           load(b"enif_make_ref\0")?,
                make_unique_integer: load(b"enif_make_unique_integer\0")?,

                // Pid
                self_pid:           load(b"enif_self\0")?,
                get_local_pid:      load(b"enif_get_local_pid\0")?,
                is_process_alive:   load(b"enif_is_process_alive\0")?,
                is_current_process_alive: load(b"enif_is_current_process_alive\0")?,
                whereis_pid:        load(b"enif_whereis_pid\0")?,

                // Port
                get_local_port:     load(b"enif_get_local_port\0")?,
                is_port_alive:      load(b"enif_is_port_alive\0")?,
                whereis_port:       load(b"enif_whereis_port\0")?,
                port_command:       load(b"enif_port_command\0")?,

                // Env / send
                alloc_env:          load(b"enif_alloc_env\0")?,
                free_env:           load(b"enif_free_env\0")?,
                clear_env:          load(b"enif_clear_env\0")?,
                send:               load(b"enif_send\0")?,
                make_copy:          load(b"enif_make_copy\0")?,

                // Resource
                open_resource_type: load(b"enif_open_resource_type\0")?,
                open_resource_type_x: load(b"enif_open_resource_type_x\0")?,
                alloc_resource:     load(b"enif_alloc_resource\0")?,
                release_resource:   load(b"enif_release_resource\0")?,
                make_resource:      load(b"enif_make_resource\0")?,
                get_resource:       load(b"enif_get_resource\0")?,
                sizeof_resource:    load(b"enif_sizeof_resource\0")?,
                keep_resource:      load(b"enif_keep_resource\0")?,

                // Exception
                make_badarg:        load(b"enif_make_badarg\0")?,
                has_pending_exception: load(b"enif_has_pending_exception\0")?,
                raise_exception:    load(b"enif_raise_exception\0")?,

                // Schedule
                schedule_nif:       load(b"enif_schedule_nif\0")?,

                // Monitor
                monitor_process:    load(b"enif_monitor_process\0")?,
                demonitor_process:  load(b"enif_demonitor_process\0")?,
                compare_monitors:   load(b"enif_compare_monitors\0")?,

                // Select
                select:             load(b"enif_select\0")?,

                // Time
                monotonic_time:     load(b"enif_monotonic_time\0")?,
                time_offset:        load(b"enif_time_offset\0")?,
                convert_time_unit:  load(b"enif_convert_time_unit\0")?,
                now_time:           load(b"enif_now_time\0")?,
                cpu_time:           load(b"enif_cpu_time\0")?,

                // Hash
                hash:               load(b"enif_hash\0")?,

                // Term serialization
                term_to_binary:     load(b"enif_term_to_binary\0")?,
                binary_to_term:     load(b"enif_binary_to_term\0")?,

                // Timeslice
                consume_timeslice:  load(b"enif_consume_timeslice\0")?,

                // System
                system_info:        load(b"enif_system_info\0")?,
                getenv:             load(b"enif_getenv\0")?,
                thread_type:        load(b"enif_thread_type\0")?,

                // Dynamic loading
                dlopen:             load(b"enif_dlopen\0")?,
                dlsym:              load(b"enif_dlsym\0")?,

                // Threading
                mutex_create:       load(b"enif_mutex_create\0")?,
                mutex_destroy:      load(b"enif_mutex_destroy\0")?,
                mutex_trylock:      load(b"enif_mutex_trylock\0")?,
                mutex_lock:         load(b"enif_mutex_lock\0")?,
                mutex_unlock:       load(b"enif_mutex_unlock\0")?,
                cond_create:        load(b"enif_cond_create\0")?,
                cond_destroy:       load(b"enif_cond_destroy\0")?,
                cond_signal:        load(b"enif_cond_signal\0")?,
                cond_broadcast:     load(b"enif_cond_broadcast\0")?,
                cond_wait:          load(b"enif_cond_wait\0")?,
                rwlock_create:      load(b"enif_rwlock_create\0")?,
                rwlock_destroy:     load(b"enif_rwlock_destroy\0")?,
                rwlock_tryrlock:    load(b"enif_rwlock_tryrlock\0")?,
                rwlock_rlock:       load(b"enif_rwlock_rlock\0")?,
                rwlock_runlock:     load(b"enif_rwlock_runlock\0")?,
                rwlock_tryrwlock:   load(b"enif_rwlock_tryrwlock\0")?,
                rwlock_rwlock:      load(b"enif_rwlock_rwlock\0")?,
                rwlock_rwunlock:    load(b"enif_rwlock_rwunlock\0")?,
                tsd_key_create:     load(b"enif_tsd_key_create\0")?,
                tsd_key_destroy:    load(b"enif_tsd_key_destroy\0")?,
                tsd_set:            load(b"enif_tsd_set\0")?,
                tsd_get:            load(b"enif_tsd_get\0")?,
                thread_opts_create: load(b"enif_thread_opts_create\0")?,
                thread_opts_destroy: load(b"enif_thread_opts_destroy\0")?,
                thread_create:      load(b"enif_thread_create\0")?,
                thread_self:        load(b"enif_thread_self\0")?,
                equal_tids:         load(b"enif_equal_tids\0")?,
                thread_exit:        load(b"enif_thread_exit\0")?,
                thread_join:        load(b"enif_thread_join\0")?,
                mutex_name:         load(b"enif_mutex_name\0")?,
                cond_name:          load(b"enif_cond_name\0")?,
                rwlock_name:        load(b"enif_rwlock_name\0")?,
                thread_name:        load(b"enif_thread_name\0")?,

                // Formatted output (variadic / va_list — raw pointers)
                fprintf:            load_raw(b"enif_fprintf\0")?,
                snprintf:           load_raw(b"enif_snprintf\0")?,
                vfprintf:           load_raw(b"enif_vfprintf\0")?,
                vsnprintf:          load_raw(b"enif_vsnprintf\0")?,

                // I/O Queue
                ioq_create:         load(b"enif_ioq_create\0")?,
                ioq_destroy:        load(b"enif_ioq_destroy\0")?,
                ioq_enq_binary:     load(b"enif_ioq_enq_binary\0")?,
                ioq_enqv:           load(b"enif_ioq_enqv\0")?,
                ioq_size:           load(b"enif_ioq_size\0")?,
                ioq_deq:            load(b"enif_ioq_deq\0")?,
                ioq_peek:           load(b"enif_ioq_peek\0")?,
                inspect_iovec:      load(b"enif_inspect_iovec\0")?,
                free_iovec:         load(b"enif_free_iovec\0")?,
                ioq_peek_head:      load(b"enif_ioq_peek_head\0")?,

                // NIF 2.15
                select_x:           load(b"enif_select_x\0")?,
                make_monitor_term:  load(b"enif_make_monitor_term\0")?,
                set_pid_undefined:  load(b"enif_set_pid_undefined\0")?,
                is_pid_undefined:   load(b"enif_is_pid_undefined\0")?,
                term_type:          load(b"enif_term_type\0")?,

                // NIF 2.16
                init_resource_type: load(b"enif_init_resource_type\0")?,
                dynamic_resource_call: load(b"enif_dynamic_resource_call\0")?,

                // NIF 2.17
                get_string_length:  load(b"enif_get_string_length\0")?,
                make_new_atom:      load(b"enif_make_new_atom\0")?,
                make_new_atom_len:  load(b"enif_make_new_atom_len\0")?,
                set_option:         load_raw(b"enif_set_option\0")?,

                // NIF 2.18
                #[cfg(feature = "nif_2_18")]
                term_size:          load(b"enif_term_size\0")?,
                #[cfg(feature = "nif_2_18")]
                get_atom_cache_index: load(b"enif_get_atom_cache_index\0")?,
                #[cfg(feature = "nif_2_18")]
                max_atom_cache_index: load(b"enif_max_atom_cache_index\0")?,
        })
    };

    let funcs = funcs?;
    let _ = FUNCS.set(funcs);
    Ok(())
}

#[cfg(not(unix))]
pub(crate) unsafe fn init() -> Result<(), &'static str> {
    compile_error!("otter::enif: only Unix is supported at this time");
}

// ===========================================================================
// Wrapper functions — baseline (NIF ≤ 2.14)
// ===========================================================================

// -- Memory ---------------------------------------------------------------
// NIF 0.1: alloc(env, size), free(env, ptr)
// NIF 2.0: env parameter removed

/// Returns the pointer to the private data set by `load` or `upgrade`. NIF 1.0 (OTP R13B04). Wraps `enif_priv_data`.
pub(crate) unsafe fn priv_data(env: *mut NifEnv) -> *mut c_void {
    unsafe { (funcs().priv_data)(env) }
}

/// Allocates `size` bytes of memory. Returns `NULL` on failure. NIF 1.0 (OTP R13B04). Wraps `enif_alloc`.
pub(crate) unsafe fn alloc(size: usize) -> *mut c_void {
    unsafe { (funcs().alloc)(size) }
}

/// Frees memory allocated by [`alloc`]. NIF 1.0 (OTP R13B04). Wraps `enif_free`.
pub(crate) unsafe fn free(ptr: *mut c_void) {
    unsafe { (funcs().free)(ptr) }
}

/// Reallocates memory to `size` bytes. Returns `NULL` on failure. NIF 1.0 (OTP R13B04). Wraps `enif_realloc`.
pub(crate) unsafe fn realloc(ptr: *mut c_void, size: usize) -> *mut c_void {
    unsafe { (funcs().realloc)(ptr, size) }
}

// -- Type checks ----------------------------------------------------------
// NIF 0.1: is_atom, is_binary, is_ref
// NIF 1.0: is_fun, is_pid, is_port
// NIF 2.0: is_list, is_tuple, is_empty_list (implicit in earlier versions)
// NIF 2.2: is_exception
// NIF 2.3: is_number
// NIF 2.6: is_map

/// Returns non-zero if `term` is an atom. NIF 0.1 (OTP R13B03). Wraps `enif_is_atom`.
pub(crate) unsafe fn is_atom(env: *mut NifEnv, term: NifTerm) -> c_int {
    unsafe { (funcs().is_atom)(env, term) }
}

/// Returns non-zero if `term` is a binary. NIF 0.1 (OTP R13B03). Wraps `enif_is_binary`.
pub(crate) unsafe fn is_binary(env: *mut NifEnv, term: NifTerm) -> c_int {
    unsafe { (funcs().is_binary)(env, term) }
}

/// Returns non-zero if `term` is a reference. NIF 0.1 (OTP R13B03). Wraps `enif_is_ref`.
pub(crate) unsafe fn is_ref(env: *mut NifEnv, term: NifTerm) -> c_int {
    unsafe { (funcs().is_ref)(env, term) }
}

/// Returns non-zero if `term` is a fun. NIF 1.0 (OTP R13B04). Wraps `enif_is_fun`.
pub(crate) unsafe fn is_fun(env: *mut NifEnv, term: NifTerm) -> c_int {
    unsafe { (funcs().is_fun)(env, term) }
}

/// Returns non-zero if `term` is a pid. NIF 1.0 (OTP R13B04). Wraps `enif_is_pid`.
pub(crate) unsafe fn is_pid(env: *mut NifEnv, term: NifTerm) -> c_int {
    unsafe { (funcs().is_pid)(env, term) }
}

/// Returns non-zero if `term` is a port. NIF 1.0 (OTP R13B04). Wraps `enif_is_port`.
pub(crate) unsafe fn is_port(env: *mut NifEnv, term: NifTerm) -> c_int {
    unsafe { (funcs().is_port)(env, term) }
}

/// Returns non-zero if `term` is a list. NIF 2.0 (OTP R14B). Wraps `enif_is_list`.
pub(crate) unsafe fn is_list(env: *mut NifEnv, term: NifTerm) -> c_int {
    unsafe { (funcs().is_list)(env, term) }
}

/// Returns non-zero if `term` is a tuple. NIF 2.0 (OTP R14B). Wraps `enif_is_tuple`.
pub(crate) unsafe fn is_tuple(env: *mut NifEnv, term: NifTerm) -> c_int {
    unsafe { (funcs().is_tuple)(env, term) }
}

/// Returns non-zero if `term` is an empty list (`[]`). NIF 1.0 (OTP R13B04). Wraps `enif_is_empty_list`.
pub(crate) unsafe fn is_empty_list(env: *mut NifEnv, term: NifTerm) -> c_int {
    unsafe { (funcs().is_empty_list)(env, term) }
}

/// Returns non-zero if `term` is a map. NIF 2.6 (OTP 17.0). Wraps `enif_is_map`.
pub(crate) unsafe fn is_map(env: *mut NifEnv, term: NifTerm) -> c_int {
    unsafe { (funcs().is_map)(env, term) }
}

/// Returns non-zero if `term` is a number (integer or float). NIF 2.3 (OTP R15A). Wraps `enif_is_number`.
pub(crate) unsafe fn is_number(env: *mut NifEnv, term: NifTerm) -> c_int {
    unsafe { (funcs().is_number)(env, term) }
}

/// Returns non-zero if `term` is an exception. NIF 2.2 (OTP R14B03). Wraps `enif_is_exception`.
pub(crate) unsafe fn is_exception(env: *mut NifEnv, term: NifTerm) -> c_int {
    unsafe { (funcs().is_exception)(env, term) }
}

/// Returns non-zero if `lhs` and `rhs` are identical (Erlang `=:=`). NIF 0.1 (OTP R13B03). Wraps `enif_is_identical`.
pub(crate) unsafe fn is_identical(lhs: NifTerm, rhs: NifTerm) -> c_int {
    unsafe { (funcs().is_identical)(lhs, rhs) }
}

/// Compares two terms using Erlang term ordering. Returns negative if `lhs` < `rhs`,
/// zero if equal, positive if `lhs` > `rhs`. NIF 0.1 (OTP R13B03). Wraps `enif_compare`.
pub(crate) unsafe fn compare(lhs: NifTerm, rhs: NifTerm) -> c_int {
    unsafe { (funcs().compare)(lhs, rhs) }
}

// -- Binary ---------------------------------------------------------------

/// Initializes `bin` with info about binary `term`. Returns non-zero on success. NIF 0.1 (OTP R13B03). Wraps `enif_inspect_binary`.
pub(crate) unsafe fn inspect_binary(
    env: *mut NifEnv, term: NifTerm, bin: *mut NifBinary,
) -> c_int {
    unsafe { (funcs().inspect_binary)(env, term, bin) }
}

/// Allocates a new binary of `size` bytes. Returns non-zero on success. NIF 0.1 (OTP R13B03). Wraps `enif_alloc_binary`.
pub(crate) unsafe fn alloc_binary(size: usize, bin: *mut NifBinary) -> c_int {
    unsafe { (funcs().alloc_binary)(size, bin) }
}

/// Changes the size of `bin`. Returns non-zero on success. NIF 1.0 (OTP R13B04). Wraps `enif_realloc_binary`.
pub(crate) unsafe fn realloc_binary(bin: *mut NifBinary, size: usize) -> c_int {
    unsafe { (funcs().realloc_binary)(bin, size) }
}

/// Releases a binary obtained from [`alloc_binary`]. NIF 2.0 (OTP R14B). Wraps `enif_release_binary`.
pub(crate) unsafe fn release_binary(bin: *mut NifBinary) {
    unsafe { (funcs().release_binary)(bin) }
}

/// Creates a binary term from `bin`, transferring ownership of the data. NIF 0.1 (OTP R13B03). Wraps `enif_make_binary`.
pub(crate) unsafe fn make_binary(env: *mut NifEnv, bin: *mut NifBinary) -> NifTerm {
    unsafe { (funcs().make_binary)(env, bin) }
}

/// Allocates a binary of `size` bytes, sets `*termp` to the term, and returns a pointer
/// to the raw data. NIF 1.0 (OTP R13B04). Wraps `enif_make_new_binary`.
pub(crate) unsafe fn make_new_binary(
    env: *mut NifEnv, size: usize, termp: *mut NifTerm,
) -> *mut u8 {
    unsafe { (funcs().make_new_binary)(env, size, termp) }
}

/// Creates a subbinary of `bin_term` starting at byte `pos` with length `size`. NIF 1.0 (OTP R13B04). Wraps `enif_make_sub_binary`.
pub(crate) unsafe fn make_sub_binary(
    env: *mut NifEnv, bin_term: NifTerm, pos: usize, size: usize,
) -> NifTerm {
    unsafe { (funcs().make_sub_binary)(env, bin_term, pos, size) }
}

/// Copies iolist `term` into a contiguous binary buffer. Returns non-zero on success. NIF 1.0 (OTP R13B04). Wraps `enif_inspect_iolist_as_binary`.
pub(crate) unsafe fn inspect_iolist_as_binary(
    env: *mut NifEnv, term: NifTerm, bin: *mut NifBinary,
) -> c_int {
    unsafe { (funcs().inspect_iolist_as_binary)(env, term, bin) }
}

/// Creates a binary term backed by resource `obj` at `data` for `size` bytes. NIF 2.0 (OTP R14B). Wraps `enif_make_resource_binary`.
pub(crate) unsafe fn make_resource_binary(
    env: *mut NifEnv, obj: *mut c_void, data: *const c_void, size: usize,
) -> NifTerm {
    unsafe { (funcs().make_resource_binary)(env, obj, data, size) }
}

// -- Integer / Float ------------------------------------------------------

/// Gets the `int` value of `term`. Returns non-zero on success. NIF 0.1 (OTP R13B03). Wraps `enif_get_int`.
pub(crate) unsafe fn get_int(env: *mut NifEnv, term: NifTerm, ip: *mut c_int) -> c_int {
    unsafe { (funcs().get_int)(env, term, ip) }
}

/// Gets the `unsigned int` value of `term`. Returns non-zero on success. NIF 1.0 (OTP R13B04). Wraps `enif_get_uint`.
pub(crate) unsafe fn get_uint(env: *mut NifEnv, term: NifTerm, ip: *mut c_uint) -> c_int {
    unsafe { (funcs().get_uint)(env, term, ip) }
}

/// Gets the `long` value of `term`. Returns non-zero on success. NIF 1.0 (OTP R13B04). Wraps `enif_get_long`.
pub(crate) unsafe fn get_long(
    env: *mut NifEnv, term: NifTerm, ip: *mut std::ffi::c_long,
) -> c_int {
    unsafe { (funcs().get_long)(env, term, ip) }
}

/// Gets the `unsigned long` value of `term`. Returns non-zero on success. NIF 0.1 (OTP R13B03). Wraps `enif_get_ulong`.
pub(crate) unsafe fn get_ulong(
    env: *mut NifEnv, term: NifTerm, ip: *mut std::ffi::c_ulong,
) -> c_int {
    unsafe { (funcs().get_ulong)(env, term, ip) }
}

/// Gets the `double` value of `term`. Returns non-zero on success. NIF 0.1 (OTP R13B03). Wraps `enif_get_double`.
pub(crate) unsafe fn get_double(env: *mut NifEnv, term: NifTerm, dp: *mut f64) -> c_int {
    unsafe { (funcs().get_double)(env, term, dp) }
}

/// Creates an integer term from a C `int`. NIF 0.1 (OTP R13B03). Wraps `enif_make_int`.
pub(crate) unsafe fn make_int(env: *mut NifEnv, i: c_int) -> NifTerm {
    unsafe { (funcs().make_int)(env, i) }
}

/// Creates an integer term from a C `unsigned int`. NIF 1.0 (OTP R13B04). Wraps `enif_make_uint`.
pub(crate) unsafe fn make_uint(env: *mut NifEnv, i: c_uint) -> NifTerm {
    unsafe { (funcs().make_uint)(env, i) }
}

/// Creates an integer term from a C `long`. NIF 1.0 (OTP R13B04). Wraps `enif_make_long`.
pub(crate) unsafe fn make_long(env: *mut NifEnv, i: std::ffi::c_long) -> NifTerm {
    unsafe { (funcs().make_long)(env, i) }
}

/// Creates an integer term from a C `unsigned long`. NIF 0.1 (OTP R13B03). Wraps `enif_make_ulong`.
pub(crate) unsafe fn make_ulong(env: *mut NifEnv, i: std::ffi::c_ulong) -> NifTerm {
    unsafe { (funcs().make_ulong)(env, i) }
}

/// Creates a floating-point term. The value must be finite. NIF 0.1 (OTP R13B03). Wraps `enif_make_double`.
pub(crate) unsafe fn make_double(env: *mut NifEnv, d: f64) -> NifTerm {
    unsafe { (funcs().make_double)(env, d) }
}

/// Gets the signed 64-bit integer value of `term`. Returns non-zero on success. NIF 2.0 (OTP R14B). Wraps `enif_get_int64`.
pub(crate) unsafe fn get_i64(env: *mut NifEnv, term: NifTerm, ip: *mut i64) -> c_int {
    unsafe { (funcs().get_i64)(env, term, ip) }
}

/// Gets the unsigned 64-bit integer value of `term`. Returns non-zero on success. NIF 2.0 (OTP R14B). Wraps `enif_get_uint64`.
pub(crate) unsafe fn get_u64(env: *mut NifEnv, term: NifTerm, ip: *mut u64) -> c_int {
    unsafe { (funcs().get_u64)(env, term, ip) }
}

/// Creates an integer term from a signed 64-bit integer. NIF 2.0 (OTP R14B). Wraps `enif_make_int64`.
pub(crate) unsafe fn make_i64(env: *mut NifEnv, i: i64) -> NifTerm {
    unsafe { (funcs().make_i64)(env, i) }
}

/// Creates an integer term from an unsigned 64-bit integer. NIF 2.0 (OTP R14B). Wraps `enif_make_uint64`.
pub(crate) unsafe fn make_u64(env: *mut NifEnv, i: u64) -> NifTerm {
    unsafe { (funcs().make_u64)(env, i) }
}

// -- Atom -----------------------------------------------------------------

/// Creates an atom from a null-terminated Latin-1 string. NIF 0.1 (OTP R13B03). Wraps `enif_make_atom`.
pub(crate) unsafe fn make_atom(env: *mut NifEnv, name: *const c_char) -> NifTerm {
    unsafe { (funcs().make_atom)(env, name) }
}

/// Looks up an existing atom. Returns non-zero on success. NIF 0.1 (OTP R13B03). Wraps `enif_make_existing_atom`.
pub(crate) unsafe fn make_existing_atom(
    env: *mut NifEnv, name: *const c_char, atom: *mut NifTerm, encoding: NifCharEncoding,
) -> c_int {
    unsafe { (funcs().make_existing_atom)(env, name, atom, encoding) }
}

/// Creates an atom from a name of `len` bytes in Latin-1. NIF 2.0 (OTP R14B). Wraps `enif_make_atom_len`.
pub(crate) unsafe fn make_atom_len(
    env: *mut NifEnv, name: *const c_char, len: usize,
) -> NifTerm {
    unsafe { (funcs().make_atom_len)(env, name, len) }
}

/// Looks up an existing atom by name and length. Returns non-zero on success. NIF 2.0 (OTP R14B). Wraps `enif_make_existing_atom_len`.
pub(crate) unsafe fn make_existing_atom_len(
    env: *mut NifEnv, name: *const c_char, len: usize, atom: *mut NifTerm,
    encoding: NifCharEncoding,
) -> c_int {
    unsafe { (funcs().make_existing_atom_len)(env, name, len, atom, encoding) }
}

/// Writes the atom name into `buf`. Returns the number of bytes written (including null
/// terminator), or 0 on failure. NIF 1.0 (OTP R13B04). Wraps `enif_get_atom`.
pub(crate) unsafe fn get_atom(
    env: *mut NifEnv, atom: NifTerm, buf: *mut c_char, len: c_uint,
    encoding: NifCharEncoding,
) -> c_int {
    unsafe { (funcs().get_atom)(env, atom, buf, len, encoding) }
}

/// Gets the length of an atom name in bytes. Returns non-zero on success. NIF 2.0 (OTP R14B). Wraps `enif_get_atom_length`.
pub(crate) unsafe fn get_atom_length(
    env: *mut NifEnv, atom: NifTerm, len: *mut c_uint, encoding: NifCharEncoding,
) -> c_int {
    unsafe { (funcs().get_atom_length)(env, atom, len, encoding) }
}

// -- List -----------------------------------------------------------------

/// Sets `head` and `tail` from a list cons cell, returning non-zero on success or 0 if the term is not a non-empty list. NIF 0.1 (OTP R13B03). Wraps `enif_get_list_cell`.
pub(crate) unsafe fn get_list_cell(
    env: *mut NifEnv, term: NifTerm, head: *mut NifTerm, tail: *mut NifTerm,
) -> c_int {
    unsafe { (funcs().get_list_cell)(env, term, head, tail) }
}

/// Sets `*len` to the length of a list, returning non-zero on success or 0 if not a proper list. NIF 2.0 (OTP R14B). Wraps `enif_get_list_length`.
pub(crate) unsafe fn get_list_length(
    env: *mut NifEnv, term: NifTerm, len: *mut c_uint,
) -> c_int {
    unsafe { (funcs().get_list_length)(env, term, len) }
}

/// Creates a list cell `[car | cdr]`. NIF 0.1 (OTP R13B03). Wraps `enif_make_list_cell`.
pub(crate) unsafe fn make_list_cell(
    env: *mut NifEnv, car: NifTerm, cdr: NifTerm,
) -> NifTerm {
    unsafe { (funcs().make_list_cell)(env, car, cdr) }
}

/// Creates an ordinary list containing the `cnt` elements from the array. NIF 1.0 (OTP R13B04). Wraps `enif_make_list_from_array`.
pub(crate) unsafe fn make_list_from_array(
    env: *mut NifEnv, arr: *const NifTerm, cnt: c_uint,
) -> NifTerm {
    unsafe { (funcs().make_list_from_array)(env, arr, cnt) }
}

/// Sets `*list` to the reverse of the input list, returning non-zero on success or 0 if not a list. NIF 2.3 (OTP R15A). Wraps `enif_make_reverse_list`.
pub(crate) unsafe fn make_reverse_list(
    env: *mut NifEnv, term: NifTerm, list: *mut NifTerm,
) -> c_int {
    unsafe { (funcs().make_reverse_list)(env, term, list) }
}

// Macro equivalents: enif_make_list1..9 → make_list_from_array

/// Creates an ordinary list term with 1 element. Convenience wrapper around `make_list_from_array`.
pub(crate) unsafe fn make_list1(env: *mut NifEnv, e1: NifTerm) -> NifTerm {
    let arr = [e1];
    unsafe { make_list_from_array(env, arr.as_ptr(), 1) }
}

/// Creates an ordinary list term with 2 elements. Convenience wrapper around `make_list_from_array`.
pub(crate) unsafe fn make_list2(env: *mut NifEnv, e1: NifTerm, e2: NifTerm) -> NifTerm {
    let arr = [e1, e2];
    unsafe { make_list_from_array(env, arr.as_ptr(), 2) }
}

/// Creates an ordinary list term with 3 elements. Convenience wrapper around `make_list_from_array`.
pub(crate) unsafe fn make_list3(
    env: *mut NifEnv, e1: NifTerm, e2: NifTerm, e3: NifTerm,
) -> NifTerm {
    let arr = [e1, e2, e3];
    unsafe { make_list_from_array(env, arr.as_ptr(), 3) }
}

/// Creates an ordinary list term with 4 elements. Convenience wrapper around `make_list_from_array`.
pub(crate) unsafe fn make_list4(
    env: *mut NifEnv, e1: NifTerm, e2: NifTerm, e3: NifTerm, e4: NifTerm,
) -> NifTerm {
    let arr = [e1, e2, e3, e4];
    unsafe { make_list_from_array(env, arr.as_ptr(), 4) }
}

/// Creates an ordinary list term with 5 elements. Convenience wrapper around `make_list_from_array`.
pub(crate) unsafe fn make_list5(
    env: *mut NifEnv, e1: NifTerm, e2: NifTerm, e3: NifTerm, e4: NifTerm,
    e5: NifTerm,
) -> NifTerm {
    let arr = [e1, e2, e3, e4, e5];
    unsafe { make_list_from_array(env, arr.as_ptr(), 5) }
}

/// Creates an ordinary list term with 6 elements. Convenience wrapper around `make_list_from_array`.
pub(crate) unsafe fn make_list6(
    env: *mut NifEnv, e1: NifTerm, e2: NifTerm, e3: NifTerm, e4: NifTerm,
    e5: NifTerm, e6: NifTerm,
) -> NifTerm {
    let arr = [e1, e2, e3, e4, e5, e6];
    unsafe { make_list_from_array(env, arr.as_ptr(), 6) }
}

/// Creates an ordinary list term with 7 elements. Convenience wrapper around `make_list_from_array`.
pub(crate) unsafe fn make_list7(
    env: *mut NifEnv, e1: NifTerm, e2: NifTerm, e3: NifTerm, e4: NifTerm,
    e5: NifTerm, e6: NifTerm, e7: NifTerm,
) -> NifTerm {
    let arr = [e1, e2, e3, e4, e5, e6, e7];
    unsafe { make_list_from_array(env, arr.as_ptr(), 7) }
}

/// Creates an ordinary list term with 8 elements. Convenience wrapper around `make_list_from_array`.
pub(crate) unsafe fn make_list8(
    env: *mut NifEnv, e1: NifTerm, e2: NifTerm, e3: NifTerm, e4: NifTerm,
    e5: NifTerm, e6: NifTerm, e7: NifTerm, e8: NifTerm,
) -> NifTerm {
    let arr = [e1, e2, e3, e4, e5, e6, e7, e8];
    unsafe { make_list_from_array(env, arr.as_ptr(), 8) }
}

/// Creates an ordinary list term with 9 elements. Convenience wrapper around `make_list_from_array`.
pub(crate) unsafe fn make_list9(
    env: *mut NifEnv, e1: NifTerm, e2: NifTerm, e3: NifTerm, e4: NifTerm,
    e5: NifTerm, e6: NifTerm, e7: NifTerm, e8: NifTerm, e9: NifTerm,
) -> NifTerm {
    let arr = [e1, e2, e3, e4, e5, e6, e7, e8, e9];
    unsafe { make_list_from_array(env, arr.as_ptr(), 9) }
}

// -- Tuple ----------------------------------------------------------------

/// Gets the elements of a tuple as a read-only array, returning non-zero on success or 0 if not a tuple. NIF 0.1 (OTP R13B03). Wraps `enif_get_tuple`.
pub(crate) unsafe fn get_tuple(
    env: *mut NifEnv, tpl: NifTerm, arity: *mut c_int, array: *mut *const NifTerm,
) -> c_int {
    unsafe { (funcs().get_tuple)(env, tpl, arity, array) }
}

/// Creates a tuple containing the `cnt` elements from the array. NIF 1.0 (OTP R13B04). Wraps `enif_make_tuple_from_array`.
pub(crate) unsafe fn make_tuple_from_array(
    env: *mut NifEnv, arr: *const NifTerm, cnt: c_uint,
) -> NifTerm {
    unsafe { (funcs().make_tuple_from_array)(env, arr, cnt) }
}

// Macro equivalents: enif_make_tuple1..9 → make_tuple_from_array

/// Creates a tuple term with 1 element. Convenience wrapper around `make_tuple_from_array`.
pub(crate) unsafe fn make_tuple1(env: *mut NifEnv, e1: NifTerm) -> NifTerm {
    let arr = [e1];
    unsafe { make_tuple_from_array(env, arr.as_ptr(), 1) }
}

/// Creates a tuple term with 2 elements. Convenience wrapper around `make_tuple_from_array`.
pub(crate) unsafe fn make_tuple2(env: *mut NifEnv, e1: NifTerm, e2: NifTerm) -> NifTerm {
    let arr = [e1, e2];
    unsafe { make_tuple_from_array(env, arr.as_ptr(), 2) }
}

/// Creates a tuple term with 3 elements. Convenience wrapper around `make_tuple_from_array`.
pub(crate) unsafe fn make_tuple3(
    env: *mut NifEnv, e1: NifTerm, e2: NifTerm, e3: NifTerm,
) -> NifTerm {
    let arr = [e1, e2, e3];
    unsafe { make_tuple_from_array(env, arr.as_ptr(), 3) }
}

/// Creates a tuple term with 4 elements. Convenience wrapper around `make_tuple_from_array`.
pub(crate) unsafe fn make_tuple4(
    env: *mut NifEnv, e1: NifTerm, e2: NifTerm, e3: NifTerm, e4: NifTerm,
) -> NifTerm {
    let arr = [e1, e2, e3, e4];
    unsafe { make_tuple_from_array(env, arr.as_ptr(), 4) }
}

/// Creates a tuple term with 5 elements. Convenience wrapper around `make_tuple_from_array`.
pub(crate) unsafe fn make_tuple5(
    env: *mut NifEnv, e1: NifTerm, e2: NifTerm, e3: NifTerm, e4: NifTerm,
    e5: NifTerm,
) -> NifTerm {
    let arr = [e1, e2, e3, e4, e5];
    unsafe { make_tuple_from_array(env, arr.as_ptr(), 5) }
}

/// Creates a tuple term with 6 elements. Convenience wrapper around `make_tuple_from_array`.
pub(crate) unsafe fn make_tuple6(
    env: *mut NifEnv, e1: NifTerm, e2: NifTerm, e3: NifTerm, e4: NifTerm,
    e5: NifTerm, e6: NifTerm,
) -> NifTerm {
    let arr = [e1, e2, e3, e4, e5, e6];
    unsafe { make_tuple_from_array(env, arr.as_ptr(), 6) }
}

/// Creates a tuple term with 7 elements. Convenience wrapper around `make_tuple_from_array`.
pub(crate) unsafe fn make_tuple7(
    env: *mut NifEnv, e1: NifTerm, e2: NifTerm, e3: NifTerm, e4: NifTerm,
    e5: NifTerm, e6: NifTerm, e7: NifTerm,
) -> NifTerm {
    let arr = [e1, e2, e3, e4, e5, e6, e7];
    unsafe { make_tuple_from_array(env, arr.as_ptr(), 7) }
}

/// Creates a tuple term with 8 elements. Convenience wrapper around `make_tuple_from_array`.
pub(crate) unsafe fn make_tuple8(
    env: *mut NifEnv, e1: NifTerm, e2: NifTerm, e3: NifTerm, e4: NifTerm,
    e5: NifTerm, e6: NifTerm, e7: NifTerm, e8: NifTerm,
) -> NifTerm {
    let arr = [e1, e2, e3, e4, e5, e6, e7, e8];
    unsafe { make_tuple_from_array(env, arr.as_ptr(), 8) }
}

/// Creates a tuple term with 9 elements. Convenience wrapper around `make_tuple_from_array`.
pub(crate) unsafe fn make_tuple9(
    env: *mut NifEnv, e1: NifTerm, e2: NifTerm, e3: NifTerm, e4: NifTerm,
    e5: NifTerm, e6: NifTerm, e7: NifTerm, e8: NifTerm, e9: NifTerm,
) -> NifTerm {
    let arr = [e1, e2, e3, e4, e5, e6, e7, e8, e9];
    unsafe { make_tuple_from_array(env, arr.as_ptr(), 9) }
}

// -- String ---------------------------------------------------------------

/// Creates a list containing the characters of a NUL-terminated string with the given encoding. NIF 0.1 (OTP R13B03). Wraps `enif_make_string`.
pub(crate) unsafe fn make_string(
    env: *mut NifEnv, string: *const c_char, encoding: NifCharEncoding,
) -> NifTerm {
    unsafe { (funcs().make_string)(env, string, encoding) }
}

/// Creates a list containing the characters of a string with the given length and encoding. NIF 2.0 (OTP R14B). Wraps `enif_make_string_len`.
pub(crate) unsafe fn make_string_len(
    env: *mut NifEnv, string: *const c_char, len: usize, encoding: NifCharEncoding,
) -> NifTerm {
    unsafe { (funcs().make_string_len)(env, string, len, encoding) }
}

/// Writes a NUL-terminated string into `buf` from a list of characters with the given encoding. NIF 1.0 (OTP R13B04). Wraps `enif_get_string`.
pub(crate) unsafe fn get_string(
    env: *mut NifEnv, list: NifTerm, buf: *mut c_char, len: c_uint,
    encoding: NifCharEncoding,
) -> c_int {
    unsafe { (funcs().get_string)(env, list, buf, len, encoding) }
}

// -- Map ------------------------------------------------------------------

/// Makes an empty map term. NIF 2.6 (OTP 17.0). Wraps `enif_make_new_map`.
pub(crate) unsafe fn make_new_map(env: *mut NifEnv) -> NifTerm {
    unsafe { (funcs().make_new_map)(env) }
}

/// Sets `*size` to the number of key-value pairs in the map, returning non-zero on success. NIF 2.6 (OTP 17.0). Wraps `enif_get_map_size`.
pub(crate) unsafe fn get_map_size(
    env: *mut NifEnv, term: NifTerm, size: *mut usize,
) -> c_int {
    unsafe { (funcs().get_map_size)(env, term, size) }
}

/// Sets `*value` to the value associated with `key` in the map, returning non-zero on success. NIF 2.6 (OTP 17.0). Wraps `enif_get_map_value`.
pub(crate) unsafe fn get_map_value(
    env: *mut NifEnv, map: NifTerm, key: NifTerm, value: *mut NifTerm,
) -> c_int {
    unsafe { (funcs().get_map_value)(env, map, key, value) }
}

/// Makes a copy of a map with the key-value pair inserted or replaced, returning non-zero on success. NIF 2.6 (OTP 17.0). Wraps `enif_make_map_put`.
pub(crate) unsafe fn make_map_put(
    env: *mut NifEnv, map_in: NifTerm, key: NifTerm, value: NifTerm,
    map_out: *mut NifTerm,
) -> c_int {
    unsafe { (funcs().make_map_put)(env, map_in, key, value, map_out) }
}

/// Makes a copy of a map with an existing key's value replaced, failing if the key does not exist. NIF 2.6 (OTP 17.0). Wraps `enif_make_map_update`.
pub(crate) unsafe fn make_map_update(
    env: *mut NifEnv, map_in: NifTerm, key: NifTerm, value: NifTerm,
    map_out: *mut NifTerm,
) -> c_int {
    unsafe { (funcs().make_map_update)(env, map_in, key, value, map_out) }
}

/// Makes a copy of a map with a key-value pair removed. NIF 2.6 (OTP 17.0). Wraps `enif_make_map_remove`.
pub(crate) unsafe fn make_map_remove(
    env: *mut NifEnv, map_in: NifTerm, key: NifTerm, map_out: *mut NifTerm,
) -> c_int {
    unsafe { (funcs().make_map_remove)(env, map_in, key, map_out) }
}

/// Creates an iterator for a map, positioned at first or last entry. NIF 2.6 (OTP 17.0). Wraps `enif_map_iterator_create`.
pub(crate) unsafe fn map_iterator_create(
    env: *mut NifEnv, map: NifTerm, iter: *mut NifMapIterator,
    entry: NifMapIteratorEntry,
) -> c_int {
    unsafe { (funcs().map_iterator_create)(env, map, iter, entry) }
}

/// Destroys a map iterator created by `map_iterator_create`. NIF 2.6 (OTP 17.0). Wraps `enif_map_iterator_destroy`.
pub(crate) unsafe fn map_iterator_destroy(env: *mut NifEnv, iter: *mut NifMapIterator) {
    unsafe { (funcs().map_iterator_destroy)(env, iter) }
}

/// Returns non-zero if the map iterator is positioned before the first entry. NIF 2.6 (OTP 17.0). Wraps `enif_map_iterator_is_head`.
pub(crate) unsafe fn map_iterator_is_head(
    env: *mut NifEnv, iter: *mut NifMapIterator,
) -> c_int {
    unsafe { (funcs().map_iterator_is_head)(env, iter) }
}

/// Returns non-zero if the map iterator is positioned after the last entry. NIF 2.6 (OTP 17.0). Wraps `enif_map_iterator_is_tail`.
pub(crate) unsafe fn map_iterator_is_tail(
    env: *mut NifEnv, iter: *mut NifMapIterator,
) -> c_int {
    unsafe { (funcs().map_iterator_is_tail)(env, iter) }
}

/// Increments the map iterator to point to the next key-value entry. NIF 2.6 (OTP 17.0). Wraps `enif_map_iterator_next`.
pub(crate) unsafe fn map_iterator_next(
    env: *mut NifEnv, iter: *mut NifMapIterator,
) -> c_int {
    unsafe { (funcs().map_iterator_next)(env, iter) }
}

/// Decrements the map iterator to point to the previous key-value entry. NIF 2.6 (OTP 17.0). Wraps `enif_map_iterator_prev`.
pub(crate) unsafe fn map_iterator_prev(
    env: *mut NifEnv, iter: *mut NifMapIterator,
) -> c_int {
    unsafe { (funcs().map_iterator_prev)(env, iter) }
}

/// Gets the key and value terms at the current map iterator position. NIF 2.6 (OTP 17.0). Wraps `enif_map_iterator_get_pair`.
pub(crate) unsafe fn map_iterator_get_pair(
    env: *mut NifEnv, iter: *mut NifMapIterator, key: *mut NifTerm,
    value: *mut NifTerm,
) -> c_int {
    unsafe { (funcs().map_iterator_get_pair)(env, iter, key, value) }
}

/// Makes a map term from parallel arrays of keys and values with `cnt` pairs. NIF 2.14 (OTP 21.0). Wraps `enif_make_map_from_arrays`.
pub(crate) unsafe fn make_map_from_arrays(
    env: *mut NifEnv, keys: *const NifTerm, values: *const NifTerm, cnt: usize,
    map_out: *mut NifTerm,
) -> c_int {
    unsafe { (funcs().make_map_from_arrays)(env, keys, values, cnt, map_out) }
}

// -- Ref / Unique integer -------------------------------------------------

/// Creates a reference like `erlang:make_ref/0`. NIF 0.1 (OTP R13B03). Wraps `enif_make_ref`.
pub(crate) unsafe fn make_ref(env: *mut NifEnv) -> NifTerm {
    unsafe { (funcs().make_ref)(env) }
}

/// Returns a unique integer with the same properties as `erlang:unique_integer/1`. NIF 2.11 (OTP 19.0). Wraps `enif_make_unique_integer`.
pub(crate) unsafe fn make_unique_integer(
    env: *mut NifEnv, properties: NifUniqueInteger,
) -> NifTerm {
    unsafe { (funcs().make_unique_integer)(env, properties) }
}

// -- Pid ------------------------------------------------------------------

/// Extracts the pid term from an `NifPid` struct. Macro equivalent of `enif_make_pid`.
// Implementation note: enif_make_pid is a C macro that reads pid->pid directly.
pub(crate) unsafe fn make_pid(_env: *mut NifEnv, pid: *const NifPid) -> NifTerm {
    unsafe { (*pid).pid }
}

/// Initializes `*pid` to represent the calling process, returning the pointer on success or NULL if not process-bound. NIF 2.0 (OTP R14B). Wraps `enif_self`.
pub(crate) unsafe fn self_pid(env: *mut NifEnv, pid: *mut NifPid) -> *mut NifPid {
    unsafe { (funcs().self_pid)(env, pid) }
}

/// Extracts a node-local pid from a term, returning non-zero on success. NIF 2.0 (OTP R14B). Wraps `enif_get_local_pid`.
pub(crate) unsafe fn get_local_pid(
    env: *mut NifEnv, term: NifTerm, pid: *mut NifPid,
) -> c_int {
    unsafe { (funcs().get_local_pid)(env, term, pid) }
}

/// Returns non-zero if the process identified by `*pid` is alive. NIF 2.11 (OTP 19.0). Wraps `enif_is_process_alive`.
pub(crate) unsafe fn is_process_alive(env: *mut NifEnv, pid: *mut NifPid) -> c_int {
    unsafe { (funcs().is_process_alive)(env, pid) }
}

/// Returns non-zero if the currently executing process is alive. NIF 2.11 (OTP 19.0). Wraps `enif_is_current_process_alive`.
pub(crate) unsafe fn is_current_process_alive(env: *mut NifEnv) -> c_int {
    unsafe { (funcs().is_current_process_alive)(env) }
}

/// Looks up a process by its registered name atom, returning non-zero on success. NIF 2.12 (OTP 20.0). Wraps `enif_whereis_pid`.
pub(crate) unsafe fn whereis_pid(
    env: *mut NifEnv, name: NifTerm, pid: *mut NifPid,
) -> c_int {
    unsafe { (funcs().whereis_pid)(env, name, pid) }
}

// -- Port -----------------------------------------------------------------

/// Extracts a node-local port from a term, returning non-zero on success. NIF 2.11 (OTP 19.0). Wraps `enif_get_local_port`.
pub(crate) unsafe fn get_local_port(
    env: *mut NifEnv, term: NifTerm, port: *mut NifPort,
) -> c_int {
    unsafe { (funcs().get_local_port)(env, term, port) }
}

/// Returns non-zero if the given port is alive. NIF 2.11 (OTP 19.0). Wraps `enif_is_port_alive`.
pub(crate) unsafe fn is_port_alive(env: *mut NifEnv, port: *mut NifPort) -> c_int {
    unsafe { (funcs().is_port_alive)(env, port) }
}

/// Looks up a port by its registered name atom, returning non-zero on success. NIF 2.12 (OTP 20.0). Wraps `enif_whereis_port`.
pub(crate) unsafe fn whereis_port(
    env: *mut NifEnv, name: NifTerm, port: *mut NifPort,
) -> c_int {
    unsafe { (funcs().whereis_port)(env, name, port) }
}

/// Sends a message to a port asynchronously, like `erlang:port_command/2`. NIF 2.11 (OTP 19.0). Wraps `enif_port_command`.
pub(crate) unsafe fn port_command(
    env: *mut NifEnv, to_port: *const NifPort, msg_env: *mut NifEnv, msg: NifTerm,
) -> c_int {
    unsafe { (funcs().port_command)(env, to_port, msg_env, msg) }
}

// -- Env / send -----------------------------------------------------------

/// Allocates a new process-independent environment for holding terms not bound to any process. NIF 2.0 (OTP R14B). Wraps `enif_alloc_env`.
pub(crate) unsafe fn alloc_env() -> *mut NifEnv {
    unsafe { (funcs().alloc_env)() }
}

/// Frees an environment allocated with `alloc_env` and all terms created in it. NIF 2.0 (OTP R14B). Wraps `enif_free_env`.
pub(crate) unsafe fn free_env(env: *mut NifEnv) {
    unsafe { (funcs().free_env)(env) }
}

/// Frees all terms in an environment and clears it for reuse. NIF 2.0 (OTP R14B). Wraps `enif_clear_env`.
pub(crate) unsafe fn clear_env(env: *mut NifEnv) {
    unsafe { (funcs().clear_env)(env) }
}

/// Sends a message to a process; `msg_env` is invalidated on success. NIF 2.0 (OTP R14B). Wraps `enif_send`.
pub(crate) unsafe fn send(
    env: *mut NifEnv, to_pid: *const NifPid, msg_env: *mut NifEnv, msg: NifTerm,
) -> c_int {
    unsafe { (funcs().send)(env, to_pid, msg_env, msg) }
}

/// Makes a copy of a term into a destination environment. NIF 2.0 (OTP R14B). Wraps `enif_make_copy`.
pub(crate) unsafe fn make_copy(dst_env: *mut NifEnv, src_term: NifTerm) -> NifTerm {
    unsafe { (funcs().make_copy)(dst_env, src_term) }
}

// -- Resource -------------------------------------------------------------

/// Opens or takes over a resource type for managing resource objects with an optional destructor. NIF 1.0 (OTP R13B04). Wraps `enif_open_resource_type`.
pub(crate) unsafe fn open_resource_type(
    env: *mut NifEnv, module_str: *const c_char, name_str: *const c_char,
    dtor: Option<unsafe extern "C" fn(*mut NifEnv, *mut c_void)>,
    flags: NifResourceFlags, tried: *mut NifResourceFlags,
) -> *mut NifResourceType {
    unsafe { (funcs().open_resource_type)(env, module_str, name_str, dtor, flags, tried) }
}

/// Opens or takes over a resource type with extended callbacks (select stop, down). NIF 2.12 (OTP 20.0). Wraps `enif_open_resource_type_x`.
pub(crate) unsafe fn open_resource_type_x(
    env: *mut NifEnv, name_str: *const c_char, init: *const NifResourceTypeInit,
    flags: NifResourceFlags, tried: *mut NifResourceFlags,
) -> *mut NifResourceType {
    unsafe { (funcs().open_resource_type_x)(env, name_str, init, flags, tried) }
}

/// Allocates a memory-managed resource object of the given type and size. NIF 1.0 (OTP R13B04). Wraps `enif_alloc_resource`.
pub(crate) unsafe fn alloc_resource(
    rtype: *mut NifResourceType, size: usize,
) -> *mut c_void {
    unsafe { (funcs().alloc_resource)(rtype, size) }
}

/// Removes a reference to a resource object; the resource is destructed when the last reference is removed. NIF 1.0 (OTP R13B04). Wraps `enif_release_resource`.
pub(crate) unsafe fn release_resource(obj: *mut c_void) {
    unsafe { (funcs().release_resource)(obj) }
}

/// Creates an opaque handle term to a memory-managed resource object. NIF 1.0 (OTP R13B04). Wraps `enif_make_resource`.
pub(crate) unsafe fn make_resource(env: *mut NifEnv, obj: *mut c_void) -> NifTerm {
    unsafe { (funcs().make_resource)(env, obj) }
}

/// Retrieves a pointer to the resource object referred to by a resource term. NIF 1.0 (OTP R13B04). Wraps `enif_get_resource`.
pub(crate) unsafe fn get_resource(
    env: *mut NifEnv, term: NifTerm, rtype: *mut NifResourceType,
    objp: *mut *mut c_void,
) -> c_int {
    unsafe { (funcs().get_resource)(env, term, rtype, objp) }
}

/// Gets the byte size of a resource object obtained from `alloc_resource`. NIF 1.0 (OTP R13B04). Wraps `enif_sizeof_resource`.
pub(crate) unsafe fn sizeof_resource(obj: *mut c_void) -> usize {
    unsafe { (funcs().sizeof_resource)(obj) }
}

/// Adds a reference to a resource object, which must be balanced with `release_resource`. NIF 2.0 (OTP R14B). Wraps `enif_keep_resource`.
pub(crate) unsafe fn keep_resource(obj: *mut c_void) {
    unsafe { (funcs().keep_resource)(obj) }
}

// -- Exception ------------------------------------------------------------

/// Creates a badarg exception to be returned from a NIF, signaling an invalid argument. NIF 0.1 (OTP R13B03). Wraps `enif_make_badarg`.
pub(crate) unsafe fn make_badarg(env: *mut NifEnv) -> NifTerm {
    unsafe { (funcs().make_badarg)(env) }
}

/// Returns non-zero if a pending exception is associated with the environment; optionally stores the reason in `*reason`. NIF 2.8 (OTP 18.0). Wraps `enif_has_pending_exception`.
pub(crate) unsafe fn has_pending_exception(
    env: *mut NifEnv, reason: *mut NifTerm,
) -> c_int {
    unsafe { (funcs().has_pending_exception)(env, reason) }
}

/// Creates an error exception with the given reason term to be returned from a NIF. NIF 2.8 (OTP 18.0). Wraps `enif_raise_exception`.
pub(crate) unsafe fn raise_exception(env: *mut NifEnv, reason: NifTerm) -> NifTerm {
    unsafe { (funcs().raise_exception)(env, reason) }
}

// -- Schedule -------------------------------------------------------------

/// Schedules a NIF function for execution, allowing long-running work to be broken into chunks. NIF 2.7 (OTP 17.3). Wraps `enif_schedule_nif`.
pub(crate) unsafe fn schedule_nif(
    env: *mut NifEnv, fun_name: *const c_char, flags: c_int,
    fp: unsafe extern "C" fn(*mut NifEnv, c_int, *const NifTerm) -> NifTerm,
    argc: c_int, argv: *const NifTerm,
) -> NifTerm {
    unsafe { (funcs().schedule_nif)(env, fun_name, flags, fp, argc, argv) }
}

// -- Monitor --------------------------------------------------------------

/// Starts monitoring a process from a resource; a process exit triggers the `down` callback. NIF 2.12 (OTP 20.0). Wraps `enif_monitor_process`.
pub(crate) unsafe fn monitor_process(
    env: *mut NifEnv, obj: *mut c_void, pid: *const NifPid, monitor: *mut NifMonitor,
) -> c_int {
    unsafe { (funcs().monitor_process)(env, obj, pid, monitor) }
}

/// Cancels a monitor created with `monitor_process`. Returns 0 on success. NIF 2.12 (OTP 20.0). Wraps `enif_demonitor_process`.
pub(crate) unsafe fn demonitor_process(
    env: *mut NifEnv, obj: *mut c_void, monitor: *const NifMonitor,
) -> c_int {
    unsafe { (funcs().demonitor_process)(env, obj, monitor) }
}

/// Compares two monitors: returns 0 if equal, <0 if mon1 < mon2, >0 if mon1 > mon2. NIF 2.12 (OTP 20.0). Wraps `enif_compare_monitors`.
pub(crate) unsafe fn compare_monitors(
    mon1: *const NifMonitor, mon2: *const NifMonitor,
) -> c_int {
    unsafe { (funcs().compare_monitors)(mon1, mon2) }
}

// -- Select ---------------------------------------------------------------

/// Registers for asynchronous notifications when an OS event object becomes ready for read or write. NIF 2.12 (OTP 20.0). Wraps `enif_select`.
pub(crate) unsafe fn select(
    env: *mut NifEnv, e: NifEvent, flags: NifSelectFlags, obj: *mut c_void,
    pid: *const NifPid, ref_term: NifTerm,
) -> c_int {
    unsafe { (funcs().select)(env, e, flags, obj, pid, ref_term) }
}

// -- Time -----------------------------------------------------------------

/// Returns the current Erlang monotonic time in the given time unit; may be negative. NIF 2.10 (OTP 18.3). Wraps `enif_monotonic_time`.
pub(crate) unsafe fn monotonic_time(unit: NifTimeUnit) -> NifTime {
    unsafe { (funcs().monotonic_time)(unit) }
}

/// Returns the current time offset between Erlang monotonic time and Erlang system time. NIF 2.10 (OTP 18.3). Wraps `enif_time_offset`.
pub(crate) unsafe fn time_offset(unit: NifTimeUnit) -> NifTime {
    unsafe { (funcs().time_offset)(unit) }
}

/// Converts a time value from one time unit to another. NIF 2.10 (OTP 18.3). Wraps `enif_convert_time_unit`.
pub(crate) unsafe fn convert_time_unit(
    time: NifTime, from: NifTimeUnit, to: NifTimeUnit,
) -> NifTime {
    unsafe { (funcs().convert_time_unit)(time, from, to) }
}

/// Returns wall-clock time as an integer term. Deprecated in favor of `monotonic_time`. NIF 2.11 (OTP 19.0). Wraps `enif_now_time`.
pub(crate) unsafe fn now_time(env: *mut NifEnv) -> NifTerm {
    unsafe { (funcs().now_time)(env) }
}

/// Returns CPU time as an integer term. NIF 2.11 (OTP 19.0). Wraps `enif_cpu_time`.
pub(crate) unsafe fn cpu_time(env: *mut NifEnv) -> NifTerm {
    unsafe { (funcs().cpu_time)(env) }
}

// -- Hash -----------------------------------------------------------------

/// Hashes a term using the specified hash type and salt. NIF 2.12 (OTP 20.0). Wraps `enif_hash`.
pub(crate) unsafe fn hash(hash_type: NifHash, term: NifTerm, salt: u64) -> u64 {
    unsafe { (funcs().hash)(hash_type, term, salt) }
}

// -- Term serialization ---------------------------------------------------

/// Serializes a term into the Erlang external term format, allocating the result binary. NIF 2.11 (OTP 19.0). Wraps `enif_term_to_binary`.
pub(crate) unsafe fn term_to_binary(
    env: *mut NifEnv, term: NifTerm, bin: *mut NifBinary,
) -> c_int {
    unsafe { (funcs().term_to_binary)(env, term, bin) }
}

/// Deserializes a term from Erlang external term format, returning the number of bytes read. NIF 2.11 (OTP 19.0). Wraps `enif_binary_to_term`.
pub(crate) unsafe fn binary_to_term(
    env: *mut NifEnv, data: *const u8, sz: usize, term: *mut NifTerm, opts: c_uint,
) -> usize {
    unsafe { (funcs().binary_to_term)(env, data, sz, term, opts) }
}

// -- Timeslice ------------------------------------------------------------

/// Reports consumption of a timeslice (1-100 percent); returns non-zero if the timeslice is exhausted. NIF 2.4 (OTP R16B). Wraps `enif_consume_timeslice`.
pub(crate) unsafe fn consume_timeslice(env: *mut NifEnv, percent: c_int) -> c_int {
    unsafe { (funcs().consume_timeslice)(env, percent) }
}

// -- System ---------------------------------------------------------------

/// Fills a `NifSysInfo` struct with runtime system information. NIF 1.0 (OTP R13B04). Wraps `enif_system_info`.
pub(crate) unsafe fn system_info(sip: *mut NifSysInfo, si_size: usize) {
    unsafe { (funcs().system_info)(sip, si_size) }
}

/// Gets the value of an environment variable, returning 0 on success. NIF 2.9 (OTP 18.2). Wraps `enif_getenv`.
pub(crate) unsafe fn getenv(
    key: *const c_char, value: *mut c_char, value_size: *mut usize,
) -> c_int {
    unsafe { (funcs().getenv)(key, value, value_size) }
}

/// Returns the type of the current thread: 1 for scheduler, 2 for dirty CPU, 3 for dirty I/O, -1 for non-ERTS. NIF 2.11 (OTP 19.0). Wraps `enif_thread_type`.
pub(crate) unsafe fn thread_type() -> c_int {
    unsafe { (funcs().thread_type)() }
}

// -- Dynamic loading ------------------------------------------------------

/// Opens a dynamically linked shared library, calling `err_handler` on failure. NIF 2.4 (OTP R16B). Wraps `enif_dlopen`.
pub(crate) unsafe fn dlopen(
    lib: *const c_char,
    err_handler: Option<unsafe extern "C" fn(*mut c_void, *const c_char)>,
    err_arg: *mut c_void,
) -> *mut c_void {
    unsafe { (funcs().dlopen)(lib, err_handler, err_arg) }
}

/// Looks up a symbol in a dynamically linked library opened with `dlopen`. NIF 2.4 (OTP R16B). Wraps `enif_dlsym`.
pub(crate) unsafe fn dlsym(
    handle: *mut c_void, symbol: *const c_char,
    err_handler: Option<unsafe extern "C" fn(*mut c_void, *const c_char)>,
    err_arg: *mut c_void,
) -> *mut c_void {
    unsafe { (funcs().dlsym)(handle, symbol, err_handler, err_arg) }
}

// -- Threading ------------------------------------------------------------

/// Creates a mutex with the given name. NIF 1.0 (OTP R13B04). Wraps `enif_mutex_create`.
pub(crate) unsafe fn mutex_create(name: *mut c_char) -> *mut NifMutex {
    unsafe { (funcs().mutex_create)(name) }
}

/// Destroys a mutex. NIF 1.0 (OTP R13B04). Wraps `enif_mutex_destroy`.
pub(crate) unsafe fn mutex_destroy(mtx: *mut NifMutex) {
    unsafe { (funcs().mutex_destroy)(mtx) }
}

/// Tries to lock a mutex without blocking; returns 0 on success. NIF 1.0 (OTP R13B04). Wraps `enif_mutex_trylock`.
pub(crate) unsafe fn mutex_trylock(mtx: *mut NifMutex) -> c_int {
    unsafe { (funcs().mutex_trylock)(mtx) }
}

/// Locks a mutex, blocking until it becomes available. NIF 1.0 (OTP R13B04). Wraps `enif_mutex_lock`.
pub(crate) unsafe fn mutex_lock(mtx: *mut NifMutex) {
    unsafe { (funcs().mutex_lock)(mtx) }
}

/// Unlocks a mutex. NIF 1.0 (OTP R13B04). Wraps `enif_mutex_unlock`.
pub(crate) unsafe fn mutex_unlock(mtx: *mut NifMutex) {
    unsafe { (funcs().mutex_unlock)(mtx) }
}

/// Creates a condition variable with the given name. NIF 1.0 (OTP R13B04). Wraps `enif_cond_create`.
pub(crate) unsafe fn cond_create(name: *mut c_char) -> *mut NifCond {
    unsafe { (funcs().cond_create)(name) }
}

/// Destroys a condition variable. NIF 1.0 (OTP R13B04). Wraps `enif_cond_destroy`.
pub(crate) unsafe fn cond_destroy(cnd: *mut NifCond) {
    unsafe { (funcs().cond_destroy)(cnd) }
}

/// Signals one thread waiting on a condition variable. NIF 1.0 (OTP R13B04). Wraps `enif_cond_signal`.
pub(crate) unsafe fn cond_signal(cnd: *mut NifCond) {
    unsafe { (funcs().cond_signal)(cnd) }
}

/// Wakes all threads waiting on a condition variable. NIF 1.0 (OTP R13B04). Wraps `enif_cond_broadcast`.
pub(crate) unsafe fn cond_broadcast(cnd: *mut NifCond) {
    unsafe { (funcs().cond_broadcast)(cnd) }
}

/// Atomically unlocks the mutex and waits on a condition variable. NIF 1.0 (OTP R13B04). Wraps `enif_cond_wait`.
pub(crate) unsafe fn cond_wait(cnd: *mut NifCond, mtx: *mut NifMutex) {
    unsafe { (funcs().cond_wait)(cnd, mtx) }
}

/// Creates a read-write lock with the given name. NIF 1.0 (OTP R13B04). Wraps `enif_rwlock_create`.
pub(crate) unsafe fn rwlock_create(name: *mut c_char) -> *mut NifRWLock {
    unsafe { (funcs().rwlock_create)(name) }
}

/// Destroys a read-write lock. NIF 1.0 (OTP R13B04). Wraps `enif_rwlock_destroy`.
pub(crate) unsafe fn rwlock_destroy(rwlck: *mut NifRWLock) {
    unsafe { (funcs().rwlock_destroy)(rwlck) }
}

/// Tries to acquire a read lock without blocking; returns 0 on success. NIF 1.0 (OTP R13B04). Wraps `enif_rwlock_tryrlock`.
pub(crate) unsafe fn rwlock_tryrlock(rwlck: *mut NifRWLock) -> c_int {
    unsafe { (funcs().rwlock_tryrlock)(rwlck) }
}

/// Acquires a read lock, blocking until available. NIF 1.0 (OTP R13B04). Wraps `enif_rwlock_rlock`.
pub(crate) unsafe fn rwlock_rlock(rwlck: *mut NifRWLock) {
    unsafe { (funcs().rwlock_rlock)(rwlck) }
}

/// Releases a read lock. NIF 1.0 (OTP R13B04). Wraps `enif_rwlock_runlock`.
pub(crate) unsafe fn rwlock_runlock(rwlck: *mut NifRWLock) {
    unsafe { (funcs().rwlock_runlock)(rwlck) }
}

/// Tries to acquire a read-write (exclusive) lock without blocking; returns 0 on success. NIF 1.0 (OTP R13B04). Wraps `enif_rwlock_tryrwlock`.
pub(crate) unsafe fn rwlock_tryrwlock(rwlck: *mut NifRWLock) -> c_int {
    unsafe { (funcs().rwlock_tryrwlock)(rwlck) }
}

/// Acquires a read-write (exclusive) lock, blocking until available. NIF 1.0 (OTP R13B04). Wraps `enif_rwlock_rwlock`.
pub(crate) unsafe fn rwlock_rwlock(rwlck: *mut NifRWLock) {
    unsafe { (funcs().rwlock_rwlock)(rwlck) }
}

/// Releases a read-write (exclusive) lock. NIF 1.0 (OTP R13B04). Wraps `enif_rwlock_rwunlock`.
pub(crate) unsafe fn rwlock_rwunlock(rwlck: *mut NifRWLock) {
    unsafe { (funcs().rwlock_rwunlock)(rwlck) }
}

/// Creates a thread-specific data key with the given name. NIF 1.0 (OTP R13B04). Wraps `enif_tsd_key_create`.
pub(crate) unsafe fn tsd_key_create(name: *mut c_char, key: *mut NifTSDKey) -> c_int {
    unsafe { (funcs().tsd_key_create)(name, key) }
}

/// Destroys a thread-specific data key. NIF 1.0 (OTP R13B04). Wraps `enif_tsd_key_destroy`.
pub(crate) unsafe fn tsd_key_destroy(key: NifTSDKey) {
    unsafe { (funcs().tsd_key_destroy)(key) }
}

/// Sets the thread-specific data for the given key. NIF 1.0 (OTP R13B04). Wraps `enif_tsd_set`.
pub(crate) unsafe fn tsd_set(key: NifTSDKey, data: *mut c_void) {
    unsafe { (funcs().tsd_set)(key, data) }
}

/// Gets the thread-specific data for the given key. NIF 1.0 (OTP R13B04). Wraps `enif_tsd_get`.
pub(crate) unsafe fn tsd_get(key: NifTSDKey) -> *mut c_void {
    unsafe { (funcs().tsd_get)(key) }
}

/// Allocates and initializes a thread options structure. NIF 1.0 (OTP R13B04). Wraps `enif_thread_opts_create`.
pub(crate) unsafe fn thread_opts_create(name: *mut c_char) -> *mut NifThreadOpts {
    unsafe { (funcs().thread_opts_create)(name) }
}

/// Destroys a thread options structure. NIF 1.0 (OTP R13B04). Wraps `enif_thread_opts_destroy`.
pub(crate) unsafe fn thread_opts_destroy(opts: *mut NifThreadOpts) {
    unsafe { (funcs().thread_opts_destroy)(opts) }
}

/// Creates a new thread with the given entry function and arguments. NIF 1.0 (OTP R13B04). Wraps `enif_thread_create`.
pub(crate) unsafe fn thread_create(
    name: *mut c_char, tid: *mut NifTid,
    func: Option<unsafe extern "C" fn(*mut c_void) -> *mut c_void>,
    args: *mut c_void, opts: *mut NifThreadOpts,
) -> c_int {
    unsafe { (funcs().thread_create)(name, tid, func, args, opts) }
}

/// Returns the thread identifier of the calling thread. NIF 1.0 (OTP R13B04). Wraps `enif_thread_self`.
pub(crate) unsafe fn thread_self() -> NifTid {
    unsafe { (funcs().thread_self)() }
}

/// Compares two thread identifiers; returns non-zero if they are equal. NIF 1.0 (OTP R13B04). Wraps `enif_equal_tids`.
pub(crate) unsafe fn equal_tids(tid1: NifTid, tid2: NifTid) -> c_int {
    unsafe { (funcs().equal_tids)(tid1, tid2) }
}

/// Terminates the calling thread with the given result. NIF 1.0 (OTP R13B04). Wraps `enif_thread_exit`.
pub(crate) unsafe fn thread_exit(resp: *mut c_void) {
    unsafe { (funcs().thread_exit)(resp) }
}

/// Waits for a thread to terminate and retrieves its result. NIF 1.0 (OTP R13B04). Wraps `enif_thread_join`.
pub(crate) unsafe fn thread_join(tid: NifTid, respp: *mut *mut c_void) -> c_int {
    unsafe { (funcs().thread_join)(tid, respp) }
}

// NIF 2.14

/// Returns the name of a mutex. NIF 2.14 (OTP 21.0). Wraps `enif_mutex_name`.
pub(crate) unsafe fn mutex_name(mtx: *mut NifMutex) -> *mut c_char {
    unsafe { (funcs().mutex_name)(mtx) }
}

/// Returns the name of a condition variable. NIF 2.14 (OTP 21.0). Wraps `enif_cond_name`.
pub(crate) unsafe fn cond_name(cnd: *mut NifCond) -> *mut c_char {
    unsafe { (funcs().cond_name)(cnd) }
}

/// Returns the name of a read-write lock. NIF 2.14 (OTP 21.0). Wraps `enif_rwlock_name`.
pub(crate) unsafe fn rwlock_name(rwlck: *mut NifRWLock) -> *mut c_char {
    unsafe { (funcs().rwlock_name)(rwlck) }
}

/// Returns the name of a thread. NIF 2.14 (OTP 21.0). Wraps `enif_thread_name`.
pub(crate) unsafe fn thread_name(tid: NifTid) -> *mut c_char {
    unsafe { (funcs().thread_name)(tid) }
}

// -- I/O Queue ------------------------------------------------------------
// NIF 2.13 (IOQ core), NIF 2.14 (ioq_peek_head)

/// Creates a new I/O queue; `opts` must be `ERL_NIF_IOQ_NORMAL`. NIF 2.12 (OTP 20.0). Wraps `enif_ioq_create`.
pub(crate) unsafe fn ioq_create(opts: NifIOQueueOpts) -> *mut NifIOQueue {
    unsafe { (funcs().ioq_create)(opts) }
}

/// Destroys an I/O queue and frees all of its contents. NIF 2.12 (OTP 20.0). Wraps `enif_ioq_destroy`.
pub(crate) unsafe fn ioq_destroy(q: *mut NifIOQueue) {
    unsafe { (funcs().ioq_destroy)(q) }
}

/// Enqueues a binary into the I/O queue, skipping the first `skip` bytes; ownership transfers to the queue. NIF 2.12 (OTP 20.0). Wraps `enif_ioq_enq_binary`.
pub(crate) unsafe fn ioq_enq_binary(
    q: *mut NifIOQueue, bin: *mut NifBinary, skip: usize,
) -> c_int {
    unsafe { (funcs().ioq_enq_binary)(q, bin, skip) }
}

/// Enqueues an iovec into the I/O queue, skipping the first `skip` bytes. NIF 2.12 (OTP 20.0). Wraps `enif_ioq_enqv`.
pub(crate) unsafe fn ioq_enqv(
    q: *mut NifIOQueue, iov: *mut NifIOVec, skip: usize,
) -> c_int {
    unsafe { (funcs().ioq_enqv)(q, iov, skip) }
}

/// Returns the total byte size of the I/O queue. NIF 2.12 (OTP 20.0). Wraps `enif_ioq_size`.
pub(crate) unsafe fn ioq_size(q: *mut NifIOQueue) -> usize {
    unsafe { (funcs().ioq_size)(q) }
}

/// Dequeues `count` bytes from the I/O queue; optionally stores the new size in `*size`. NIF 2.12 (OTP 20.0). Wraps `enif_ioq_deq`.
pub(crate) unsafe fn ioq_deq(
    q: *mut NifIOQueue, count: usize, size: *mut usize,
) -> c_int {
    unsafe { (funcs().ioq_deq)(q, count, size) }
}

/// Returns the I/O queue contents as a `SysIOVec` array suitable for `writev`. NIF 2.12 (OTP 20.0). Wraps `enif_ioq_peek`.
pub(crate) unsafe fn ioq_peek(
    q: *mut NifIOQueue, iovlen: *mut c_int,
) -> *mut SysIOVec {
    unsafe { (funcs().ioq_peek)(q, iovlen) }
}

/// Inspects an iolist or binary term as an iovec, processing up to `max_length` elements. NIF 2.12 (OTP 20.0). Wraps `enif_inspect_iovec`.
pub(crate) unsafe fn inspect_iovec(
    env: *mut NifEnv, max_length: usize, iovec_term: NifTerm,
    tail: *mut NifTerm, iovec: *mut *mut NifIOVec,
) -> c_int {
    unsafe { (funcs().inspect_iovec)(env, max_length, iovec_term, tail, iovec) }
}

/// Frees an iovec returned by `inspect_iovec`. NIF 2.12 (OTP 20.0). Wraps `enif_free_iovec`.
pub(crate) unsafe fn free_iovec(iov: *mut NifIOVec) {
    unsafe { (funcs().free_iovec)(iov) }
}

/// Gets the head of the I/O queue as a binary term, returning non-zero on success. NIF 2.14 (OTP 21.0). Wraps `enif_ioq_peek_head`.
pub(crate) unsafe fn ioq_peek_head(
    env: *mut NifEnv, q: *mut NifIOQueue, size: *mut usize, head: *mut NifTerm,
) -> c_int {
    unsafe { (funcs().ioq_peek_head)(env, q, size, head) }
}

// ===========================================================================
// NIF 2.15 (OTP 22)
// ===========================================================================

/// Extended select with custom message support. NIF 2.15 (OTP 22.0). Wraps `enif_select_x`.
pub(crate) unsafe fn select_x(
    env: *mut NifEnv, e: NifEvent, flags: NifSelectFlags, obj: *mut c_void,
    pid: *const NifPid, msg: NifTerm, msg_env: *mut NifEnv,
) -> c_int {
    unsafe { (funcs().select_x)(env, e, flags, obj, pid, msg, msg_env) }
}

/// Creates a term from a monitor for use in Erlang code. NIF 2.15 (OTP 22.0). Wraps `enif_make_monitor_term`.
pub(crate) unsafe fn make_monitor_term(
    env: *mut NifEnv, monitor: *const NifMonitor,
) -> NifTerm {
    unsafe { (funcs().make_monitor_term)(env, monitor) }
}

/// Sets a pid variable to undefined, for use as a sentinel value. NIF 2.15 (OTP 22.0). Wraps `enif_set_pid_undefined`.
pub(crate) unsafe fn set_pid_undefined(pid: *mut NifPid) {
    unsafe { (funcs().set_pid_undefined)(pid) }
}

/// Returns non-zero if the pid was set to undefined with `set_pid_undefined`. NIF 2.15 (OTP 22.0). Wraps `enif_is_pid_undefined`.
pub(crate) unsafe fn is_pid_undefined(pid: *const NifPid) -> c_int {
    unsafe { (funcs().is_pid_undefined)(pid) }
}

/// Returns the type of a term as a `NifTermType` enum value. NIF 2.15 (OTP 22.0). Wraps `enif_term_type`.
pub(crate) unsafe fn term_type(env: *mut NifEnv, term: NifTerm) -> NifTermType {
    unsafe { (funcs().term_type)(env, term) }
}

/// Compares two pids for ordering: returns 0 if equal, <0 if a < b, >0 if a > b. NIF 2.15 (OTP 22.0). Macro equivalent of `enif_compare_pids`.
// Implementation note: enif_compare_pids is a C macro that calls enif_compare on the pid terms.
pub(crate) unsafe fn compare_pids(a: *const NifPid, b: *const NifPid) -> c_int {
    unsafe { compare((*a).pid, (*b).pid) }
}

/// Registers for async read notifications with a custom message. NIF 2.15 (OTP 22.0). Macro equivalent of `enif_select_read`.
// Implementation note: calls select_x with SELECT_READ | SELECT_CUSTOM_MSG.
pub(crate) unsafe fn select_read(
    env: *mut NifEnv, e: NifEvent, obj: *mut c_void, pid: *const NifPid,
    msg: NifTerm, msg_env: *mut NifEnv,
) -> c_int {
    unsafe { select_x(env, e, NifSelectFlags::READ | NifSelectFlags::CUSTOM_MSG, obj, pid, msg, msg_env) }
}

/// Registers for async write notifications with a custom message. NIF 2.15 (OTP 22.0). Macro equivalent of `enif_select_write`.
// Implementation note: calls select_x with SELECT_WRITE | SELECT_CUSTOM_MSG.
pub(crate) unsafe fn select_write(
    env: *mut NifEnv, e: NifEvent, obj: *mut c_void, pid: *const NifPid,
    msg: NifTerm, msg_env: *mut NifEnv,
) -> c_int {
    unsafe { select_x(env, e, NifSelectFlags::WRITE | NifSelectFlags::CUSTOM_MSG, obj, pid, msg, msg_env) }
}

// ===========================================================================
// NIF 2.16 (OTP 24)
// ===========================================================================

/// Opens or takes over a resource type with versioned init struct. NIF 2.16 (OTP 24.0). Wraps `enif_init_resource_type`.
pub(crate) unsafe fn init_resource_type(
    env: *mut NifEnv, name_str: *const c_char, init: *const NifResourceTypeInit,
    flags: NifResourceFlags, tried: *mut NifResourceFlags,
) -> *mut NifResourceType {
    unsafe { (funcs().init_resource_type)(env, name_str, init, flags, tried) }
}

/// Calls a resource type's dynamic callback across NIF modules. NIF 2.16 (OTP 24.0). Wraps `enif_dynamic_resource_call`.
pub(crate) unsafe fn dynamic_resource_call(
    env: *mut NifEnv, mod_term: NifTerm, name_term: NifTerm, rsrc: NifTerm,
    call_data: *mut c_void,
) -> c_int {
    unsafe { (funcs().dynamic_resource_call)(env, mod_term, name_term, rsrc, call_data) }
}

/// Registers for async error notifications with a custom message. NIF 2.16 (OTP 24.0). Macro equivalent of `enif_select_error`.
// Implementation note: calls select_x with SELECT_ERROR | SELECT_CUSTOM_MSG.
pub(crate) unsafe fn select_error(
    env: *mut NifEnv, e: NifEvent, obj: *mut c_void, pid: *const NifPid,
    msg: NifTerm, msg_env: *mut NifEnv,
) -> c_int {
    unsafe { select_x(env, e, NifSelectFlags::ERROR | NifSelectFlags::CUSTOM_MSG, obj, pid, msg, msg_env) }
}

// ===========================================================================
// NIF 2.17 (OTP 26)
// ===========================================================================

/// Gets the length (in bytes) of a string list without extracting it. NIF 2.17 (OTP 26.0). Wraps `enif_get_string_length`.
pub(crate) unsafe fn get_string_length(
    env: *mut NifEnv, list: NifTerm, len: *mut c_uint, encoding: NifCharEncoding,
) -> c_int {
    unsafe { (funcs().get_string_length)(env, list, len, encoding) }
}

/// Creates an atom from a NUL-terminated string, failing if the atom does not already exist and the table is full. NIF 2.17 (OTP 26.0). Wraps `enif_make_new_atom`.
pub(crate) unsafe fn make_new_atom(
    env: *mut NifEnv, name: *const c_char, atom: *mut NifTerm,
    encoding: NifCharEncoding,
) -> c_int {
    unsafe { (funcs().make_new_atom)(env, name, atom, encoding) }
}

/// Creates an atom from a string with explicit length, failing if the atom does not already exist and the table is full. NIF 2.17 (OTP 26.0). Wraps `enif_make_new_atom_len`.
pub(crate) unsafe fn make_new_atom_len(
    env: *mut NifEnv, name: *const c_char, len: usize, atom: *mut NifTerm,
    encoding: NifCharEncoding,
) -> c_int {
    unsafe { (funcs().make_new_atom_len)(env, name, len, atom, encoding) }
}

/// Sets the `ERL_NIF_OPT_DELAY_HALT` option, preventing the runtime from halting until NIF cleanup completes. NIF 2.17 (OTP 26.0). Wraps `enif_set_option`.
// Implementation note: enif_set_option is variadic in C; this transmutes to the single-arg variant.
pub(crate) unsafe fn set_option_delay_halt(env: *mut NifEnv) -> c_int {
    let f: unsafe extern "C" fn(*mut NifEnv, NifOption) -> c_int =
        unsafe { std::mem::transmute(funcs().set_option) };
    unsafe { f(env, NifOption::DelayHalt) }
}

/// Registers a callback to be invoked when the runtime halts. NIF 2.17 (OTP 26.0). Wraps `enif_set_option`.
// Implementation note: enif_set_option is variadic in C; this transmutes to the two-arg variant (option + callback).
pub(crate) unsafe fn set_option_on_halt(
    env: *mut NifEnv,
    callback: unsafe extern "C" fn(*mut c_void),
) -> c_int {
    let f: unsafe extern "C" fn(
        *mut NifEnv,
        NifOption,
        unsafe extern "C" fn(*mut c_void),
    ) -> c_int = unsafe { std::mem::transmute(funcs().set_option) };
    unsafe { f(env, NifOption::OnHalt, callback) }
}

// ===========================================================================
// NIF 2.18 (OTP 29)
// ===========================================================================

/// Returns the number of words used on the heap by the term. NIF 2.18 (OTP 29.0). Wraps `enif_term_size`.
#[cfg(feature = "nif_2_18")]
pub(crate) unsafe fn term_size(term: NifTerm) -> usize {
    unsafe { (funcs().term_size)(term) }
}

/// Gets the atom cache index for an atom term. NIF 2.18 (OTP 29.0). Wraps `enif_get_atom_cache_index`.
#[cfg(feature = "nif_2_18")]
pub(crate) unsafe fn get_atom_cache_index(
    env: *mut NifEnv, atom: NifTerm, index: *mut c_uint,
) -> c_int {
    unsafe { (funcs().get_atom_cache_index)(env, atom, index) }
}

/// Returns the maximum atom cache index currently in use. NIF 2.18 (OTP 29.0). Wraps `enif_max_atom_cache_index`.
#[cfg(feature = "nif_2_18")]
pub(crate) unsafe fn max_atom_cache_index() -> c_uint {
    unsafe { (funcs().max_atom_cache_index)() }
}
