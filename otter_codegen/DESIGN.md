# otter_codegen: Proc-Macro Crate

## Purpose

`otter_codegen` provides the procedural macros that eliminate boilerplate when writing NIFs with otter. It generates the C ABI wrapper functions and the NIF entry point, and provides the `#[raw]` visibility-widening attribute otter uses internally. (Derive macros that generate `Encoder`/`Decoder` for user-defined types are deferred to v2 — see below.)

This is a `proc-macro` crate — it runs at compile time and produces Rust token streams. It depends on `otter` for types but is a separate crate because Rust requires proc-macro crates to be isolated.

**Design principle:** The macros generate code the user could write by hand. Nothing is hidden. The generated code is straightforward and auditable.

---

## Macros

### `#[otter::nif]`

Applied to a plain Rust function. Generates the `extern "C"` wrapper required by the NIF ABI.

**Argument type rule:**

One rule: **the first argument is the NIF call environment, and every remaining argument is decoded through `Decoder`.**

- The first parameter is passed through as the `CallEnv<'a>` the BEAM hands to this call. The macro does not inspect its declared type — if it isn't compatible with `CallEnv<'_>`, the user gets a normal Rust type error at the call site.
- Each subsequent parameter is unpacked from `argv` and passed to `Decoder::decode` (with the env). A wrong type or decode failure raises `badarg` automatically before the user function is called.
- The first parameter does not count toward the NIF arity. Remaining parameters do.

```rust
// Atoms used below are declared in init!'s `atoms = [...]` list:
//   atoms = [division_by_zero, integer, atom, other]

// Every NIF takes the call env first, even if it doesn't use it.
#[otter::nif]
fn add<'a>(env: CallEnv<'a>, a: Integer<'a>, b: Integer<'a>) -> Integer<'a> {
    Integer::from_i64(env, a.to_i64(env).unwrap() + b.to_i64(env).unwrap())
}

// Use the env when raising custom exceptions or constructing terms.
#[otter::nif]
fn divide<'a>(env: CallEnv<'a>, a: Integer<'a>, b: Integer<'a>) -> Result<Integer<'a>, Raised<'a>> {
    let (Some(a), Some(b)) = (a.to_i64(env), b.to_i64(env)) else {
        return env.badarg();
    };
    if b == 0 {
        return env.raise(otter::atom![division_by_zero]);
    }
    Ok(Integer::from_i64(env, a / b))
}

// TypedTerm is a Decoder (resolve), so it flows through the same path.
#[otter::nif]
fn inspect(_env: CallEnv, val: TypedTerm) -> Atom {
    match val {
        TypedTerm::Integer(_) => otter::atom![integer],
        TypedTerm::Atom(_)    => otter::atom![atom],
        _                     => otter::atom![other],
    }
}
```

The macro does no name-based classification of arguments. A user type named `TypedTerm` decodes through its own `Decoder` impl (or fails to compile cleanly); an env-typed parameter renamed via `use otter::Env as E` works because the type is never inspected by name.

**Return type rule:**

One rule: **the user's return value must implement `Encoder`.** The macro emits a single `Encoder::encode(&val, env)` call with no inspection of the return type. Trait dispatch picks the right impl at compile time.

The interesting impls:

- Every otter term type (`Integer`, `Binary`, `Atom`, `TypedTerm`, `AnyTerm`, etc.) and the common native Rust types implement `Encoder`. `Encoder::encode` is **fallible** — it returns `Result<AnyTerm<'a>, CodecError>`. Otter term types encode their word for free (always `Ok`); a native value outside the Erlang term domain (e.g. a non-finite `f64`) returns `Err`, which the wrapper turns into a `badret` exception — the encode-side mirror of the `badarg` a failed decode raises.
- `Result<T: Encoder, Raised<'a>>` implements `Encoder`: `Ok(v)` encodes `v`; `Err(Raised)` returns the non-value marker of an *already-pending* exception — the BEAM raises it on return and it is never re-raised. A NIF that can raise returns `Result<T, Raised<'a>>` and produces the `Raised` via `env.raise(reason)` / `env.badarg()`. See `otter/DESIGN.md` "the `Raised` witness".

