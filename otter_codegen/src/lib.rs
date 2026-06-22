use proc_macro::TokenStream;

mod init_macro;
mod nif_macro;
mod raw_macro;
mod resource_impl_macro;

/// Mark a Rust function as a NIF.
///
/// The annotated function takes an env as its first parameter (a `CallEnv<'_>`)
/// followed by one parameter per Erlang argument, each a type implementing
/// `Decoder`; it returns any type implementing `Encoder`. The macro generates the
/// `extern "C"` trampoline the BEAM calls: it decodes each argument (a failed
/// decode becomes `badarg`), invokes your function inside a `catch_unwind` (a
/// panic is turned into a raised exception rather than crossing the C-ABI
/// boundary), and encodes the result (a failed encode becomes `badret`). List the
/// function in [`init!`](macro@init)'s NIF table to export it.
///
/// To raise on purpose, return `Result<T, Raised<'_>>` and use `CallEnv::raise` /
/// `CallEnv::badarg`.
///
/// # Options
///
/// - `schedule = "DirtyCpu"` — run on a dirty CPU scheduler (long, CPU-bound work).
/// - `schedule = "DirtyIo"` — run on a dirty I/O scheduler (work that blocks).
/// - `name = "erlang_name"` — export under a different Erlang name than the Rust
///   function name.
///
/// ```text
/// #[otter::nif]
/// fn add(env: CallEnv<'_>, a: Integer<'_>, b: Integer<'_>) -> Integer<'_> {
///     Integer::from_i64(env, a.to_i64(env).unwrap() + b.to_i64(env).unwrap())
/// }
///
/// #[otter::nif(schedule = "DirtyCpu", name = "heavy")]
/// fn heavy_impl(env: CallEnv<'_>, n: Integer<'_>) -> Integer<'_> { /* … */ }
/// ```
#[proc_macro_attribute]
pub fn nif(attr: TokenStream, item: TokenStream) -> TokenStream {
    nif_macro::expand(attr.into(), item.into())
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

/// Declare the NIF library: its module name, exported NIFs, pre-interned atoms,
/// resource types, and lifecycle callbacks. Invoke it exactly once per library.
///
/// It generates the `nif_init` entry point, the `load`/`upgrade`/`unload`
/// scaffolding (which installs the private-data slot, registers resource types,
/// and interns the declared atoms — all under a `catch_unwind`), and the
/// `__otter_atoms` table that the `atom!` macro reads.
///
/// # Syntax
///
/// ```text
/// otter::init!(
///     "my_module",            // the Erlang module name (required, first)
///     [add, subtract],        // the NIF table — functions marked #[otter::nif]
///     atoms = [ok, error, not_found = "not found"],
///     resources = [MyResource, Conn: "conn-v1"],
///     load = on_load,         // optional lifecycle callbacks
///     upgrade = on_upgrade,
///     unload = on_unload,
/// );
/// ```
///
/// The first two arguments are positional; everything after is an
/// order-independent keyword entry:
///
/// - **`atoms = [name, alias = "beam name", …]`** — atoms interned once at load
///   (and re-interned on upgrade). Use `alias = "…"` when the BEAM atom text is
///   not a valid Rust identifier. Names are length-checked (≤255 chars) at
///   compile time. Retrieve with the `atom!` macro.
/// - **`resources = [Type, Type: "tag", …]`** — resource types to register. A
///   bare `Type` is registered under an ABI-fingerprinted name (no cross-build
///   takeover); `Type: "tag"` registers under a stable tagged name that opts into
///   hot-upgrade takeover.
/// - **`load = f` / `upgrade = f` / `unload = f`** — lifecycle callbacks. `load`
///   and `upgrade` receive the env and the decoded `load_info` term and return
///   `bool` (return `false` to veto the load); `unload` receives the env and
///   cannot veto. Their `load_raw` / `upgrade_raw` / `unload_raw` variants
///   (require the `raw` feature) additionally hand you the library's
///   `user_priv_data` `void*` to manage yourself.
/// - **`allow_panic_abort`** — a bare flag that opts out of the compile-time
///   guard which otherwise rejects building this crate with `panic = "abort"`
///   (abort would bypass otter's panic interception and crash the VM).
#[proc_macro]
pub fn init(input: TokenStream) -> TokenStream {
    init_macro::expand(input.into())
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

/// Optional marker attribute for an `impl Resource for T` block.
///
/// Currently it re-emits the impl block unchanged — writing `impl Resource for T`
/// directly is equivalent. It exists as the designated annotation point for
/// resource impls so that future codegen (or tooling) has a stable hook, and to
/// document intent at the impl site. Registration still happens via
/// [`init!`](macro@init)'s `resources = [...]` list.
#[proc_macro_attribute]
pub fn resource_impl(_attr: TokenStream, item: TokenStream) -> TokenStream {
    resource_impl_macro::expand(item.into())
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

/// Widen an item's visibility to `pub` only when the `raw` feature is enabled.
///
/// Write the item with its real, non-raw visibility; the macro emits the
/// original verbatim under `#[cfg(not(feature = "raw"))]` and a `pub` copy under
/// `#[cfg(feature = "raw")]`. The `cfg` resolves in the using crate, so this is
/// meant for a crate that has a `raw` feature. Works on any visibility-bearing
/// item (fn, method, struct, enum, type, const, static, mod, use), including
/// items written with no visibility at all.
///
/// ```text
/// #[raw] pub(crate) fn internal() {}   // pub under `raw`, else pub(crate)
/// #[raw] use crate::Thing;             // pub use under `raw`, else private
/// ```
#[proc_macro_attribute]
pub fn raw(attr: TokenStream, item: TokenStream) -> TokenStream {
    raw_macro::expand(attr.into(), item.into())
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}
