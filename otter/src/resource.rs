//! Resource types: `Resource` trait, `ResourceArc<T>`, `Monitor`.
//!
//! Resources allow Rust values to live on the BEAM heap, garbage collected and
//! reference counted by the VM. A `ResourceArc<T>` is the Rust-side handle; the
//! corresponding Erlang-side value is a reference term.

use std::ffi::{c_int, c_void};
use std::marker::PhantomData;
use std::panic::{catch_unwind, AssertUnwindSafe};

use crate::codec::{CodecError, Decoder, Encoder};
use crate::priv_data::{PrivData, ResourceRegistry};
use crate::types::{AnyTerm, CallbackEnv, Env, InitEnv, LocalPid, Term};

// ---------------------------------------------------------------------------
// ResourceTypeHandle
// ---------------------------------------------------------------------------

/// A `Send` handle to a registered resource type `T`, obtained from
/// [`resource_handle`].
///
/// Holds the BEAM-side resource-type pointer so a resource can be created
/// ([`make`](Self::make)) without consulting `enif_priv_data` — capture it
/// inside a module-bound NIF call, then move it to an OS thread where no
/// module-bound env is available.
pub struct ResourceTypeHandle<T: Resource> {
    ptr: *mut enif_ffi::ResourceType,
    _t: PhantomData<fn() -> T>,
}

// SAFETY: enif_ffi::ResourceType is BEAM-internal data that lives for the VM's
// lifetime. Safe to share across threads once registered.
unsafe impl<T: Resource> Send for ResourceTypeHandle<T> {}
unsafe impl<T: Resource> Sync for ResourceTypeHandle<T> {}

impl<T: Resource> ResourceTypeHandle<T> {
    /// Wrap `val` in a new resource object on the BEAM heap.
    pub fn make(self, val: T) -> ResourceArc<T> {
        // Allocate enough for T at its required alignment (see ResourceArc docs).
        let alloc_size = std::mem::size_of::<T>() + std::mem::align_of::<T>() - 1;
        let raw = unsafe { enif_ffi::alloc_resource(self.ptr, alloc_size) };
        assert!(!raw.is_null(), "enif_alloc_resource returned null");
        let inner = align_ptr::<T>(raw);
        unsafe { std::ptr::write(inner, val) };
        ResourceArc { raw, inner }
    }
}

/// Look up this build's resource registry from a module-bound env. `None` if the
/// library has no `PrivData` installed (no load callback).
fn registry<'id>(env: impl Env<'id>) -> Option<&'id ResourceRegistry> {
    let pd = unsafe { enif_ffi::priv_data(env.raw_env()) } as *const PrivData;
    if pd.is_null() {
        return None;
    }
    // SAFETY: pd points at this build's PrivData, leaked for the VM lifetime.
    Some(unsafe { (*pd).registry() })
}

/// Obtain the [`ResourceTypeHandle`] for resource type `T`.
///
/// Looks `T` up in this library's registry (via `enif_priv_data`), so the env
/// must be module-bound. Panics if `T` was never registered.
pub fn resource_handle<'id, T: Resource>(env: impl Env<'id>) -> ResourceTypeHandle<T> {
    let ptr = registry(env).and_then(|r| r.get::<T>()).expect(
        "resource type not registered — call otter::resource::register::<T> in your load callback",
    );
    ResourceTypeHandle { ptr, _t: PhantomData }
}

/// Wrap `val` in a new resource object on the BEAM heap. Shorthand for
/// `resource_handle::<T>(env).make(val)`. Panics if `T` was never registered.
pub fn make_resource<'id, T: Resource>(env: impl Env<'id>, val: T) -> ResourceArc<T> {
    resource_handle::<T>(env).make(val)
}

// ---------------------------------------------------------------------------
// Monitor
// ---------------------------------------------------------------------------

/// A process monitor handle, returned by [`ResourceArc::monitor`]. Passed to
/// [`Resource::down`] when the monitored process exits.
#[derive(Clone, Copy)]
pub struct Monitor(pub(crate) enif_ffi::Monitor);

