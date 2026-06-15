//! Resource types: `Resource` trait, `ResourceArc<T>`, `Monitor`.
//!
//! Resources allow Rust values to live on the BEAM heap, garbage collected
//! and reference counted by the VM. A `ResourceArc<T>` is the Rust-side
//! handle; the corresponding Erlang-side value is a reference term.

use std::ffi::{c_int, c_void};
use std::marker::PhantomData;

use crate::codec::{CodecError, Decoder, Encoder};
use crate::env::{Env, EnvKind};
use crate::priv_data::{PrivData, ResourceRegistry};
use crate::sys::{NifEnv, NifEvent, NifMonitor, NifPid, NifResourceType, NifResourceTypeInit};
use crate::term::{Term, AsNifTerm};
use crate::types::LocalPid;

// ---------------------------------------------------------------------------
// ResourceTypeHandle
// ---------------------------------------------------------------------------

/// A `Send` handle to a registered resource type `T`, obtained from
/// [`Env::resource_handle`].
///
/// Holds the BEAM-side resource-type pointer so a resource can be created
/// ([`make`](Self::make)) without consulting `enif_priv_data` — capture it
/// inside a module-bound NIF call, then move it to an OS thread or
/// [`OwnedEnv`](crate::env::OwnedEnv) where no module-bound env is available.
pub struct ResourceTypeHandle<T: Resource> {
    ptr: *mut NifResourceType,
    _t:  PhantomData<fn() -> T>,
}

// SAFETY: NifResourceType is BEAM-internal data that lives for the lifetime
// of the VM. Safe to share across threads once registered.
unsafe impl<T: Resource> Send for ResourceTypeHandle<T> {}
unsafe impl<T: Resource> Sync for ResourceTypeHandle<T> {}

impl<T: Resource> ResourceTypeHandle<T> {
    /// Wrap `val` in a new resource object on the BEAM heap.
    pub fn make(self, val: T) -> ResourceArc<T> {
        // Allocate enough for T at its required alignment (see ResourceArc docs).
        let alloc_size = std::mem::size_of::<T>() + std::mem::align_of::<T>() - 1;
        let raw = unsafe { crate::enif::alloc_resource(self.ptr, alloc_size) };
        assert!(!raw.is_null(), "enif_alloc_resource returned null");
        let inner = align_ptr::<T>(raw);
        unsafe { std::ptr::write(inner, val) };
        ResourceArc { raw, inner }
    }
}

/// Look up this build's resource registry from a module-bound env. Returns
/// `None` if the library has no `PrivData` installed (no load callback).
fn registry<'a>(env: Env<'a>) -> Option<&'a ResourceRegistry> {
    let pd = unsafe { crate::enif::priv_data(env.as_ptr()) } as *const PrivData;
    if pd.is_null() {
        return None;
    }
    // SAFETY: pd points at this build's PrivData, leaked for the VM lifetime.
    Some(unsafe { (*pd).registry() })
}

impl<'a> Env<'a> {
    /// Obtain the [`ResourceTypeHandle`] for resource type `T`.
    ///
    /// Looks `T` up in this library's registry (via `enif_priv_data`), so the
    /// env must be module-bound (a normal NIF call, or a load/upgrade env).
    /// Panics if `T` was never registered. Capture the returned handle before
    /// moving work to a thread or `OwnedEnv` that has no module-bound env.
    pub fn resource_handle<T: Resource>(self) -> ResourceTypeHandle<T> {
        let ptr = registry(self)
            .and_then(|r| r.get::<T>())
            .expect(
                "resource type not registered — call \
                 otter::resource::register::<T> in your load callback",
            );
        ResourceTypeHandle { ptr, _t: PhantomData }
    }

    /// Wrap `val` in a new resource object on the BEAM heap.
    ///
    /// Shorthand for `env.resource_handle::<T>().make(val)`. Panics if `T` was
    /// never registered.
    pub fn make_resource<T: Resource>(self, val: T) -> ResourceArc<T> {
        self.resource_handle::<T>().make(val)
    }
}

// ---------------------------------------------------------------------------
// Monitor
// ---------------------------------------------------------------------------

