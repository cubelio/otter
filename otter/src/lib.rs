pub mod sys;
pub(crate) mod wrapper;
pub(crate) mod enif;
pub mod env;
pub mod types;
pub mod term;
pub mod codec;
pub mod resource;
pub mod time;
pub mod system;
pub mod select;

#[doc(hidden)]
#[path = "__codegen.rs"]
pub mod __codegen;

pub use otter_codegen::nif;
pub use otter_codegen::init;
pub use otter_codegen::resource_impl;

/// Declare pre-initialized atoms that can be retrieved with zero lookup cost.
///
/// Generates a hidden `__otter_atoms` module containing one [`StaticAtom`] per
/// entry, plus an `init` function that interns them all at once.
///
/// Atoms whose BEAM name is a valid Rust identifier can be listed bare.
/// Atoms whose name is not a valid identifier use `ident = "name"` syntax.
///
/// ```ignore
/// otter::declare_atoms![ok, error, content_type = "content-type"];
/// ```
///
/// Call [`init_atoms!`] from your `on_load` callback to initialize them,
/// then retrieve individual atoms with [`atom!`].
///
/// [`StaticAtom`]: crate::types::atom::StaticAtom
#[macro_export]
macro_rules! declare_atoms {
    // Parse comma-separated entries, each either `ident` or `ident = "name"`.
    ($($entry:tt)*) => {
        $crate::__declare_atoms_inner!($($entry)*);
    };
}

/// Internal helper — not part of the public API.
#[doc(hidden)]
#[macro_export]
macro_rules! __declare_atoms_inner {
    // Terminal — emit the module from accumulated pairs.
    (@acc [$(($ident:ident, $name:expr))*]) => {
        #[doc(hidden)]
        #[allow(non_upper_case_globals)]
        pub mod __otter_atoms {
            use $crate::types::atom::StaticAtom;

            $(pub static $ident: StaticAtom = StaticAtom::new($name);)*

            pub fn init(env: $crate::env::Env<'_>) {
                $($ident.init(env);)*
            }
        }
    };

    // ident = "name", more entries follow
    (@acc [$($done:tt)*] $id:ident = $lit:expr, $($rest:tt)*) => {
        $crate::__declare_atoms_inner!(@acc [$($done)* ($id, $lit)] $($rest)*);
    };
    // ident = "name", last entry
    (@acc [$($done:tt)*] $id:ident = $lit:expr) => {
        $crate::__declare_atoms_inner!(@acc [$($done)* ($id, $lit)]);
    };

    // bare ident, more entries follow
    (@acc [$($done:tt)*] $id:ident, $($rest:tt)*) => {
        $crate::__declare_atoms_inner!(@acc [$($done)* ($id, stringify!($id))] $($rest)*);
    };
    // bare ident, last entry
    (@acc [$($done:tt)*] $id:ident) => {
        $crate::__declare_atoms_inner!(@acc [$($done)* ($id, stringify!($id))]);
    };

    // Entry point — start with empty accumulator
    ($($rest:tt)*) => {
        $crate::__declare_atoms_inner!(@acc [] $($rest)*);
    };
}

/// Initialize all atoms declared with [`declare_atoms!`].
///
/// Must be called from the NIF `on_load` callback.
///
/// ```ignore
/// fn on_load(env: Env, _load_info: Term) -> bool {
///     otter::init_atoms!(env);
///     true
/// }
/// ```
#[macro_export]
macro_rules! init_atoms {
    ($env:expr) => {
        __otter_atoms::init($env)
    };
}

/// Retrieve a pre-declared atom by name.
///
/// Returns an [`Atom`] via a single atomic load — no hash lookup, no NIF call.
///
/// ```ignore
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
