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

// enif-ffi's `nif_init!` builds the platform entry point and resolves the
// enif_* table at load. `#[macro_export]` macros aren't reachable through the
// re-exported crate path (`otter::enif_ffi::nif_init!`), so re-export it by name
// into otter's root; the `init!`-generated code invokes `::otter::nif_init!`.
#[doc(hidden)]
pub use enif_ffi::nif_init;

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