/// A process monitor handle, returned by [`ResourceArc::monitor`].
///
/// Passed to the [`Resource::down`] callback when the monitored process exits.
#[derive(Clone, Copy)]
pub struct Monitor(pub(crate) NifMonitor);

impl Monitor {
    /// Convert this monitor to a term.
    ///
    /// Wraps `enif_make_monitor_term`.
    pub fn to_term<'a>(self, env: Env<'a>) -> Term<'a> {
        env.make_monitor_term(&self.0)
    }
}

impl<'a> Env<'a> {
    /// Create a term from a monitor handle (`enif_make_monitor_term`).
    pub fn make_monitor_term(self, mon: &NifMonitor) -> Term<'a> {
        let raw = unsafe { crate::enif::make_monitor_term(self.as_ptr(), mon) };
        Term::new(self, raw)
    }
}

impl PartialEq for Monitor {
    fn eq(&self, other: &Self) -> bool {
        // enif_compare_monitors is env-less.
        unsafe { crate::enif::compare_monitors(&self.0, &other.0) == 0 }
    }
}

impl Eq for Monitor {}

// ---------------------------------------------------------------------------
// Resource trait
// ---------------------------------------------------------------------------

/// Marker and callback trait for Erlang resource types.
///
/// Implement this on any `Sized + Send + Sync + 'static` type to allow it to
/// live on the BEAM heap as a resource object.
///
/// ## Registration
///
/// Every resource type must be registered once in the NIF load callback
/// before any `ResourceArc<T>` is created or decoded:
///
/// ```ignore
/// fn load(env: Env<'_>, _info: Term<'_>) -> bool {
///     otter::resource::register::<MyType>(env);
///     true
/// }
/// ```
///
/// The registered type pointer lives in the library's private data, keyed by
/// `TypeId`; `env.make_resource::<MyType>(..)` and decoding consult it.
///
/// ## Hot upgrade
///
/// A resource's `T` is *not* assumed to survive a hot code upgrade across
/// non-identical builds. When a second build of the library takes over this
/// resource type, otter (outside the `raw` feature) must not assume it can
/// interpret or drop a `T` the previous build allocated — different compiler,
/// allocator, or layout. This is a core safety invariant; see `docs/UPGRADE.md`.
pub trait Resource: Sized + Send + Sync + 'static {
    /// Called when the BEAM garbage collects the last reference to this
    /// resource. Takes ownership of `self`; the default drops it.
    fn destructor(self, _env: Env<'_>) {}

    /// Called when a process monitored via [`ResourceArc::monitor`] exits.
    /// The default is a no-op. The exiting process is always local (only local
    /// processes can be monitored), so `pid` is a [`LocalPid`].
    fn down<'a>(&'a self, _env: Env<'a>, _pid: LocalPid, _monitor: Monitor) {}

    /// Called when the BEAM stops monitoring an event that was selected on
    /// this resource via [`select`](crate::select::select) — either an
    /// explicit `ERL_NIF_SELECT_STOP`/`CANCEL`, or VM cleanup of a
    /// still-selected event when the resource is garbage collected. The
    /// default is a no-op.
    ///
    /// `is_direct_call` is `true` when the callback runs synchronously inside
    /// the `select`/`select_x` call, `false` when it runs later from a
    /// scheduler thread.
    ///
    /// A `stop` callback is registered unconditionally for every resource
    /// type (the BEAM requires one for any resource passed to `enif_select`),
    /// so leaving this as the default no-op is harmless for resources that are
    /// never selected.
    fn stop(&self, _env: Env<'_>, _event: NifEvent, _is_direct_call: bool) {}
}

// ---------------------------------------------------------------------------
// C-level callbacks (one instantiation per type T via monomorphization)
// ---------------------------------------------------------------------------

/// Absorb a panic that escaped a resource callback.
///
/// `destructor` and `down` run on scheduler threads during GC or monitor
/// delivery — outside any Erlang process context. A panic must not unwind
/// across the `extern "C"` boundary (that aborts the whole VM), and there is
/// no process to raise an exception into, so the only correct action is to
/// catch and discard it. The message is logged to stderr for diagnosis.
fn absorb_callback_panic(what: &str, result: std::thread::Result<()>) {
    if let Err(payload) = result {
        let msg = payload
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
            .unwrap_or("<non-string panic payload>");
        eprintln!("otter: panic in resource {what} callback absorbed: {msg}");
    }
}

unsafe extern "C" fn destructor_callback<T: Resource>(env: *mut NifEnv, obj: *mut c_void) {
    let inner = align_ptr::<T>(obj);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // SAFETY: obj was written by ResourceTypeHandle::make and is not yet dropped.
        let val = unsafe { std::ptr::read(inner) };
        let marker = ();
        // SAFETY: env is valid for the duration of this callback.
        let env = unsafe { Env::new(&marker, env, EnvKind::Callback) };
        val.destructor(env);
    }));
    absorb_callback_panic("destructor", result);
}