Because the dispatch is by type (not by token-stream string matching on `Result`), a user type that happens to be named `Result` does not silently inherit this behavior — it gets whatever `Encoder` impl it has, or a compile error if none.

If the user's return type does not implement `Encoder`, the trait bound on the `encode_result` helper surfaces the failure as "the trait `otter::Encoder` is not implemented for `<your type>`" rather than as a `method not found` error deep in the wrapper.

**Input:**
```rust
#[otter::nif]
fn add<'a>(env: CallEnv<'a>, a: Integer<'a>, b: Integer<'a>) -> Integer<'a> {
    Integer::from_i64(env, a.to_i64(env).unwrap() + b.to_i64(env).unwrap())
}
```

**Generated code (conceptually):**
```rust
pub unsafe extern "C" fn __otter_nif_add(
    nif_env: *mut __codegen::ffi::Env,
    argc: c_int,
    argv: *const __codegen::ffi::Term,
) -> __codegen::ffi::Term {
    // `with_call_env` mints a fresh generative brand `'id` for this call through
    // its `for<'id>` closure; the closure's return is the raw word handed back to
    // the C ABI. The user fn, decoded args, and result all share `'id` by
    // inference — the macro never names the user's lifetime.
    unsafe {
        __codegen::with_call_env(nif_env, |env| {
            // The BEAM always calls with argc == the registered arity; a mismatch
            // is a registration/ABI bug — fail safe with badarg.
            if argc != 2 { return __codegen::badarg_word(env); }

            // Decode, call, AND encode all run inside the catch: an
            // `Encoder::encode` panic must not unwind across this `extern "C"`
            // boundary. `decode_arg` decodes argv[i] in `env`; `encode_result`
            // encodes the return and maps an encoder `Err` to `badret_word`.
            let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
                let a = __codegen::decode_arg(env, *argv.add(0))?;
                let b = __codegen::decode_arg(env, *argv.add(1))?;
                let val = add(env, a, b);
                Ok::<_, CodecError>(__codegen::encode_result(&val, env))
            }));

            match result {
                Ok(Ok(word)) => word,                       // all three stages ok
                Ok(Err(_))   => __codegen::badarg_word(env), // an arg `?`-bailed in decode
                Err(_panic)  => /* intern "nif_panicked", raise it */,
            }
        })
    }
}
```

The `?` propagation of `CodecError` is an internal detail of the generated code. The user writes a plain Rust function. Argument decoding, error handling, and panic catching are all handled by the macro. Note a *domain* encode failure (a value that can't become a term) is already turned into `error:badret` inside `encode_result`, so it surfaces as `Ok(Ok(non_value_word))`; the `Err(_panic)` arm is reached only by an actual panic.

**Examples of all return type forms:**
```rust
// T: Encoder — macro encodes the return value
#[otter::nif]
fn add<'a>(env: CallEnv<'a>, a: Integer<'a>, b: Integer<'a>) -> Integer<'a> {
    Integer::from_i64(env, a.to_i64(env).unwrap() + b.to_i64(env).unwrap())
}

// AnyTerm / TypedTerm — Encoder + Decoder, passes through unchanged
#[otter::nif]
fn identity(_env: CallEnv, val: TypedTerm) -> TypedTerm { val }

