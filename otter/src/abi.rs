//! Per-build ABI tag: a content hash of this NIF library's own binary.
//!
//! The tag is appended to default resource-type names so that two
//! independently compiled builds register *distinct* types and never take
//! each other's resources over across a hot upgrade — the core
//! no-cross-build-ABI invariant (see `docs/UPGRADE.md`). A byte-identical
//! reload of the same `.so` hashes to the same tag, so self-takeover (reload
//! of the very same build) still works.

use std::collections::hash_map::DefaultHasher;
use std::hash::Hasher;
use std::sync::OnceLock;

/// This build's ABI tag. Computed once on first use and cached for the
/// lifetime of the load.
pub(crate) fn tag() -> u64 {
    static TAG: OnceLock<u64> = OnceLock::new();
    *TAG.get_or_init(compute)
}

fn compute() -> u64 {
    match self_path().and_then(|p| std::fs::read(p).ok()) {
        Some(bytes) => {
            // DefaultHasher's "unstable across Rust versions" caveat does not
            // bite us: a binary always hashes itself with its own compiled-in
            // algorithm, and a tag is only ever compared to itself.
            let mut h = DefaultHasher::new();
            h.write(&bytes);
            h.finish()
        }
        None => fallback(),
    }
}

/// Path to the shared object this code is compiled into.
#[cfg(unix)]
fn self_path() -> Option<std::path::PathBuf> {
    use std::ffi::{CStr, OsStr};
    use std::os::unix::ffi::OsStrExt;

    // The address of any function in this crate lies inside the one cdylib
    // (otter is linked in as an rlib), so dladdr resolves to its path.
    let mut info: libc::Dl_info = unsafe { std::mem::zeroed() };
    let addr = compute as *const std::ffi::c_void;
    if unsafe { libc::dladdr(addr, &mut info) } == 0 || info.dli_fname.is_null() {
        return None;
    }
    let bytes = unsafe { CStr::from_ptr(info.dli_fname) }.to_bytes();
    Some(std::path::PathBuf::from(OsStr::from_bytes(bytes)))
}

#[cfg(not(unix))]
fn self_path() -> Option<std::path::PathBuf> {
    None
}

/// A per-load value used when the binary can't be located or read. Being
/// distinct from any other build's tag (and from this build's true hash), it
/// degrades to "never take over" rather than risking a false takeover.
fn fallback() -> u64 {
    static SENTINEL: u8 = 0;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let aslr = &SENTINEL as *const u8 as u64;
    let mut h = DefaultHasher::new();
    h.write_u64(nanos);
    h.write_u64(aslr);
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tag_is_cached() {
        assert_eq!(tag(), tag());
    }

    #[test]
    fn computes_from_the_real_binary() {
        // Two independent reads+hashes of the same unchanged binary must
        // agree. The fallback uses the wall clock, so equality here also
        // proves the real dladdr+read path was taken, not the fallback.
        assert_eq!(compute(), compute());
    }
}