unsafe extern "C" fn down_callback<T: Resource>(
    env: *mut NifEnv,
    obj: *mut c_void,
    pid: *mut NifPid,
    mon: *mut NifMonitor,
) {
    let inner = align_ptr::<T>(obj) as *const T;
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let marker = ();
        // SAFETY: env is valid for the duration of this callback.
        let env = unsafe { Env::new(&marker, env, EnvKind::Callback) };
        let pid = LocalPid { pid: unsafe { *pid } };
        let monitor = Monitor(unsafe { *mon });
        unsafe { (*inner).down(env, pid, monitor) };
    }));
    absorb_callback_panic("down", result);
}

unsafe extern "C" fn stop_callback<T: Resource>(
    env: *mut NifEnv,
    obj: *mut c_void,
    event: NifEvent,
    is_direct_call: c_int,
) {
    let inner = align_ptr::<T>(obj) as *const T;
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let marker = ();
        // SAFETY: env is valid for the duration of this callback.
        let env = unsafe { Env::new(&marker, env, EnvKind::Callback) };
        // SAFETY: obj points at a live, initialized T; stop runs before the
        // destructor, so the value is not yet dropped.
        unsafe { (*inner).stop(env, event, is_direct_call != 0) };
    }));
    absorb_callback_panic("stop", result);
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

/// Register resource type `T` with the BEAM, using the fully-qualified Rust
/// type path as the identifier.
///
/// Must be called exactly once per type, from the NIF load (or upgrade)
/// callback, before any `ResourceArc<T>` is created or decoded. Panics if
/// registration fails, is called twice, or is called outside a load/upgrade
/// env.
///
/// The identifier is `std::any::type_name::<T>()`, e.g.
/// `"my_crate::nifs::HashMapResource"`. This guarantees uniqueness within a
/// single NIF library (BEAM's resource type table is per-library), since
/// rustc's `type_name` for distinct types produces distinct strings in
/// practice. For backward compatibility with an existing resource type
/// identifier (or any case where the auto-derived string would be wrong),
/// use [`register_tagged`].
///
// NOTE: `std::any::type_name::<T>()` is documented as a "best-effort
// description", not a uniqueness contract. Within one crate it is
// nonetheless unambiguous — Rust's own path resolution forbids two items
// at the same path, so distinct types in the user's crate always yield
// distinct strings. The one way two genuinely-distinct types can share a
// `type_name` in a single cdylib is dependency-mediated: two
// semver-incompatible versions of the same crate (both linked in) render
// their crate name identically, so `dep::Foo` from v1 and `dep::Foo` from
// v2 collide. That requires a NIF to register `Resource` for a *dependency*
// type that is duplicated across versions — an exotic case we accept rather
// than guard. If it ever arises, the escape hatch is [`register_tagged`]
// with an explicit unique name.
pub fn register<T: Resource>(env: Env<'_>) {
    register_tagged::<T>(env, std::any::type_name::<T>());
}

