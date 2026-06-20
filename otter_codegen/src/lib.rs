use proc_macro::TokenStream;

mod init_macro;
mod nif_macro;
mod raw_macro;
mod resource_impl_macro;

/// Attribute macro that transforms a Rust function into a NIF.
///
/// # Options
///
/// - `schedule = "DirtyCpu"` — run on a dirty CPU scheduler
/// - `schedule = "DirtyIo"` — run on a dirty I/O scheduler
/// - `name = "erlang_name"` — override the exported NIF name
#[proc_macro_attribute]
pub fn nif(attr: TokenStream, item: TokenStream) -> TokenStream {
    nif_macro::expand(attr.into(), item.into())
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

/// Generates the NIF library entry point (`nif_init`).
///
/// ```ignore
/// otter::init!("my_module", [add, subtract], load = on_load);
/// ```
#[proc_macro]
pub fn init(input: TokenStream) -> TokenStream {
    init_macro::expand(input.into())
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

/// Attribute macro for `impl Resource for T` blocks.
///
/// Optional attribute for `impl Resource for T` blocks.
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
/// ```ignore
/// #[raw] pub(crate) fn internal() {}   // pub under `raw`, else pub(crate)
/// #[raw] use crate::Thing;             // pub use under `raw`, else private
/// ```
#[proc_macro_attribute]
pub fn raw(attr: TokenStream, item: TokenStream) -> TokenStream {
    raw_macro::expand(attr.into(), item.into())
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}
