//! Resource types: `Resource` trait, `ResourceArc<T>`, `Monitor`.
//!
//! Resources allow Rust values to live on the BEAM heap, garbage collected
//! and reference counted by the VM. A `ResourceArc<T>` is the Rust-side
//! handle; the corresponding Erlang-side value is a reference term.

use std::ffi::c_void;
use std::sync::OnceLock;

use crate::codec::{CodecError, Decoder, Encoder};
use crate::env::{Env, EnvKind};
use crate::sys::{NifEnv, NifMonitor, NifPid, NifResourceType, NifResourceTypeInit};
use crate::term::{RawTerm, TypedTerm};
use crate::types::Pid;

// ---------------------------------------------------------------------------
// ResourceTypeHandle
// ---------------------------------------------------------------------------

/// Opaque handle to a registered resource type.
///
/// Obtained from [`register_resource_type`] and stored in the static provided
/// by each [`Resource`] implementation via `resource_type_handle`.
pub struct ResourceTypeHandle(*mut NifResourceType);

// SAFETY: NifResourceType is BEAM-internal data that lives for the lifetime
// of the VM. Safe to share across threads once registered.
unsafe impl Send for ResourceTypeHandle {}
unsafe impl Sync for ResourceTypeHandle {}

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
    pub fn to_term<'a>(self, env: Env<'a>) -> TypedTerm<'a> {
        let raw = unsafe { crate::wrapper::monitor::make_monitor_term(env.as_ptr(), &self.0) };
        RawTerm::new(env, raw).resolve()
    }
}

impl PartialEq for Monitor {
    fn eq(&self, other: &Self) -> bool {
        crate::wrapper::monitor::compare_monitors(&self.0, &other.0) == 0
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
/// fn load(env: Env<'_>) {
///     otter::resource::register_resource_type::<MyType>(env, "MyType");
/// }
/// ```
///
/// ## Type handle storage
///
/// Implement `resource_type_handle` by declaring a module-level static:
///
/// ```ignore
/// static MY_TYPE_HANDLE: OnceLock<otter::resource::ResourceTypeHandle> = OnceLock::new();
///
/// impl otter::resource::Resource for MyType {
///     fn resource_type_handle() -> &'static OnceLock<otter::resource::ResourceTypeHandle> {
///         &MY_TYPE_HANDLE
///     }
/// }
/// ```
///
/// The `init!` macro will generate this boilerplate automatically in a future
/// release.
pub trait Resource: Sized + Send + Sync + 'static {
    /// Returns the static storage slot for this type's registered handle.
    ///
    /// Declare a `static OnceLock<ResourceTypeHandle>` and return a reference
    /// to it. Called by `ResourceArc` to look up the type pointer.
    fn resource_type_handle() -> &'static OnceLock<ResourceTypeHandle>;

    /// Called when the BEAM garbage collects the last reference to this
    /// resource. Takes ownership of `self`; the default drops it.
    fn destructor(self, _env: Env<'_>) {}

    /// Called when a process monitored via [`ResourceArc::monitor`] exits.
    /// The default is a no-op.
    fn down<'a>(&'a self, _env: Env<'a>, _pid: Pid, _monitor: Monitor) {}
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
        // SAFETY: obj was written by From<T> and is not yet dropped.
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
        let pid = Pid { term: unsafe { (*pid).pid } };
        let monitor = Monitor(unsafe { *mon });
        unsafe { (*inner).down(env, pid, monitor) };
    }));
    absorb_callback_panic("down", result);
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