impl Monitor {
    /// Convert this monitor to a term (`enif_make_monitor_term`).
    pub fn to_term<'id>(self, env: impl Env<'id>) -> AnyTerm<'id> {
        let raw = unsafe { enif_ffi::make_monitor_term(env.raw_env(), &self.0) };
        AnyTerm::wrap(raw, env)
    }
}

impl PartialEq for Monitor {
    fn eq(&self, other: &Self) -> bool {
        // enif_compare_monitors is env-less.
        unsafe { enif_ffi::compare_monitors(&self.0, &other.0) == 0 }
    }
}

impl Eq for Monitor {}

// ---------------------------------------------------------------------------
// Resource trait
// ---------------------------------------------------------------------------

/// Marker and callback trait for Erlang resource types.
///
/// Implement on any `Sized + Send + Sync + 'static` type to let it live on the
/// BEAM heap as a resource object. The callbacks run with a [`CallbackEnv`].
///
/// Every resource type must be registered once in the load callback via
/// [`register`] before any `ResourceArc<T>` is created or decoded. A resource's
/// `T` is not assumed to survive a hot code upgrade across non-identical builds
/// (see `docs/UPGRADE.md`).
pub trait Resource: Sized + Send + Sync + 'static {
    /// Called when the BEAM garbage collects the last reference. Takes ownership
    /// of `self`; the default drops it.
    fn destructor(self, _env: CallbackEnv<'_>) {}

    /// Called when a process monitored via [`ResourceArc::monitor`] exits. The
    /// exiting process is always local, so `pid` is a [`LocalPid`].
    fn down<'a>(&'a self, _env: CallbackEnv<'a>, _pid: LocalPid, _monitor: Monitor) {}

    /// Called when the BEAM stops monitoring an event selected on this resource.
    /// `is_direct_call` is `true` when the callback runs synchronously inside
    /// the `select` call. A `stop` callback is registered unconditionally.
    fn stop(&self, _env: CallbackEnv<'_>, _event: enif_ffi::Event, _is_direct_call: bool) {}
}

// ---------------------------------------------------------------------------
// C-level callbacks (one instantiation per type T via monomorphization)
// ---------------------------------------------------------------------------

/// Absorb a panic that escaped a resource callback. These run on scheduler
/// threads outside any process context, so unwinding across the `extern "C"`
/// boundary would abort the VM and there is no process to raise into — catch
/// and log.
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

unsafe extern "C" fn destructor_callback<T: Resource>(env: *mut enif_ffi::Env, obj: *mut c_void) {
    let inner = align_ptr::<T>(obj);
    let result = catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: obj was written by ResourceTypeHandle::make and is not yet dropped.
        let val = unsafe { std::ptr::read(inner) };
        // SAFETY: env is valid for the duration of this callback.
        unsafe { CallbackEnv::with_raw(env, |cenv| val.destructor(cenv)) };
    }));
    absorb_callback_panic("destructor", result);
}

unsafe extern "C" fn down_callback<T: Resource>(
    env: *mut enif_ffi::Env,
    obj: *mut c_void,
    pid: *mut enif_ffi::Pid,
    mon: *mut enif_ffi::Monitor,
) {
    let inner = align_ptr::<T>(obj) as *const T;
    let result = catch_unwind(AssertUnwindSafe(|| {
        let pid = LocalPid { pid: unsafe { *pid } };
        let monitor = Monitor(unsafe { *mon });
        // SAFETY: env is valid for the callback; inner points at a live T.
        unsafe { CallbackEnv::with_raw(env, |cenv| (*inner).down(cenv, pid, monitor)) };
    }));
    absorb_callback_panic("down", result);
}

unsafe extern "C" fn stop_callback<T: Resource>(
    env: *mut enif_ffi::Env,
    obj: *mut c_void,
    event: enif_ffi::Event,
    is_direct_call: c_int,
) {
    let inner = align_ptr::<T>(obj) as *const T;
    let result = catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: env is valid for the callback; stop runs before the destructor,
        // so the value is live.
        unsafe { CallbackEnv::with_raw(env, |cenv| (*inner).stop(cenv, event, is_direct_call != 0)) };
    }));
    absorb_callback_panic("stop", result);
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

