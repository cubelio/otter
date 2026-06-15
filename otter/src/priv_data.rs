//! `PrivData` — the per-library private data the BEAM hands back via
//! `enif_priv_data`.
//!
//! Hot code upgrade loads a second build of a NIF library beside the first.
//! The two builds may differ in compiler, allocator, or std layout, so the
//! only fields one build may read from another build's `PrivData` are the two
//! in the frozen `#[repr(C)]` header: `magic` and `user_priv_data`, at their
//! fixed offsets. Everything after the header (the resource-type [registry])
//! is build-private: each build constructs its own and drops it with its own
//! allocator in its own `unload`. See `docs/UPGRADE.md`.
//!
//! [registry]: ResourceRegistry

use std::any::TypeId;
use std::collections::HashMap;
use std::ffi::c_void;

use crate::sys::NifResourceType;

/// Layout-version tag for the frozen header. The trailing digit is bumped only
/// if the two header fields below ever change; it subsumes any separate
/// version field. Both sides of any upgrade are otter, so a mismatch is a
/// non-case in practice — the check exists purely to refuse to interpret
/// genuinely foreign private data.
pub const PRIV_MAGIC: u64 = u64::from_be_bytes(*b"RUSTNIF1");

/// Library-private data owned by otter for the lifetime of one build of the
/// library.
///
/// The first two fields form the frozen cross-build header (see module docs);
/// `registry` is build-private and never read across the upgrade boundary.
#[repr(C)]
pub struct PrivData {
    /// Always [`PRIV_MAGIC`]. Read cross-build to confirm the header layout.
    magic: u64,
    /// The user's own private data pointer (tier-2 `raw` mode). Always null in
    /// tier-1 mode. Read cross-build on upgrade to hand the user their old
    /// pointer.
    user_priv_data: *mut c_void,
    /// Resource types registered by *this* build. Keyed by [`TypeId`], which
    /// distinguishes types within one build and is only ever compared within
    /// one build.
    registry: ResourceRegistry,
}

impl PrivData {
    /// A freshly-initialized `PrivData` with an empty registry and no user
    /// pointer.
    pub fn new() -> PrivData {
        PrivData {
            magic: PRIV_MAGIC,
            user_priv_data: std::ptr::null_mut(),
            registry: ResourceRegistry::new(),
        }
    }

    pub(crate) fn registry(&self) -> &ResourceRegistry {
        &self.registry
    }

    pub(crate) fn registry_mut(&mut self) -> &mut ResourceRegistry {
        &mut self.registry
    }
}

impl Default for PrivData {
    fn default() -> Self {
        Self::new()
    }
}

/// Maps each registered resource type to its BEAM-side `NifResourceType`
/// pointer. Build-private; reconstructed fresh by every build.
pub struct ResourceRegistry {
    types: HashMap<TypeId, *mut NifResourceType>,
}

impl ResourceRegistry {
    fn new() -> ResourceRegistry {
        ResourceRegistry { types: HashMap::new() }
    }

    /// Record the resource type pointer for `T`. Panics if `T` is already
    /// registered (registration must happen exactly once per type).
    pub(crate) fn insert<T: 'static>(&mut self, ptr: *mut NifResourceType) {
        let prev = self.types.insert(TypeId::of::<T>(), ptr);
        assert!(prev.is_none(), "resource type already registered");
    }

    /// Look up the resource type pointer for `T`, if registered.
    pub(crate) fn get<T: 'static>(&self) -> Option<*mut NifResourceType> {
        self.types.get(&TypeId::of::<T>()).copied()
    }
}

// ---------------------------------------------------------------------------
// Lifecycle helpers used by generated load/unload scaffolding
// ---------------------------------------------------------------------------

/// Allocate a fresh [`PrivData`] and publish it into the BEAM's private-data
/// slot, so that registration during the load callback can populate it via
/// `enif_priv_data`.
///
/// Returns the raw pointer so a vetoed load can hand it back to
/// [`discard_priv_data`].
///
/// # Safety
///
/// `slot` must be the `*mut *mut c_void` priv-data slot passed to the load
/// callback by the BEAM.
pub unsafe fn install_priv_data(slot: *mut *mut c_void) -> *mut PrivData {
    let pd = Box::into_raw(Box::new(PrivData::new()));
    unsafe { *slot = pd as *mut c_void };
    pd
}

/// Free a [`PrivData`] published by [`install_priv_data`] and clear the slot.
/// Called when the user's load/upgrade callback vetoes.
///
/// # Safety
///
/// `pd` must have come from [`install_priv_data`] with this same `slot`, and
/// must not have been freed already.
pub unsafe fn discard_priv_data(slot: *mut *mut c_void, pd: *mut PrivData) {
    unsafe {
        drop(Box::from_raw(pd));
        *slot = std::ptr::null_mut();
    }
}

/// Free a [`PrivData`] in the unload callback, which receives the pointer by
/// value rather than through a slot.
///
/// # Safety
///
/// `pd` must have come from [`install_priv_data`] and must not have been freed
/// already.
pub unsafe fn free_priv_data(pd: *mut PrivData) {
    unsafe { drop(Box::from_raw(pd)) };
}