/// Register resource type `T` with the BEAM under an explicit name.
///
/// Use this when the auto-derived `type_name` string would be wrong — for
/// example, when migrating a NIF library that previously registered the
/// type under a different identifier and you need the BEAM-side resource
/// type table to keep matching pre-existing resource terms (over a hot
/// reload of the same library, etc.). For new code prefer [`register`] which
/// is auto-named.
///
/// Same calling-context rules as [`register`]: load/upgrade callback only,
/// exactly once per type.
pub fn register_tagged<T: Resource>(env: Env<'_>, name: &str) {
    use crate::sys::NifResourceFlags;

    assert!(
        matches!(env.kind, EnvKind::Load | EnvKind::Upgrade),
        "register must be called from the NIF load or upgrade callback"
    );

    let cname = std::ffi::CString::new(name)
        .expect("resource type name must not contain null bytes");

    let init = NifResourceTypeInit {
        dtor:     Some(destructor_callback::<T>),
        stop:     Some(stop_callback::<T>),
        down:     Some(down_callback::<T>),
        members:  3,
        dyncall:  None,
    };

    let mut tried = NifResourceFlags::CREATE;
    let type_ptr = unsafe {
        crate::enif::init_resource_type(
            env.as_ptr(),
            cname.as_ptr(),
            &init,
            NifResourceFlags::CREATE,
            &mut tried,
        )
    };

    assert!(
        !type_ptr.is_null(),
        "enif_init_resource_type failed — ensure env is from the load callback"
    );

    // The load scaffolding installs PrivData before dispatching the user
    // callback, so the slot is non-null here.
    let pd = unsafe { crate::enif::priv_data(env.as_ptr()) } as *mut PrivData;
    assert!(
        !pd.is_null(),
        "priv_data not installed — register must run inside otter's load scaffolding"
    );
    // SAFETY: single-threaded load/upgrade; pd is this build's PrivData.
    unsafe { (*pd).registry_mut().insert::<T>(type_ptr) };
}

// ---------------------------------------------------------------------------
// ResourceArc<T>
// ---------------------------------------------------------------------------

/// A reference-counted Rust value on the BEAM heap.
///
/// `ResourceArc<T>` gives the BEAM GC visibility into the lifetime of the
/// wrapped value. The BEAM holds one reference (via the resource term);
/// each `ResourceArc` handle holds another. The wrapped value is destroyed
/// only when all references — both Erlang-side and Rust-side — are released.
///
/// ## Memory layout
///
/// `enif_alloc_resource` aligns to at least `sizeof(void*)`. To support types
/// with stricter alignment requirements, we allocate
/// `size_of::<T>() + align_of::<T>() - 1` bytes and store two pointers:
///
/// - `raw` — the allocation start, used with `enif_keep/release_resource`
/// - `inner` — the aligned write position within the allocation, used for
///   `Deref` and destructor calls
pub struct ResourceArc<T: Resource> {
    raw:   *mut c_void,
    inner: *mut T,
}

unsafe impl<T: Resource> Send for ResourceArc<T> {}
unsafe impl<T: Resource> Sync for ResourceArc<T> {}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Compute the aligned pointer for T within a raw allocation.
fn align_ptr<T>(raw: *mut c_void) -> *mut T {
    let align = std::mem::align_of::<T>();
    let aligned = ((raw as usize) + align - 1) & !(align - 1);
    aligned as *mut T
}

impl<T: Resource> ResourceArc<T> {
    /// Return the raw resource pointer. Used internally for `enif_select`
    /// and similar calls that need the raw allocation address.
    pub fn raw_ptr(&self) -> *mut c_void {
        self.raw
    }

    /// Monitor the process identified by `pid`.
    ///
    /// Returns `Some(Monitor)` on success. Returns `None` if the process is
    /// already dead or `pid` is not a valid local pid.
    ///
    /// `env` may be `None` when calling from a non-NIF thread (e.g. a dirty
    /// scheduler callback). Pass `Some(env)` from a normal NIF call.
    pub fn monitor(&self, env: Option<Env<'_>>, pid: &LocalPid) -> Option<Monitor> {
        let env_ptr = env.map(|e| e.as_ptr()).unwrap_or(std::ptr::null_mut());
        let mut mon = NifMonitor([0u8; 32]);
        let rc = unsafe {
            crate::enif::monitor_process(env_ptr, self.raw, &pid.pid, &mut mon)
        };
        if rc == 0 { Some(Monitor(mon)) } else { None }
    }