/// Flags controlling resource type registration. `CREATE` opens a new type;
/// `CREATE | TAKEOVER` additionally takes over a matching type from a previous
/// build during a hot upgrade.
pub use enif_ffi::ResourceFlags;

/// Register resource type `T` with the BEAM, keyed by the fully-qualified Rust
/// type path. Must be called exactly once per type, from the load/upgrade
/// callback ([`InitEnv`]), before any `ResourceArc<T>` is created or decoded.
///
/// The BEAM-side name is `"{type_name}#abi={hash}"`, where `{hash}` is a content
/// hash of this build's binary, so a different build takes a different name and
/// the BEAM does not take this build's resources over across an upgrade —
/// upholding the no-cross-build-ABI invariant. For deliberate cross-build
/// takeover, use [`register_tagged`].
pub fn register<'id, T: Resource>(env: InitEnv<'id>, flags: ResourceFlags) {
    let name = format!("{}#abi={:016x}", std::any::type_name::<T>(), crate::abi::tag());
    register_named::<T>(env, flags, &name);
}

/// Register resource type `T` under a stable, ABI-versioned name, opting it into
/// cross-build takeover during a hot upgrade (a per-type promise that `T`'s
/// layout is stable across those builds). Same calling rules as [`register`].
pub fn register_tagged<'id, T: Resource>(env: InitEnv<'id>, flags: ResourceFlags, tag: &str) {
    let name = format!("{}#tag={}", std::any::type_name::<T>(), tag);
    register_named::<T>(env, flags, &name);
}

