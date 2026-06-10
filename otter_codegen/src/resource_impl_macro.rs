use proc_macro2::TokenStream;
use quote::quote;
use syn::{parse2, ItemImpl, Result};

pub fn expand(item: TokenStream) -> Result<TokenStream> {
    let impl_block: ItemImpl = parse2(item)?;

    Ok(quote! { #impl_block })
}
