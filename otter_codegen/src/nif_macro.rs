use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};
use syn::{parse2, Error, FnArg, ItemFn, LitStr, Result, Token};
use syn::parse::{Parse, ParseStream};

// ---------------------------------------------------------------------------
// Attribute parsing
// ---------------------------------------------------------------------------

pub struct NifAttrs {
    pub name: Option<String>,
    pub schedule: Option<String>,
}

impl Parse for NifAttrs {
    fn parse(input: ParseStream) -> Result<Self> {
        let mut name = None;
        let mut schedule = None;

        while !input.is_empty() {
            let key: syn::Ident = input.parse()?;
            input.parse::<Token![=]>()?;
            let value: LitStr = input.parse()?;

            match key.to_string().as_str() {
                "name" => name = Some(value.value()),
                "schedule" => {
                    let s = value.value();
                    match s.as_str() {
                        "DirtyCpu" | "DirtyIo" => schedule = Some(s),
                        _ => {
                            return Err(Error::new_spanned(
                                value,
                                "expected \"DirtyCpu\" or \"DirtyIo\"",
                            ))
                        }
                    }
                }
                _ => return Err(Error::new_spanned(key, "expected `name` or `schedule`")),
            }

            if !input.is_empty() {
                input.parse::<Token![,]>()?;
            }
        }

        Ok(NifAttrs { name, schedule })
    }
}

// ---------------------------------------------------------------------------
// Argument handling
// ---------------------------------------------------------------------------
//
// One rule: the first argument is the env, and every remaining argument is
// decoded from `argv` through `Decoder`. The macro does not classify by
// name — a wrong-type slot 0 surfaces as a normal type error at the user's
// call site (the env is passed straight through to the user function), and
// `TypedTerm` / `Term` / any decodable type go through `Decoder::decode`
// uniformly.

fn arg_ident(arg: &FnArg) -> Result<syn::Ident> {
    let pat_ty = match arg {
        FnArg::Typed(pt) => pt,
        FnArg::Receiver(_) => {
            return Err(Error::new_spanned(arg, "NIF functions cannot have `self`"))
        }
    };
    match &*pat_ty.pat {
        syn::Pat::Ident(pi) => Ok(pi.ident.clone()),
        _ => Err(Error::new_spanned(&pat_ty.pat, "expected a simple identifier")),
    }
}

// ---------------------------------------------------------------------------
// Panic handler
// ---------------------------------------------------------------------------

fn panic_handler() -> TokenStream {
    quote! {
        match ::otter::__codegen::Atom::intern(__otter_env, "nif_panicked") {
            Some(__atom) => __otter_env.raise(
                ::otter::__codegen::Encoder::encode(&__atom, __otter_env)
            ).as_raw(),
            None => __otter_env.raise_badarg().as_raw(),
        }
    }
}

// ---------------------------------------------------------------------------
// Main expansion
// ---------------------------------------------------------------------------