/// Register resource type `T` with the BEAM.
///
/// Must be called exactly once per type, from the NIF load callback (an
/// `EnvKind::Init` env). Panics if registration fails or is called twice.
///
/// Must be called from the NIF load callback before any `ResourceArc<T>` is
/// constructed or decoded. Panics if called outside of the load callback.
pub fn register_resource_type<T: Resource>(env: Env<'_>, name: &str) {
    use crate::sys::NifResourceFlags;

    assert!(
        env.kind == EnvKind::Init,
        "register_resource_type must be called from the NIF load callback"
    );

    let cname = std::ffi::CString::new(name)
        .expect("resource type name must not contain null bytes");

    let init = NifResourceTypeInit {
        dtor:     Some(destructor_callback::<T>),
        stop:     None,
        down:     Some(down_callback::<T>),
        members:  3,
        dyncall:  None,
    };

    let mut tried = NifResourceFlags::CREATE;
    let type_ptr = unsafe {
        crate::wrapper::resource::init_resource_type(
            env.as_ptr(),
            cname.as_ptr(),
            &init,
            NifResourceFlags::CREATE,
            &mut tried,
        )
    };

    let type_ptr =
        type_ptr.expect("enif_init_resource_type failed — ensure env is from the load callback");

    T::resource_type_handle()
        .set(ResourceTypeHandle(type_ptr))
        .map_err(|_| ())
        .expect("resource type already registered");
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

    fn type_ptr() -> *mut NifResourceType {
        T::resource_type_handle()
            .get()
            .expect("ResourceArc: resource type not registered — \
                     call register_resource_type::<T> in your load callback")
            .0
    }

    /// Monitor the process identified by `pid`.
    ///
    /// Returns `Some(Monitor)` on success. Returns `None` if the process is
    /// already dead or `pid` is not a valid local pid.
    ///
    /// `env` may be `None` when calling from a non-NIF thread (e.g. a dirty
    /// scheduler callback). Pass `Some(env)` from a normal NIF call.
    pub fn monitor(&self, env: Option<Env<'_>>, pid: &Pid) -> Option<Monitor> {
        let env_ptr = env.map(|e| e.as_ptr()).unwrap_or(std::ptr::null_mut());
        let nif_pid = NifPid { pid: pid.term };
        let mut mon = NifMonitor([0u8; 32]);
        let rc = unsafe {
            crate::wrapper::monitor::monitor_process(env_ptr, self.raw, &nif_pid, &mut mon)
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
            crate::wrapper::monitor::demonitor_process(env_ptr, self.raw, &mon.0) == 0
        }
    }
}

// ---------------------------------------------------------------------------
// From<T>, Clone, Drop, Deref
// ---------------------------------------------------------------------------

impl<T: Resource> From<T> for ResourceArc<T> {
    /// Wrap `val` in a new resource object on the BEAM heap.
    ///
    /// Panics if the resource type has not been registered.
    fn from(val: T) -> ResourceArc<T> {
        // Allocate enough for T at its required alignment.
        let alloc_size = std::mem::size_of::<T>() + std::mem::align_of::<T>() - 1;
        let raw = unsafe { crate::wrapper::resource::alloc_resource(Self::type_ptr(), alloc_size) };
        assert!(!raw.is_null(), "enif_alloc_resource returned null");
        let inner = align_ptr::<T>(raw);
        unsafe { std::ptr::write(inner, val) };
        ResourceArc { raw, inner }
    }
}

impl<T: Resource> Clone for ResourceArc<T> {
    fn clone(&self) -> ResourceArc<T> {
        unsafe { crate::wrapper::resource::keep_resource(self.raw) };
        ResourceArc { raw: self.raw, inner: self.inner }
    }
}

impl<T: Resource> Drop for ResourceArc<T> {
    fn drop(&mut self) {
        // Decrement ref count. When it hits zero, the BEAM calls
        // destructor_callback which reads and drops the T value.
        unsafe { crate::wrapper::resource::release_resource(self.raw) };
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
    fn encode<'a>(&self, env: Env<'a>) -> RawTerm<'a> {
        let raw_term = unsafe {
            crate::wrapper::resource::make_resource(env.as_ptr(), self.raw)
        };
        RawTerm::new(env, raw_term)
    }
}

impl<'a, T: Resource> Decoder<'a> for ResourceArc<T> {
    /// Decode a resource term into a `ResourceArc<T>`.
    ///
    /// Returns `WrongType` if the term is not a resource of type `T`, or if
    /// the resource type has not been registered.
    fn decode(term: TypedTerm<'a>) -> Result<Self, CodecError> {
        // Resources appear as Reference terms from enif_term_type's perspective.
        let (raw_term, env) = match term {
            TypedTerm::Reference(r) => (r.term, r.env),
            _ => return Err(CodecError::WrongType),
        };

        let type_ptr = T::resource_type_handle()
            .get()
            .ok_or(CodecError::WrongType)?
            .0;

        let mut obj: *mut c_void = std::ptr::null_mut();
        if !unsafe {
            crate::wrapper::resource::get_resource(env.as_ptr(), raw_term, type_ptr, &mut obj)
        } {
            return Err(CodecError::WrongType);
        }

        // We are creating a new Rust-side reference; increment the ref count.
        unsafe { crate::wrapper::resource::keep_resource(obj) };

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
pub unsafe fn dynamic_resource_call(
    env: Env<'_>,
    mod_name: TypedTerm<'_>,
    name: TypedTerm<'_>,
    rsrc: TypedTerm<'_>,
    call_data: *mut c_void,
) -> i32 {
    unsafe {
        crate::wrapper::resource::dynamic_resource_call(
            env.as_ptr(),
            mod_name.as_raw(),
            name.as_raw(),
            rsrc.as_raw(),
            call_data,
        )
    }
}
