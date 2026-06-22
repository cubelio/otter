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

/// A single entry in the `atoms = [...]` list: an identifier, optionally
/// followed by `= "name"` when the BEAM atom name is not a valid Rust
/// identifier. The identifier is the handle used with [`atom!`]; the name is
/// what gets interned.
struct AtomEntry {
    ident: Ident,
    name:  LitStr,
}

impl Parse for AtomEntry {
    fn parse(input: ParseStream) -> Result<Self> {
        let ident: Ident = input.parse()?;
        let name = if input.peek(Token![=]) {
            input.parse::<Token![=]>()?;
            input.parse::<LitStr>()?
        } else {
            LitStr::new(&ident.to_string(), ident.span())
        };
        // The BEAM atom-table limit is 255 characters (MAX_ATOM_CHARACTERS),
        // counted in codepoints — matching `chars()` here. Catching it at
        // expansion makes `StaticAtom::init` infallible for declared atoms.
        if name.value().chars().count() > 255 {
            return Err(Error::new_spanned(
                &name,
                "atom name exceeds 255 characters (MAX_ATOM_CHARACTERS)",
            ));
        }
        Ok(AtomEntry { ident, name })
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
    atoms:       Vec<AtomEntry>,
    resources:   Vec<ResourceEntry>,
    load:        Callback,
    upgrade:     Callback,
    unload:      Callback,
    /// `allow_panic_abort`: opt out of the `panic = "abort"` build guard (see
    /// the guard in `expand`). A bare flag, not a `key = value` entry.
    allow_panic_abort: bool,
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

        let mut atoms = Vec::new();
        let mut seen_atoms = false;
        let mut resources = Vec::new();
        let mut seen_resources = false;
        let mut load = Callback::None;
        let mut upgrade = Callback::None;
        let mut unload = Callback::None;
        let mut allow_panic_abort = false;

        // Remaining arguments are order-independent keyword entries:
        //   atoms = [..], resources = [..], load[_raw] = f, upgrade[_raw] = f,
        //   unload[_raw] = f, allow_panic_abort (a bare flag)
        while input.peek(Token![,]) {
            input.parse::<Token![,]>()?;
            if input.is_empty() {
                break; // tolerate a trailing comma
            }
            let key: Ident = input.parse()?;
            // Bare flags carry no `= value`; handle them before consuming `=`.
            if key == "allow_panic_abort" {
                if allow_panic_abort {
                    return Err(Error::new_spanned(&key, "duplicate `allow_panic_abort`"));
                }
                allow_panic_abort = true;
                continue;
            }
            input.parse::<Token![=]>()?;
            match key.to_string().as_str() {
                "atoms" => {
                    if seen_atoms {
                        return Err(Error::new_spanned(&key, "duplicate `atoms`"));
                    }
                    seen_atoms = true;
                    let content;
                    syn::bracketed!(content in input);
                    atoms = content
                        .parse_terminated(AtomEntry::parse, Token![,])?
                        .into_iter()
                        .collect();
                }
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
                            "unknown init! key `{other}` — expected `atoms`, `resources`, \
                             `load`, `upgrade`, `unload` (or their `_raw` variants), \
                             or the bare flag `allow_panic_abort`"
                        ),
                    ));
                }
            }
        }

        Ok(InitInput {
            module_name, nifs, atoms, resources, load, upgrade, unload, allow_panic_abort,
        })
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
            match ::otter::__codegen::decode_arg(__env, #info) {
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
                match ::otter::__codegen::decode_arg(__env, #info) {
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
            __otter_env:   ::otter::__codegen::InitEnv<'_>,
            __otter_flags: ::otter::__codegen::ResourceFlags,
        ) {
            #register_body
        }
    };

    // --- pre-declared atoms ---
    //
    // The `__otter_atoms` module holds one `StaticAtom` per declared name (the
    // `atom!` macro retrieves them). They are interned in BOTH load and upgrade
    // (see below): an atom term is a VM-global immediate, so each build owns its
    // own statics and re-interns idempotently — no cross-build state, tier-1.
    let atoms_module = if input.atoms.is_empty() {
        quote! {}
    } else {
        let decls = input.atoms.iter().map(|a| {
            let ident = &a.ident;
            let name = &a.name;
            quote! { pub static #ident: StaticAtom = StaticAtom::new(#name); }
        });
        let inits = input.atoms.iter().map(|a| {
            let ident = &a.ident;
            // `init` is infallible here: the name was length-checked at
            // expansion (see AtomEntry::parse), so `NameTooLong` is unreachable.
            quote! {
                #ident.init(__otter_env).expect(concat!(
                    "atom `", stringify!(#ident),
                    "` failed to intern (unreachable: validated at compile time)"
                ));
            }
        });
        quote! {
            #[doc(hidden)]
            #[allow(non_upper_case_globals)]
            pub mod __otter_atoms {
                use ::otter::types::atom::StaticAtom;

                #( #decls )*

                pub fn init(__otter_env: ::otter::__codegen::InitEnv<'_>) {
                    #( #inits )*
                }
            }
        }
    };
    // Interning call spliced into the load/upgrade scaffolding (empty when no
    // atoms are declared, so the module need not exist).
    let intern_atoms = if input.atoms.is_empty() {
        quote! {}
    } else {
        quote! { __otter_atoms::init(__env); }
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
            __otter_load_env:  *mut ::otter::__codegen::ffi::Env,
            __otter_priv_data: *mut *mut ::std::ffi::c_void,
            __otter_load_info: ::otter::__codegen::ffi::Term,
        ) -> ::std::ffi::c_int {
            let __pd = unsafe { ::otter::__codegen::install_priv_data(__otter_priv_data) };
            let __outcome = ::std::panic::catch_unwind(::std::panic::AssertUnwindSafe(|| {
                unsafe {
                    ::otter::__codegen::InitEnv::with_raw(__otter_load_env, |__env| {
                        __otter_register(__env, ::otter::__codegen::ResourceFlags::CREATE);
                        #intern_atoms
                        #load_body
                    })
                }
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
            __otter_upgrade_env: *mut ::otter::__codegen::ffi::Env,
            __otter_priv_data:   *mut *mut ::std::ffi::c_void,
            __otter_old_priv:    *mut *mut ::std::ffi::c_void,
            __otter_upgrade_info: ::otter::__codegen::ffi::Term,
        ) -> ::std::ffi::c_int {
            #upgrade_old_consume
            let __pd = unsafe { ::otter::__codegen::install_priv_data(__otter_priv_data) };
            let __outcome = ::std::panic::catch_unwind(::std::panic::AssertUnwindSafe(|| {
                unsafe {
                    ::otter::__codegen::InitEnv::with_raw(__otter_upgrade_env, |__env| {
                        __otter_register(
                            __env,
                            ::otter::__codegen::ResourceFlags::CREATE
                                | ::otter::__codegen::ResourceFlags::TAKEOVER,
                        );
                        #intern_atoms
                        #upgrade_body
                    })
                }
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
            let _ = ::std::panic::catch_unwind(::std::panic::AssertUnwindSafe(|| {
                unsafe { ::otter::__codegen::DeinitEnv::with_raw(__otter_unload_env, |__env| #f(__env)) }
            }));
        },
        Callback::Raw(f) => quote! {
            let __otter_user = unsafe {
                *::otter::__codegen::user_priv_field(
                    __otter_priv_data as *mut ::otter::__codegen::PrivData,
                )
            };
            let _ = ::std::panic::catch_unwind(::std::panic::AssertUnwindSafe(|| {
                unsafe {
                    ::otter::__codegen::DeinitEnv::with_raw(__otter_unload_env, |__env| #f(__env, __otter_user))
                }
            }));
        },
    };
    let unload_wrapper = quote! {
        #[doc(hidden)]
        unsafe extern "C" fn __otter_unload(
            __otter_unload_env: *mut ::otter::__codegen::ffi::Env,
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

    // --- panic-strategy guard (issue audit-13) ---
    //
    // otter stops a Rust panic in a NIF, resource callback, or load/upgrade hook
    // from crossing the C-ABI boundary into the BEAM by catching it with
    // `catch_unwind` (the wrappers above and in `nif_macro` / `resource`). That
    // only intercepts *unwinding* panics: under `panic = "abort"` a panic calls
    // `abort()` at the panic site and takes down the whole emulator instead.
    //
    // The check lives here, in the macro output, rather than in an otter build
    // script: `panic` is a graph-wide, root-only profile setting, so only the
    // NIF crate's own compilation sees it. `init!` expands into that crate and
    // is invoked exactly once per NIF library, so `cfg(panic = ...)` here
    // resolves against the cdylib's actual strategy and fires exactly once.
    //
    // The `allow_panic_abort` flag on `init!` opts out: an author who accepts
    // that a panic aborts the VM (e.g. NIFs proven panic-free) suppresses the
    // guard at the registration site, where the acknowledgment is visible.
    let panic_guard = if input.allow_panic_abort {
        quote! {}
    } else {
        quote! {
            #[cfg(panic = "abort")]
            const _: () = ::std::compile_error!(
                "otter requires `panic = \"unwind\"`, but this NIF crate is built with \
                 `panic = \"abort\"`. otter keeps a panic in a NIF, resource callback, or \
                 load/upgrade hook from crossing the C-ABI boundary and crashing the BEAM by \
                 catching it with std::panic::catch_unwind, which only works while panics \
                 unwind; `panic = \"abort\"` aborts the whole emulator at the panic site and \
                 silently removes this protection. Remove the `panic = \"abort\"` setting (the \
                 default is \"unwind\") from the [profile.*] section that builds this cdylib, \
                 or pass the `allow_panic_abort` flag to init! to opt out of this check."
            );
        }
    };

    // --- nif_init entry point ---

    Ok(quote! {
        #panic_guard
        #atoms_module
        #register_fn
        #load_wrapper
        #upgrade_wrapper
        #unload_wrapper

        // enif_ffi::nif_init! emits the platform-correct `nif_init` entry point,
        // resolves the enif_* table (dlsym on Unix / the BEAM callback table on
        // Windows), and on success calls this builder. So the builder runs only
        // after the table is live and can use the enif_ffi wrappers freely.
        fn __otter_build_entry() -> *const ::otter::__codegen::ffi::Entry {
            let mut __otter_funcs = ::std::vec![
                #( #meta_paths .to_nif_func() ),*
            ];
            let __otter_funcs_ptr = __otter_funcs.as_mut_ptr();
            ::std::mem::forget(__otter_funcs);

            let __otter_entry = ::std::boxed::Box::new(::otter::__codegen::ffi::Entry {
                major: ::otter::__codegen::ffi::MAJOR_VERSION,
                minor: ::otter::__codegen::ffi::MINOR_VERSION,
                name: #module_name_bytes .as_ptr() as *const ::std::ffi::c_char,
                num_of_funcs: #nif_count as ::std::ffi::c_int,
                funcs: __otter_funcs_ptr,
                load: Some(__otter_load),
                reload: None,
                upgrade: Some(__otter_upgrade),
                unload: Some(__otter_unload),
                vm_variant: ::otter::__codegen::ffi::VM_VARIANT.as_ptr(),
                options: ::otter::__codegen::NIF_ENTRY_OPTIONS as ::std::ffi::c_uint,
                sizeof_resource_type_init: ::std::mem::size_of::<
                    ::otter::__codegen::ffi::ResourceTypeInit
                >(),
                min_erts: ::otter::__codegen::ffi::MIN_ERTS_VERSION.as_ptr(),
            });
            ::std::boxed::Box::leak(__otter_entry) as *const _
        }

        ::otter::nif_init!(__otter_build_entry);
    })
}