// Result<T, Raised> — Ok encodes and returns, Err carries the pending raise out
// `division_by_zero` is declared in init!'s `atoms = [...]` list.
#[otter::nif]
fn divide<'a>(env: CallEnv<'a>, a: Integer<'a>, b: Integer<'a>) -> Result<Integer<'a>, Raised<'a>> {
    let (Some(a), Some(b)) = (a.to_i64(env), b.to_i64(env)) else {
        return env.badarg();
    };
    if b == 0 {
        env.raise(otter::atom![division_by_zero])
    } else {
        Ok(Integer::from_i64(env, a / b))
    }
}
```

**Arity:** all arguments after the leading env count toward the NIF arity declared to the BEAM.

**Options:**
```rust
#[otter::nif(schedule = "DirtyCpu")]   // run on dirty CPU scheduler
#[otter::nif(schedule = "DirtyIo")]    // run on dirty I/O scheduler
#[otter::nif(name = "erlang_name")]    // override the exported function name
```

**Panic safety:** Every NIF wrapper runs argument decoding, the NIF body, *and* return-value encoding inside a single `std::panic::catch_unwind`. A panic in any of those three stages — including a panic inside an `Encoder::encode` impl — is caught and raises a `nif_panicked` atom exception in the calling process rather than unwinding across the `extern "C"` boundary (which would be UB) or crashing the VM. The only code outside the catch is the result dispatch, which interns the `"nif_panicked"` atom (`Atom::intern`, falling back to `badarg` if interning somehow fails) and must itself stay panic-free. This protection assumes `panic = "unwind"`; `init!` emits a compile-time guard for the `panic = "abort"` case (see below).

---

### `otter::init!`

Generates the NIF library entry point — the `nif_init` symbol the BEAM searches for when loading a `.so`.

```rust
otter::init!("my_module", [
    add,
    subtract,
    lookup,
], atoms = [ok, error], resources = [MyResource], load = on_load);
```

**The NIF list is explicit.** The user lists every NIF. This is consistent with how Erlang itself declares NIFs and makes the registration visible and auditable. The remaining arguments are order-independent keyword entries: `atoms = [...]`, `resources = [...]`, `load`, `upgrade`, `unload` (each lifecycle slot also has a `_raw` variant — `load_raw`/`upgrade_raw`/`unload_raw`, gated behind otter's `raw` feature; the plain and `_raw` form of a slot are mutually exclusive), plus the bare flag `allow_panic_abort`.

**Generated entry point:** the macro emits a builder fn that constructs and
leaks the `ErlNifEntry`, then invokes `enif_ffi::nif_init!` (re-exported as
`otter::nif_init`). That macro — from the `enif-ffi` crate — emits the
platform-correct `nif_init` symbol (Unix *and* Windows), resolves the `enif_*`
table at load (`dlsym` on Unix / the BEAM-supplied callback table on Windows),
and on success calls the builder. So otter's codegen owns the `ErlNifEntry`
contents while enif-ffi owns the entry-point signature and symbol resolution.
See the core `DESIGN.md` Layer 1.

**`load`/`upgrade`/`unload` are always generated** (non-`NULL`), so every otter
module is hot-upgradeable. Each `load`/`upgrade` wrapper installs otter-owned
`PrivData`, registers the listed `resources` (`CREATE` in load,
`CREATE | TAKEOVER` in upgrade), interns the declared `atoms`, then dispatches
the optional user callback — all under one `catch_unwind`. Any veto (user
`false`, a `load_info` decode failure, or a panic) frees the `PrivData` and
NULLs the slot, returning a distinct `LOAD_FAILED_*` code (`LOAD_FAILED_USER_FALSE`
= 1, `_PANIC` = 2, `_DECODE` = 3 — read the `(N)` in the BEAM's
`{error, {load_failed, "...(N)."}}`). `unload` dispatches the optional user
callback (which cannot veto; a panic is absorbed) and frees the `PrivData`. The
user `load`/`upgrade` fns receive `(InitEnv<'_>, T)` — the init env and the
`load_info` term from `erlang:load_nif/2`, decoded through `Decoder` into any `T`
the user declares (a bad decode vetoes the load) — and return `bool`; `unload`
receives `(DeinitEnv<'_>)`.

**Panic-strategy guard (`allow_panic_abort`).** otter's FFI-boundary protection
relies on `catch_unwind`, which only intercepts *unwinding* panics; under
`panic = "abort"` a panic aborts the whole emulator at the panic site, silently
removing that protection. So `init!` emits a `#[cfg(panic = "abort")] const _: ()
= compile_error!(...)` guard — placed in the macro output (not an otter build
script) because `panic` is a graph-wide, root-only profile setting that only the
cdylib's own compilation sees, and `init!` expands into that crate exactly once.
The bare `allow_panic_abort` flag suppresses the guard for an author who accepts
that a panic aborts the VM (e.g. NIFs proven panic-free); the acknowledgment lives
at the registration site where it is visible.

**Tier-2 `_raw` lifecycle callbacks** (behind otter's `raw` feature). `load_raw`/
`upgrade_raw`/`unload_raw` are the escape hatch for managing the library's own
`user_priv_data` `void*` across a hot upgrade: the user fn is handed `&mut` access
to that pointer (and, for `upgrade_raw`, the previous build's). This is the tier-2
path of the 3-tier `priv_data` plan — see `docs/UPGRADE.md`. Outside the `raw`
feature, an `init!` naming a `_raw` slot is a compile error.

**`atoms = [...]`** generates a hidden `__otter_atoms` module of `StaticAtom`s
(retrieved via the `atom!` macro) and interns them in both load and upgrade.
Because an atom term is a VM-global immediate, each build owns its own statics
and re-interns idempotently — no cross-build state, so atom pre-declaration is
upgrade-safe with no fingerprint or `PrivData` involvement (unlike resources).
Each entry is a Rust identifier (the handle) optionally followed by `= "name"`
when the BEAM atom name is not a valid Rust identifier (e.g. `ok = "ok!"`). Names
are length-checked (≤255 codepoints, `MAX_ATOM_CHARACTERS`) **at macro expansion**,
so the generated `StaticAtom::init` is infallible-by-construction and the
scaffolding `.expect()`s it.

---

### `#[otter::resource_impl]`

Applied to `impl Resource for T`. Currently a pass-through: it parses the `impl` and re-emits it unchanged. Reserved for future use (e.g. derive-style code generation for resource callbacks).

**Registration is list-driven.** The user lists each resource type in
`init!`'s `resources = [...]`, and the generated load/upgrade scaffolding
registers them (see `otter::init!` above). A bare entry `MyResource` registers
under an ABI-suffixed name; `MyResource: "v1"` registers under a stable tagged
name (opting into cross-build takeover). The underlying primitives,
`otter::resource::register::<T>(env, flags)` and `register_tagged`, remain
callable by hand inside a `load`/`upgrade` callback for dynamic cases:

```rust
fn on_load(env: InitEnv<'_>, _load_info: AnyTerm<'_>) -> bool {
    otter::resource::register::<MyResource>(env, ResourceFlags::CREATE);
    true
}
```

---

### `#[raw]`

A visibility-widening attribute, exported as `otter::raw` (doc-hidden — it is an
otter-internal mechanism, not a user-facing macro). Written on an item with its
*real*, non-raw visibility, it emits two `cfg`-gated copies: the original verbatim
under `#[cfg(not(feature = "raw"))]`, and a copy with the visibility forced to
`pub` under `#[cfg(feature = "raw")]`.

```rust
#[raw]
pub(crate) fn from_raw(term: RawTerm) -> Self { … }
// → pub(crate) under not(raw); pub under raw
```

It works by peeling the leading `attrs + Visibility` off the item and re-emitting
the rest verbatim, so it is agnostic to the item kind (fn, method, struct, enum,
type, const, static, mod, use) and to whether a visibility is written at all (a
missing one parses as `Inherited` and becomes `pub`). It takes no arguments. otter
uses it on the `from_raw`/`wrap`/`from_filled` term constructors to expose the
raw↔typed bridge under the `raw` feature without duplicating each item; the `cfg`
resolves in the *using* crate's own compilation, so the macro stays oblivious to
feature state.

---

## Code Generation Approach

All macros use `syn` to parse input token streams and `quote` to generate output token streams. The generated code is intentionally straightforward — no clever tricks, no hidden state. If a user wants to understand what the macro produced, they can run `cargo expand` and read plain Rust.

---

## Deferred to v2

- **Derive macros** (`NifRecord`, `NifTuple`, `NifMap`, `NifUnitEnum`, `NifTaggedEnum`) — generate `Encoder`/`Decoder` for user-defined Rust structs and enums. Deferred because user-defined struct/enum mapping is a convenience, not a core need: otter term types and the common native Rust types (integers, floats, `bool`, `String`, tuples, `Vec`, `HashMap`) already implement `Encoder`/`Decoder`, so the derives would only add field-by-field generation for *user-defined* types.

---

## What is deliberately excluded

- **`NifUntaggedEnum`** — try-each structural dispatch has no Erlang equivalent. Users needing structural dispatch receive a `TypedTerm` and pattern match explicitly.
- **`NifStruct`** — Elixir struct with `__struct__` key. Not an Erlang concept.
- **`NifException`** — Elixir exception struct. Not an Erlang concept.
