//! `#[raw]` — widen an item's visibility to `pub` under the `raw` feature.
//!
//! Written on an item with its real (non-raw) visibility, e.g.
//! `#[raw] pub(crate) fn foo() {}`. The macro emits two `cfg`-gated copies:
//! the original verbatim under `#[cfg(not(feature = "raw"))]`, and a copy with
//! the visibility forced to `pub` under `#[cfg(feature = "raw")]`. The `cfg` is
//! resolved in the *using* crate's own compilation, so the macro stays oblivious
//! to feature state.
//!
//! It operates purely on the leading `attrs + visibility` of the item, so it is
//! agnostic to the item kind (fn, method, struct, enum, type, const, static,
//! mod, use, …) and to whether a visibility is written at all — a missing
//! visibility parses as [`Visibility::Inherited`] and is replaced just the same.

use proc_macro2::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{Attribute, Error, Result, Visibility, parse2};

/// An item peeled into its outer attributes, its visibility, and everything
/// after it (the keyword + body, kept verbatim).
struct RawItem {
    attrs: Vec<Attribute>,
    vis: Visibility,
    rest: TokenStream,
}

impl Parse for RawItem {
    fn parse(input: ParseStream) -> Result<Self> {
        let attrs = input.call(Attribute::parse_outer)?;
        // `Visibility::parse` never fails — it yields `Inherited` when no
        // visibility keyword is present (a private item).
        let vis: Visibility = input.parse()?;
        let rest: TokenStream = input.parse()?;
        Ok(RawItem { attrs, vis, rest })
    }
}

pub fn expand(attr: TokenStream, item: TokenStream) -> Result<TokenStream> {
    if !attr.is_empty() {
        return Err(Error::new_spanned(attr, "`#[raw]` takes no arguments"));
    }
    let RawItem { attrs, vis, rest } = parse2(item)?;
    Ok(quote! {
        #[cfg(not(feature = "raw"))]
        #(#attrs)*
        #vis #rest

        // The widened copy is the one that shows up in the docs (built with the
        // `raw` feature on); label it so rustdoc renders the feature pill. The
        // `doc(cfg)` fires only under `--cfg docsrs`, so it is inert on stable.
        #[cfg(feature = "raw")]
        #[cfg_attr(docsrs, doc(cfg(feature = "raw")))]
        #(#attrs)*
        pub #rest
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::{ToTokens, quote};

    /// Expand `src` and return one `(is_raw_branch, visibility, full_item)`
    /// tuple per emitted copy.
    fn copies(src: TokenStream) -> Vec<(bool, String, String)> {
        let out = expand(TokenStream::new(), src).expect("expansion failed");
        let file: syn::File = parse2(out).expect("output did not parse as items");
        file.items
            .iter()
            .map(|it| {
                let full = it.to_token_stream().to_string();
                // The raw copy carries `cfg(feature = "raw")`; the plain copy
                // carries `cfg(not(feature = "raw"))`, which has no `cfg (feature`.
                let is_raw = full.contains("cfg (feature = \"raw\")");
                (is_raw, item_vis(it).to_token_stream().to_string(), full)
            })
            .collect()
    }

    fn item_vis(it: &syn::Item) -> &Visibility {
        match it {
            syn::Item::Fn(x) => &x.vis,
            syn::Item::Struct(x) => &x.vis,
            syn::Item::Enum(x) => &x.vis,
            syn::Item::Use(x) => &x.vis,
            syn::Item::Mod(x) => &x.vis,
            syn::Item::Const(x) => &x.vis,
            syn::Item::Static(x) => &x.vis,
            syn::Item::Type(x) => &x.vis,
            other => panic!("unhandled item kind in test: {}", other.to_token_stream()),
        }
    }

    fn raw(cs: &[(bool, String, String)]) -> &(bool, String, String) {
        cs.iter().find(|c| c.0).expect("no raw branch")
    }
    fn plain(cs: &[(bool, String, String)]) -> &(bool, String, String) {
        cs.iter().find(|c| !c.0).expect("no plain branch")
    }

    #[test]
    fn emits_exactly_two_cfg_gated_copies() {
        let cs = copies(quote! { fn foo() {} });
        assert_eq!(cs.len(), 2);
        assert!(cs.iter().any(|c| c.0), "missing raw branch");
        assert!(cs.iter().any(|c| !c.0), "missing plain branch");
        assert!(plain(&cs).2.contains("cfg (not (feature = \"raw\"))"));
    }

    #[test]
    fn no_visibility_inherited_becomes_pub() {
        let cs = copies(quote! { fn foo() {} });
        assert_eq!(plain(&cs).1, "", "plain copy keeps the (empty) inherited vis");
        assert_eq!(raw(&cs).1, "pub");
    }

    #[test]
    fn pub_crate_becomes_pub() {
        let cs = copies(quote! { pub(crate) fn foo() {} });
        assert_eq!(plain(&cs).1, "pub (crate)");
        assert_eq!(raw(&cs).1, "pub");
    }

    #[test]
    fn pub_super_struct_becomes_pub() {
        let cs = copies(quote! { pub(super) struct S { x: u8 } });
        assert_eq!(plain(&cs).1, "pub (super)");
        assert_eq!(raw(&cs).1, "pub");
    }

    #[test]
    fn already_pub_stays_pub_in_both() {
        let cs = copies(quote! { pub fn foo() {} });
        assert_eq!(plain(&cs).1, "pub");
        assert_eq!(raw(&cs).1, "pub");
    }

    #[test]
    fn use_item_is_widened() {
        let cs = copies(quote! { use crate::Foo; });
        assert_eq!(raw(&cs).1, "pub");
        assert!(raw(&cs).2.contains("pub use crate :: Foo"));
        assert_eq!(plain(&cs).1, "");
    }

    #[test]
    fn mod_item_is_widened() {
        let cs = copies(quote! { mod inner; });
        assert_eq!(raw(&cs).1, "pub");
        assert!(raw(&cs).2.contains("pub mod inner"));
    }

    #[test]
    fn outer_attributes_are_preserved_on_both_copies() {
        let cs = copies(quote! { #[doc = "d"] fn foo() {} });
        for c in &cs {
            assert!(c.2.contains("doc = \"d\""), "attr lost in {}", c.2);
        }
        assert_eq!(raw(&cs).1, "pub");
    }

    #[test]
    fn rejects_arguments() {
        assert!(expand(quote! { something }, quote! { fn x() {} }).is_err());
    }
}
