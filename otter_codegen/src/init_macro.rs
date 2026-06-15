use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};
use syn::{parse2, Error, Ident, LitByteStr, LitStr, Path, Result, Token};
use syn::parse::{Parse, ParseStream};

// ---------------------------------------------------------------------------
// Input parsing
// ---------------------------------------------------------------------------

/// A single entry in the `resources = [...]` list: a resource type, optionally
/// followed by `: "tag"` to give it a stable, ABI-versioned registration name.
struct ResourceEntry {
    ty:  Path,
    tag: Option<LitStr>,
}

impl Parse for ResourceEntry {
    fn parse(input: ParseStream) -> Result<Self> {
        let ty: Path = input.parse()?;
        let tag = if input.peek(Token![:]) {
            input.parse::<Token![:]>()?;
            Some(input.parse::<LitStr>()?)
        } else {
            None
        };
        Ok(ResourceEntry { ty, tag })
    }
}

/// A `load`/`upgrade`/`unload` slot: unset, a tier-1 (plain) user fn, or a
/// tier-2 (`_raw`) user fn that manages the `user_priv_data` `void*`.
enum Callback {
    None,
    Plain(Path),
    Raw(Path),
}

struct InitInput {
    module_name: LitStr,
    nifs:        Vec<Path>,
    resources:   Vec<ResourceEntry>,
    load:        Callback,
    upgrade:     Callback,
    unload:      Callback,
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

        let mut resources = Vec::new();
        let mut seen_resources = false;
        let mut load = Callback::None;
        let mut upgrade = Callback::None;
        let mut unload = Callback::None;

        // Remaining arguments are order-independent keyword entries:
        //   resources = [..], load[_raw] = f, upgrade[_raw] = f, unload[_raw] = f
        while input.peek(Token![,]) {
            input.parse::<Token![,]>()?;
            if input.is_empty() {
                break; // tolerate a trailing comma
            }
            let key: Ident = input.parse()?;
            input.parse::<Token![=]>()?;
            match key.to_string().as_str() {
                "resources" => {
                    if seen_resources {
                        return Err(Error::new_spanned(&key, "duplicate `resources`"));
                    }
                    seen_resources = true;
                    let content;
                    syn::bracketed!(content in input);
                    resources = content
                        .parse_terminated(ResourceEntry::parse, Token![,])?
                        .into_iter()
                        .collect();
                }
                "load" => set_plain(&mut load, &key, input)?,
                "upgrade" => set_plain(&mut upgrade, &key, input)?,
                "unload" => set_plain(&mut unload, &key, input)?,
                "load_raw" => set_raw(&mut load, &key, input)?,
                "upgrade_raw" => set_raw(&mut upgrade, &key, input)?,
                "unload_raw" => set_raw(&mut unload, &key, input)?,
                other => {
                    return Err(Error::new_spanned(
                        &key,
                        format!(
                            "unknown init! key `{other}` — expected `resources`, \
                             `load`, `upgrade`, `unload` (or their `_raw` variants)"
                        ),
                    ));
                }
            }
        }

        Ok(InitInput { module_name, nifs, resources, load, upgrade, unload })
    }
}

/// `load` / `upgrade` / `unload` (the kind name is implied by which slot).
fn kind_of(key: &Ident) -> &'static str {
    let s = key.to_string();
    if s.starts_with("load") {
        "load"
    } else if s.starts_with("upgrade") {
        "upgrade"
    } else {
        "unload"
    }
}

fn set_plain(slot: &mut Callback, key: &Ident, input: ParseStream) -> Result<()> {
    ensure_unset(slot, key)?;
    *slot = Callback::Plain(input.parse::<Path>()?);
    Ok(())
}

fn set_raw(slot: &mut Callback, key: &Ident, input: ParseStream) -> Result<()> {
    if !cfg!(feature = "raw") {
        return Err(Error::new_spanned(
            key,
            format!(
                "`{key}` requires otter's `raw` feature — enable it with \
                 `otter = {{ version = \"…\", features = [\"raw\"] }}`. The tier-2 \
                 `_raw` callbacks hand you the library's `priv_data` `void*` directly."
            ),
        ));
    }
    ensure_unset(slot, key)?;
    *slot = Callback::Raw(input.parse::<Path>()?);
    Ok(())
}