pub fn expand(attr: TokenStream, item: TokenStream) -> Result<TokenStream> {
    let attrs: NifAttrs = parse2(attr)?;
    let func: ItemFn = parse2(item)?;

    let fn_name = &func.sig.ident;
    let wrapper_name = format_ident!("__otter_nif_{}", fn_name);
    let meta_name = format_ident!("__otter_nif_meta_{}", fn_name);

    // --- argument unpacking ---
    //
    // The first argument is always the env. The macro passes `__otter_env`
    // straight through to that slot; if its declared type isn't compatible
    // with `Env<'_>` the user gets a normal type error at the call site.
    // Every remaining argument is decoded from `argv` through `Decoder`.

    let mut inputs = func.sig.inputs.iter();
    let env_arg = inputs.next().ok_or_else(|| {
        Error::new_spanned(
            &func.sig,
            "every `#[otter::nif]` function must take `Env` as its first argument",
        )
    })?;
    // Validate that the first arg is a simple `name: Type` (not `self`,
    // not a pattern destructuring); we don't use the name, but the check
    // gives a clean error if someone writes something unusual.
    let _ = arg_ident(env_arg)?;

    let rest_idents: Vec<syn::Ident> = inputs
        .clone()
        .map(arg_ident)
        .collect::<Result<_>>()?;
    let arity = rest_idents.len() as u32;

    let unpack: Vec<TokenStream> = rest_idents
        .iter()
        .enumerate()
        .map(|(idx, name)| {
            quote! {
                let #name = ::otter::__codegen::Decoder::decode(
                    ::otter::__codegen::new_raw_term(
                        __otter_env,
                        unsafe { *__otter_argv.add(#idx) },
                    ).resolve()
                )?;
            }
        })
        .collect();

    let mut call_args: Vec<TokenStream> = Vec::with_capacity(rest_idents.len() + 1);
    call_args.push(quote! { __otter_env });
    for name in &rest_idents {
        call_args.push(quote! { #name });
    }

    // --- generate return handling ---
    //
    // The user's return value passes through `Encoder::encode`. The macro
    // does not inspect the return type at all; trait dispatch picks the
    // right impl. `Result<T, E>` has its own `Encoder` impl in otter that
    // raises on `Err`, so `Result`-returning NIFs work via the same path as
    // any other return type — no syntactic special-casing.

    let panic_arm = panic_handler();
    let result_handling = quote! {
        match __otter_result {
            Ok(Ok(__val)) => ::otter::__codegen::Encoder::encode(&__val, __otter_env).as_raw(),
            Ok(Err(_))    => __otter_env.raise_badarg().as_raw(),
            Err(_)        => { #panic_arm }
        }
    };

    // --- NIF name and flags ---

    let nif_name = attrs.name.unwrap_or_else(|| fn_name.to_string());
    let nif_name_bytes = syn::LitByteStr::new(
        format!("{}\0", nif_name).as_bytes(),
        Span::call_site(),
    );
    // Emit the otter-exported constants rather than bare literals so the
    // generated flags stay in lockstep with the crate's own definitions.
    let flags = match attrs.schedule.as_deref() {
        Some("DirtyCpu") => quote! { ::otter::__codegen::NIF_FUNC_DIRTY_CPU as u32 },
        Some("DirtyIo") => quote! { ::otter::__codegen::NIF_FUNC_DIRTY_IO as u32 },
        _ => quote! { 0u32 },
    };

    // --- assemble output ---

    Ok(quote! {
        #func

        #[doc(hidden)]
        #[allow(non_snake_case, unused_variables)]
        pub unsafe extern "C" fn #wrapper_name(
            __otter_nif_env: *mut ::otter::__codegen::NifEnv,
            __otter_argc: ::std::ffi::c_int,
            __otter_argv: *const ::otter::__codegen::NifTerm,
        ) -> ::otter::__codegen::NifTerm {
            let __otter_marker = ();
            let __otter_env = unsafe {
                ::otter::__codegen::new_env(
                    &__otter_marker,
                    __otter_nif_env,
                    ::otter::__codegen::EnvKind::ProcessBound,
                )
            };

            // The unpack below reads argv[0..arity) with unchecked pointer
            // offsets. The BEAM always calls a NIF with argc equal to its
            // registered arity, so a mismatch means a registration/ABI bug —
            // fail safe with badarg rather than reading out of bounds.
            if __otter_argc != #arity as ::std::ffi::c_int {
                return __otter_env.raise_badarg().as_raw();
            }

            // Constrain the user fn's return type to `Encoder` here so the
            // diagnostic on a missing impl points at this assertion's bound
            // rather than at the `Encoder::encode` call deep in the wrapper.
            fn __otter_assert_encoder<T: ::otter::__codegen::Encoder>(t: T) -> T { t }

            let __otter_result = ::std::panic::catch_unwind(
                ::std::panic::AssertUnwindSafe(|| {
                    #(#unpack)*
                    Ok::<_, ::otter::__codegen::CodecError>(
                        __otter_assert_encoder(#fn_name(#(#call_args),*))
                    )
                })
            );

            #result_handling
        }

        #[doc(hidden)]
        #[allow(non_upper_case_globals)]
        pub const #meta_name: ::otter::__codegen::NifMeta = ::otter::__codegen::NifMeta {
            name: #nif_name_bytes,
            arity: #arity,
            raw_fptr: #wrapper_name,
            flags: #flags,
        };
    })
}