    /// Remove a monitor previously set with [`monitor`].
    ///
    /// Returns `true` if the monitor existed and was removed. Returns `false`
    /// if the monitor had already fired or was never valid.
    ///
    /// [`monitor`]: Self::monitor
    pub fn demonitor(&self, env: Option<Env<'_>>, mon: &Monitor) -> bool {
        let env_ptr = env.map(|e| e.as_ptr()).unwrap_or(std::ptr::null_mut());
        unsafe {
            crate::enif::demonitor_process(env_ptr, self.raw, &mon.0) == 0
        }
    }
}

// ---------------------------------------------------------------------------
// From<T>, Clone, Drop, Deref
// ---------------------------------------------------------------------------

impl<T: Resource> Clone for ResourceArc<T> {
    fn clone(&self) -> ResourceArc<T> {
        unsafe { crate::enif::keep_resource(self.raw) };
        ResourceArc { raw: self.raw, inner: self.inner }
    }
}

impl<T: Resource> Drop for ResourceArc<T> {
    fn drop(&mut self) {
        // Decrement ref count. When it hits zero, the BEAM calls
        // destructor_callback which reads and drops the T value.
        unsafe { crate::enif::release_resource(self.raw) };
    }
}

impl<T: Resource> std::ops::Deref for ResourceArc<T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { &*self.inner }
    }
}

// ---------------------------------------------------------------------------
// Encoder / Decoder
// ---------------------------------------------------------------------------

impl<T: Resource> Encoder for ResourceArc<T> {
    /// Encode the resource as a reference term.
    ///
    /// The resulting term holds an Erlang-side reference to the resource.
    /// The BEAM will release that reference when the term is garbage collected.
    fn encode<'a>(&self, env: Env<'a>) -> Term<'a> {
        let raw_term = unsafe {
            crate::enif::make_resource(env.as_ptr(), self.raw)
        };
        Term::new(env, raw_term)
    }
}

impl<'a, T: Resource> Decoder<'a> for ResourceArc<T> {
    /// Decode a resource term into a `ResourceArc<T>`.
    ///
    /// Returns `WrongType` if the term is not a resource of type `T`, or if
    /// the resource type has not been registered. `enif_get_resource` is a
    /// strict check — it returns false for any non-resource term and for
    /// resources of the wrong type — so we call it directly without a
    /// preliminary type-tag check.
    fn decode(term: Term<'a>) -> Result<Self, CodecError> {
        let type_ptr = registry(term.env)
            .and_then(|r| r.get::<T>())
            .ok_or(CodecError::WrongType)?;

        let mut obj: *mut c_void = std::ptr::null_mut();
        if unsafe {
            crate::enif::get_resource(term.env.as_ptr(), term.term, type_ptr, &mut obj) == 0
        } {
            return Err(CodecError::WrongType);
        }

        // We are creating a new Rust-side reference; increment the ref count.
        unsafe { crate::enif::keep_resource(obj) };

        let inner = align_ptr::<T>(obj);
        Ok(ResourceArc { raw: obj, inner })
    }
}

// ---------------------------------------------------------------------------
// Dynamic resource call
// ---------------------------------------------------------------------------

/// Invoke a dynamic call on a resource identified by module and name.
///
/// This calls the `dyncall` callback registered for the resource type.
/// `mod_name` and `name` identify the resource type; `rsrc` is the resource
/// term; `call_data` is passed to the callback.
///
/// Returns `0` on success, non-zero on failure.
///
/// # Safety
///
/// `call_data` must match what the dyncall callback expects.
///
/// Wraps `enif_dynamic_resource_call`.
pub unsafe fn dynamic_resource_call<'a>(
    env: Env<'a>,
    mod_name: impl AsNifTerm<'a>,
    name: impl AsNifTerm<'a>,
    rsrc: impl AsNifTerm<'a>,
    call_data: *mut c_void,
) -> i32 {
    unsafe {
        crate::enif::dynamic_resource_call(
            env.as_ptr(),
            mod_name.as_nif_term(),
            name.as_nif_term(),
            rsrc.as_nif_term(),
            call_data,
        )
    }
}
