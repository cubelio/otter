// The raw 1:1 unsafe enif surface. Public escape hatch under the `raw` feature;
// otherwise crate-private (the safe layer uses it via crate:: paths regardless).
#[cfg(feature = "raw")]
pub mod enif;
#[cfg(not(feature = "raw"))]
pub(crate) mod enif;
pub mod env;
pub mod types;
pub mod term;
pub mod codec;
pub mod resource;
pub mod priv_data;
mod abi;
pub mod alloc;
pub mod time;
pub mod system;
pub mod select;

#[doc(hidden)]
#[path = "__codegen.rs"]
pub mod __codegen;

// Re-export the raw enif-ffi crate so codegen-generated code — which is spliced
// into the *user's* crate and can therefore only name `::otter::…` paths — can
// reference the raw C ABI types (`Env`, `Term`, `Entry`, …) that appear in the
// `extern "C"` entry points it emits. Unconditional and only `#[doc(hidden)]`
// for now; gating it behind `raw` is tracked as issue enhance-11.
#[doc(hidden)]
pub use enif_ffi;

pub use otter_codegen::nif;
pub use otter_codegen::init;
pub use otter_codegen::resource_impl;

/// Retrieve an atom pre-declared in the `atoms = [...]` list of
/// [`init!`](crate::init).
///
/// Returns an [`Atom`] via a single atomic load — no hash lookup, no NIF call.
/// The atoms are interned once at NIF load (and re-interned on hot upgrade) by
/// the `init!`-generated scaffolding, so they are ready before any NIF runs.
///
/// ```ignore
/// otter::init!("my_nif", [/* nifs */], atoms = [ok, error]);
///
/// // in a NIF:
/// let ok: Atom = otter::atom![ok];
/// ```
///
/// [`Atom`]: crate::types::Atom
#[macro_export]
macro_rules! atom {
    ($id:ident) => {
        __otter_atoms::$id.get()
    };
}

/// Load all `enif_*` function pointers via `dlsym`.
///
/// Must be called exactly once, from the generated `nif_init` entry point,
/// before any other otter API is used.
///
/// Returns `Ok(())` on success, or `Err(name)` with the first symbol that
/// could not be resolved.
///
/// # Safety
///
/// Must be called from the BEAM's NIF loading context.
pub unsafe fn init() -> Result<(), &'static str> {
    unsafe { crate::enif::init() }
}