fn register_named<'id, T: Resource>(env: InitEnv<'id>, flags: ResourceFlags, name: &str) {
    let cname =
        std::ffi::CString::new(name).expect("resource type name must not contain null bytes");

    let init = enif_ffi::ResourceTypeInit {
        dtor: Some(destructor_callback::<T>),
        stop: Some(stop_callback::<T>),
        down: Some(down_callback::<T>),
        members: 3,
        dyncall: None,
    };

    let mut tried = flags;
    let type_ptr =
        unsafe { enif_ffi::init_resource_type(env.raw_env(), cname.as_ptr(), &init, flags, &mut tried) };

    assert!(
        !type_ptr.is_null(),
        "enif_init_resource_type failed — it must run inside load/upgrade, and the \
         flags must match the name's state (CREATE needs a new name, TAKEOVER an \
         existing one); a name collision under CREATE alone also returns null"
    );

    // The load scaffolding installs PrivData before dispatching the user
    // callback, so the slot is non-null here.
    let pd = unsafe { enif_ffi::priv_data(env.raw_env()) } as *mut PrivData;
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
/// The BEAM holds one reference (via the resource term); each `ResourceArc`
/// handle holds another. The wrapped value is destroyed only when all
/// references — Erlang-side and Rust-side — are released.
///
/// `enif_alloc_resource` aligns to at least `sizeof(void*)`; to support stricter
/// alignment we allocate `size_of::<T>() + align_of::<T>() - 1` bytes and store
/// `raw` (allocation start, for keep/release) and `inner` (aligned write
/// position, for `Deref`/destructor).
pub struct ResourceArc<T: Resource> {
    raw: *mut c_void,
    inner: *mut T,
}

unsafe impl<T: Resource> Send for ResourceArc<T> {}
unsafe impl<T: Resource> Sync for ResourceArc<T> {}

/// Compute the aligned pointer for T within a raw allocation.
fn align_ptr<T>(raw: *mut c_void) -> *mut T {
    let align = std::mem::align_of::<T>();
    let aligned = ((raw as usize) + align - 1) & !(align - 1);
    aligned as *mut T
}

impl<T: Resource> ResourceArc<T> {
    /// The raw resource pointer. Used internally for `enif_select` and similar
    /// calls that need the raw allocation address.
    pub fn raw_ptr(&self) -> *mut c_void {
        self.raw
    }

    /// Monitor the process identified by `pid`. `Some(Monitor)` on success;
    /// `None` if the process is already dead or `pid` is not a valid local pid.
    /// `env` may be `None` when calling from a non-NIF thread.
    pub fn monitor<'id, E: Env<'id>>(&self, env: Option<E>, pid: &LocalPid) -> Option<Monitor> {
        let env_ptr = env.map(|e| e.raw_env()).unwrap_or(std::ptr::null_mut());
        // Zero-init an out-param the BEAM fills; sized by enif-ffi (sizeof(void*)*4,
        // 32 on 64-bit / 16 on 32-bit), so never hardcode the length here.
        let mut mon: enif_ffi::Monitor = unsafe { std::mem::zeroed() };
        let rc = unsafe { enif_ffi::monitor_process(env_ptr, self.raw, &pid.pid, &mut mon) };
        if rc == 0 {
            Some(Monitor(mon))
        } else {
            None
        }
    }

    /// Remove a monitor previously set with [`monitor`](Self::monitor). `true`
    /// if the monitor existed and was removed.
    pub fn demonitor<'id, E: Env<'id>>(&self, env: Option<E>, mon: &Monitor) -> bool {
        let env_ptr = env.map(|e| e.raw_env()).unwrap_or(std::ptr::null_mut());
        unsafe { enif_ffi::demonitor_process(env_ptr, self.raw, &mon.0) == 0 }
    }
}

impl<T: Resource> Clone for ResourceArc<T> {
    fn clone(&self) -> ResourceArc<T> {
        unsafe { enif_ffi::keep_resource(self.raw) };
        ResourceArc { raw: self.raw, inner: self.inner }
    }
}

impl<T: Resource> Drop for ResourceArc<T> {
    fn drop(&mut self) {
        // Decrement the ref count; at zero the BEAM calls destructor_callback.
        unsafe { enif_ffi::release_resource(self.raw) };
    }
}

impl<T: Resource> std::ops::Deref for ResourceArc<T> {
    type Target = T;
    fn deref(&self) -> &T {
        unsafe { &*self.inner }
    }
}

impl<'id, T: Resource> Encoder<'id> for ResourceArc<T> {
    /// Encode the resource as a reference term (`enif_make_resource`). The BEAM
    /// releases that reference when the term is garbage collected.
    fn encode(&self, env: impl Env<'id>) -> Result<AnyTerm<'id>, CodecError> {
        let raw = unsafe { enif_ffi::make_resource(env.raw_env(), self.raw) };
        Ok(AnyTerm::wrap(raw, env))
    }
}

impl<'id, T: Resource> Decoder<'id> for ResourceArc<T> {
    /// Decode a resource term into a `ResourceArc<T>`. `WrongType` if the term is
    /// not a resource of type `T`, or if `T` was never registered.
    fn decode(term: AnyTerm<'id>, env: impl Env<'id>) -> Result<Self, CodecError> {
        let type_ptr = registry(env).and_then(|r| r.get::<T>()).ok_or(CodecError::WrongType)?;

        let mut obj: *mut c_void = std::ptr::null_mut();
        if unsafe { enif_ffi::get_resource(env.raw_env(), term.raw_term(), type_ptr, &mut obj) } == 0 {
            return Err(CodecError::WrongType);
        }

        // New Rust-side reference; increment the ref count.
        unsafe { enif_ffi::keep_resource(obj) };
        Ok(ResourceArc { raw: obj, inner: align_ptr::<T>(obj) })
    }
}

// ---------------------------------------------------------------------------
// Dynamic resource call
// ---------------------------------------------------------------------------

/// Invoke a dynamic call on a resource (`enif_dynamic_resource_call`).
/// `mod_name`/`name` identify the resource type; `rsrc` is the resource term.
/// Returns `0` on success.
///
/// # Safety
/// `call_data` must match what the dyncall callback expects.
pub unsafe fn dynamic_resource_call<'id>(
    env: impl Env<'id>,
    mod_name: impl Term<'id>,
    name: impl Term<'id>,
    rsrc: impl Term<'id>,
    call_data: *mut c_void,
) -> i32 {
    unsafe {
        enif_ffi::dynamic_resource_call(
            env.raw_env(),
            mod_name.raw_term(),
            name.raw_term(),
            rsrc.raw_term(),
            call_data,
        )
    }
}
