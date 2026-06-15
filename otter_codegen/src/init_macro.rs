use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};
use syn::{parse2, Error, LitStr, Path, Result, Token};
use syn::parse::{Parse, ParseStream};

// ---------------------------------------------------------------------------
// Input parsing
// ---------------------------------------------------------------------------

struct InitInput {
    module_name: LitStr,
    nifs: Vec<Path>,
    load: Option<Path>,
}

impl Parse for InitInput {
    fn parse(input: ParseStream) -> Result<Self> {
        let module_name: LitStr = input.parse()?;
        input.parse::<Token![,]>()?;

        let content;
        syn::bracketed!(content in input);
        let nifs = content
            .parse_terminated(Path::parse, Token![,])?
            .into_iter()
            .collect();

        let load = if input.peek(Token![,]) {
            input.parse::<Token![,]>()?;
            let key: syn::Ident = input.parse()?;
            if key != "load" {
                return Err(Error::new_spanned(key, "expected `load`"));
            }
            input.parse::<Token![=]>()?;
            Some(input.parse::<Path>()?)
        } else {
            None
        };

        Ok(InitInput { module_name, nifs, load })
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Given a NIF function path (`add` or `nifs::add`), produce the path to its
/// generated metadata constant (`__otter_nif_meta_add` or `nifs::__otter_nif_meta_add`).
fn meta_path(nif_path: &Path) -> Path {
    let mut path = nif_path.clone();
    let last = path.segments.last_mut().unwrap();
    last.ident = format_ident!("__otter_nif_meta_{}", last.ident);
    path
}

// ---------------------------------------------------------------------------
// Main expansion
// ---------------------------------------------------------------------------

pub fn expand(input: TokenStream) -> Result<TokenStream> {
    let input: InitInput = parse2(input)?;

    let module_name_bytes = syn::LitByteStr::new(
        format!("{}\0", input.module_name.value()).as_bytes(),
        Span::call_site(),
    );
    let nif_count = input.nifs.len();

    let meta_paths: Vec<Path> = input.nifs.iter().map(meta_path).collect();

    // --- load callback wrapper ---

    let (load_wrapper, load_value) = if let Some(ref load_fn) = input.load {
        let wrapper = quote! {
            #[doc(hidden)]
            unsafe extern "C" fn __otter_load(
                __otter_load_env: *mut ::otter::__codegen::NifEnv,
                __otter_priv_data: *mut *mut ::std::ffi::c_void,
                __otter_load_info: ::otter::__codegen::NifTerm,
            ) -> ::std::ffi::c_int {
                // Publish PrivData before dispatching the user callback so that
                // resource registration can populate it via enif_priv_data.
                let __otter_pd = unsafe {
                    ::otter::__codegen::install_priv_data(__otter_priv_data)
                };
                let __marker = ();
                let __env = unsafe {
                    ::otter::__codegen::new_env(
                        &__marker,
                        __otter_load_env,
                        ::otter::__codegen::EnvKind::Load,
                    )
                };
                let __load_info_raw = ::otter::__codegen::new_raw_term(
                    __env, __otter_load_info,
                );
                let __load_info = match ::otter::__codegen::Decoder::decode(__load_info_raw) {
                    Ok(v) => v,
                    Err(_) => {
                        unsafe {
                            ::otter::__codegen::discard_priv_data(__otter_priv_data, __otter_pd)
                        };
                        return ::otter::__codegen::LOAD_FAILED_DECODE;
                    }
                };
                let __result = ::std::panic::catch_unwind(
                    ::std::panic::AssertUnwindSafe(|| #load_fn(__env, __load_info))
                );
                match __result {
                    Ok(true) => ::otter::__codegen::LOAD_OK,
                    Ok(false) => {
                        unsafe {
                            ::otter::__codegen::discard_priv_data(__otter_priv_data, __otter_pd)
                        };
                        ::otter::__codegen::LOAD_FAILED_USER_FALSE
                    }
                    Err(_) => {
                        unsafe {
                            ::otter::__codegen::discard_priv_data(__otter_priv_data, __otter_pd)
                        };
                        ::otter::__codegen::LOAD_FAILED_PANIC
                    }
                }
            }
        };
        (wrapper, quote! { Some(__otter_load) })
    } else {
        (quote! {}, quote! { None })
    };

    // --- nif_init entry point ---

    Ok(quote! {
        #load_wrapper

        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn nif_init() -> *const ::otter::__codegen::NifEntry {
            if let Err(sym) = unsafe { ::otter::init() } {
                eprintln!("otter: failed to resolve symbol `{sym}` — the NIF \
                    was compiled for a newer NIF API version than this BEAM supports. \
                    NIF load aborted.");
                return ::std::ptr::null();
            }

            let mut __otter_funcs = ::std::vec![
                #( #meta_paths .to_nif_func() ),*
            ];
            let __otter_funcs_ptr = __otter_funcs.as_mut_ptr();
            ::std::mem::forget(__otter_funcs);

            let __otter_entry = ::std::boxed::Box::new(::otter::__codegen::NifEntry {
                major: ::otter::__codegen::NIF_MAJOR_VERSION,
                minor: ::otter::__codegen::NIF_MINOR_VERSION,
                name: #module_name_bytes .as_ptr() as *const ::std::ffi::c_char,
                num_of_funcs: #nif_count as ::std::ffi::c_int,
                funcs: __otter_funcs_ptr,
                load: #load_value,
                reload: None,
                upgrade: None,
                unload: None,
                vm_variant: ::otter::__codegen::NIF_VM_VARIANT.as_ptr(),
                options: ::otter::__codegen::NIF_ENTRY_OPTIONS as ::std::ffi::c_uint,
                sizeof_resource_type_init: ::std::mem::size_of::<
                    ::otter::__codegen::NifResourceTypeInit
                >(),
                min_erts: ::otter::__codegen::NIF_MIN_ERTS_VERSION.as_ptr(),
            });
            ::std::boxed::Box::leak(__otter_entry) as *const _
        }
    })
}