fn ensure_unset(slot: &Callback, key: &Ident) -> Result<()> {
    if matches!(slot, Callback::None) {
        Ok(())
    } else {
        let kind = kind_of(key);
        Err(Error::new_spanned(
            key,
            format!("duplicate `{kind}` callback — `{kind}` and `{kind}_raw` may appear at most once, combined"),
        ))
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

/// The `c_int`-valued body that dispatches an optional `load`/`upgrade`
/// callback, run inside the scaffolding's `catch_unwind` after registration.
/// `info` names the raw `load_info` term; `old` names the `*mut *mut c_void`
/// old-priv-data slot (present only for `upgrade`).
fn lifecycle_dispatch(cb: &Callback, info: TokenStream, old: Option<TokenStream>) -> TokenStream {
    match cb {
        Callback::None => quote! {{
            let _ = #info;
            ::otter::__codegen::LOAD_OK
        }},
        Callback::Plain(f) => quote! {{
            let __otter_info_raw = ::otter::__codegen::new_raw_term(__env, #info);
            match ::otter::__codegen::Decoder::decode(__otter_info_raw) {
                Ok(__otter_info) => if #f(__env, __otter_info) {
                    ::otter::__codegen::LOAD_OK
                } else {
                    ::otter::__codegen::LOAD_FAILED_USER_FALSE
                },
                Err(_) => ::otter::__codegen::LOAD_FAILED_DECODE,
            }
        }},
        Callback::Raw(f) => {
            // The user fn receives `&mut` handles to the `user_priv_data`
            // void* it owns (plus the old build's, for upgrade).
            let call = match &old {
                None => quote! {
                    #f(__env, unsafe { &mut *::otter::__codegen::user_priv_field(__pd) }, __otter_info)
                },
                Some(old_slot) => quote! {
                    {
                        let __otter_new_ref = unsafe {
                            &mut *::otter::__codegen::user_priv_field(__pd)
                        };
                        let __otter_old_field = unsafe {
                            ::otter::__codegen::old_user_priv_field(#old_slot)
                        };
                        let mut __otter_old_scratch: *mut ::std::ffi::c_void =
                            ::std::ptr::null_mut();
                        let __otter_old_ref = if __otter_old_field.is_null() {
                            &mut __otter_old_scratch
                        } else {
                            unsafe { &mut *__otter_old_field }
                        };
                        #f(__env, __otter_new_ref, __otter_old_ref, __otter_info)
                    }
                },
            };
            quote! {{
                let __otter_info_raw = ::otter::__codegen::new_raw_term(__env, #info);
                match ::otter::__codegen::Decoder::decode(__otter_info_raw) {
                    Ok(__otter_info) => if #call {
                        ::otter::__codegen::LOAD_OK
                    } else {
                        ::otter::__codegen::LOAD_FAILED_USER_FALSE
                    },
                    Err(_) => ::otter::__codegen::LOAD_FAILED_DECODE,
                }
            }}
        }
    }
}

// ---------------------------------------------------------------------------
// Main expansion
// ---------------------------------------------------------------------------

pub fn expand(input: TokenStream) -> Result<TokenStream> {
    let input: InitInput = parse2(input)?;

    let module_name_bytes = LitByteStr::new(
        format!("{}\0", input.module_name.value()).as_bytes(),
        Span::call_site(),
    );
    let nif_count = input.nifs.len();
    let meta_paths: Vec<Path> = input.nifs.iter().map(meta_path).collect();

    // --- resource registration ---
    //
    // Generated once and called from both load (CREATE) and upgrade
    // (CREATE | TAKEOVER). PrivData is published before this runs, so a user
    // callback may register additional types into the same live registry.
    let register_body = if input.resources.is_empty() {
        quote! { let _ = (__otter_env, __otter_flags); }
    } else {
        let register_calls = input.resources.iter().map(|entry| {
            let ty = &entry.ty;
            match &entry.tag {
                Some(tag) => quote! {
                    ::otter::__codegen::register_tagged::<#ty>(__otter_env, __otter_flags, #tag);
                },
                None => quote! {
                    ::otter::__codegen::register::<#ty>(__otter_env, __otter_flags);
                },
            }
        });
        quote! { #( #register_calls )* }
    };
    let register_fn = quote! {
        #[doc(hidden)]
        fn __otter_register(
            __otter_env:   ::otter::__codegen::Env<'_>,
            __otter_flags: ::otter::__codegen::ResourceFlags,
        ) {
            #register_body
        }
    };

    // --- load / upgrade wrappers ---
    //
    // Both: install PrivData, register resources, dispatch the optional user
    // callback — all under one catch_unwind. Any veto (user `false`, decode
    // failure, or panic) frees the PrivData and NULLs the slot.

    let load_body = lifecycle_dispatch(&input.load, quote! { __otter_load_info }, None);
    let upgrade_body = lifecycle_dispatch(
        &input.upgrade,
        quote! { __otter_upgrade_info },
        Some(quote! { __otter_old_priv }),
    );

    let load_wrapper = quote! {
        #[doc(hidden)]
        unsafe extern "C" fn __otter_load(
            __otter_load_env:  *mut ::otter::__codegen::NifEnv,
            __otter_priv_data: *mut *mut ::std::ffi::c_void,
            __otter_load_info: ::otter::__codegen::NifTerm,
        ) -> ::std::ffi::c_int {
            let __marker = ();
            let __env = unsafe {
                ::otter::__codegen::new_env(
                    &__marker, __otter_load_env, ::otter::__codegen::EnvKind::Load,
                )
            };
            let __pd = unsafe { ::otter::__codegen::install_priv_data(__otter_priv_data) };
            let __outcome = ::std::panic::catch_unwind(::std::panic::AssertUnwindSafe(|| {
                __otter_register(__env, ::otter::__codegen::ResourceFlags::CREATE);
                #load_body
            }));
            match __outcome {
                Ok(::otter::__codegen::LOAD_OK) => ::otter::__codegen::LOAD_OK,
                Ok(__code) => {
                    unsafe { ::otter::__codegen::discard_priv_data(__otter_priv_data, __pd) };
                    __code
                }
                Err(_) => {
                    unsafe { ::otter::__codegen::discard_priv_data(__otter_priv_data, __pd) };
                    ::otter::__codegen::LOAD_FAILED_PANIC
                }
            }
        }
    };

    // The upgrade wrapper consumes the old-priv slot only when a raw callback
    // reads it; otherwise mark it used to avoid an unused-variable warning.
    let upgrade_old_consume = match &input.upgrade {
        Callback::Raw(_) => quote! {},
        _ => quote! { let _ = __otter_old_priv; },
    };
    let upgrade_wrapper = quote! {
        #[doc(hidden)]
        unsafe extern "C" fn __otter_upgrade(
            __otter_upgrade_env: *mut ::otter::__codegen::NifEnv,
            __otter_priv_data:   *mut *mut ::std::ffi::c_void,
            __otter_old_priv:    *mut *mut ::std::ffi::c_void,
            __otter_upgrade_info: ::otter::__codegen::NifTerm,
        ) -> ::std::ffi::c_int {
            #upgrade_old_consume
            let __marker = ();
            let __env = unsafe {
                ::otter::__codegen::new_env(
                    &__marker, __otter_upgrade_env, ::otter::__codegen::EnvKind::Upgrade,
                )
            };
            let __pd = unsafe { ::otter::__codegen::install_priv_data(__otter_priv_data) };
            let __outcome = ::std::panic::catch_unwind(::std::panic::AssertUnwindSafe(|| {
                __otter_register(
                    __env,
                    ::otter::__codegen::ResourceFlags::CREATE
                        | ::otter::__codegen::ResourceFlags::TAKEOVER,
                );
                #upgrade_body
            }));
            match __outcome {
                Ok(::otter::__codegen::LOAD_OK) => ::otter::__codegen::LOAD_OK,
                Ok(__code) => {
                    unsafe { ::otter::__codegen::discard_priv_data(__otter_priv_data, __pd) };
                    __code
                }
                Err(_) => {
                    unsafe { ::otter::__codegen::discard_priv_data(__otter_priv_data, __pd) };
                    ::otter::__codegen::LOAD_FAILED_PANIC
                }
            }
        }
    };

    // --- unload wrapper ---
    //
    // Dispatches the optional user callback (which cannot veto; a panic is
    // absorbed) and frees this build's PrivData. The BEAM passes priv_data by
    // value here, not through a slot.
    let unload_dispatch = match &input.unload {
        Callback::None => quote! { let _ = __otter_unload_env; },
        Callback::Plain(f) => quote! {
            let __marker = ();
            let __env = unsafe {
                ::otter::__codegen::new_env(
                    &__marker, __otter_unload_env, ::otter::__codegen::EnvKind::Unload,
                )
            };
            let _ = ::std::panic::catch_unwind(::std::panic::AssertUnwindSafe(|| #f(__env)));
        },
        Callback::Raw(f) => quote! {
            let __marker = ();
            let __env = unsafe {
                ::otter::__codegen::new_env(
                    &__marker, __otter_unload_env, ::otter::__codegen::EnvKind::Unload,
                )
            };
            let __otter_user = unsafe {
                *::otter::__codegen::user_priv_field(
                    __otter_priv_data as *mut ::otter::__codegen::PrivData,
                )
            };
            let _ = ::std::panic::catch_unwind(
                ::std::panic::AssertUnwindSafe(|| #f(__env, __otter_user)),
            );
        },
    };
    let unload_wrapper = quote! {
        #[doc(hidden)]
        unsafe extern "C" fn __otter_unload(
            __otter_unload_env: *mut ::otter::__codegen::NifEnv,
            __otter_priv_data:  *mut ::std::ffi::c_void,
        ) {
            #unload_dispatch
            unsafe {
                ::otter::__codegen::free_priv_data(
                    __otter_priv_data as *mut ::otter::__codegen::PrivData,
                )
            };
        }
    };

    // --- nif_init entry point ---

    Ok(quote! {
        #register_fn
        #load_wrapper
        #upgrade_wrapper
        #unload_wrapper

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
                load: Some(__otter_load),
                reload: None,
                upgrade: Some(__otter_upgrade),
                unload: Some(__otter_unload),
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
