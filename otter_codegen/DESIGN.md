# otter_codegen: Proc-Macro Crate

## Purpose

`otter_codegen` provides the procedural macros that eliminate boilerplate when writing NIFs with otter. It generates the C ABI wrapper functions, the NIF entry point, and Encoder/Decoder implementations for user-defined types.

This is a `proc-macro` crate — it runs at compile time and produces Rust token streams. It depends on `otter` for types but is a separate crate because Rust requires proc-macro crates to be isolated.

**Design principle:** The macros generate code the user could write by hand. Nothing is hidden. The generated code is straightforward and auditable.

---

## Macros

### `#[otter::nif]`

Applied to a plain Rust function. Generates the `extern "C"` wrapper required by the NIF ABI.

**Argument type rule:**

One rule: **the first argument is the NIF call environment, and every remaining argument is decoded through `Decoder`.**

- The first parameter is passed through as the `Env<'a>` the BEAM hands to this call. The macro does not inspect its declared type — if it isn't compatible with `Env<'_>`, the user gets a normal Rust type error at the call site.
- Each subsequent parameter is unpacked from `argv` and passed to `Decoder::decode`. A wrong type or decode failure raises `badarg` automatically before the user function is called.
- The first parameter does not count toward the NIF arity. Remaining parameters do.

```rust
// Every NIF takes Env first, even if it doesn't use it.
#[otter::nif]
fn add(_env: Env, a: Integer, b: Integer) -> Integer { a + b }

// Use the env when raising custom exceptions or constructing terms.
#[otter::nif]
fn divide(env: Env, a: Integer, b: Integer) -> TypedTerm {
    match i64::try_from(b) {
        Ok(0)  => env.raise(Atom::new(env, "division_by_zero").unwrap().encode(env)),
        Ok(_)  => (a / b).encode(env),
        Err(_) => env.raise_badarg(),
    }
}

// TypedTerm is a Decoder (no-op resolve), so it flows through the same path.
#[otter::nif]
fn inspect(env: Env, val: TypedTerm) -> Atom {
    match val {
        TypedTerm::Integer(_) => Atom::new(env, "integer").unwrap(),
        TypedTerm::Atom(_)    => Atom::new(env, "atom").unwrap(),
        _                => Atom::new(env, "other").unwrap(),
    }
}
```

The macro does no name-based classification of arguments. A user type named `TypedTerm` decodes through its own `Decoder` impl (or fails to compile cleanly); an env-typed parameter renamed via `use otter::Env as E` works because the type is never inspected by name.

**Return type rule:**

One rule: **the user's return value must implement `Encoder`.** The macro emits a single `Encoder::encode(&val, env)` call with no inspection of the return type. Trait dispatch picks the right impl at compile time.

The interesting impls:

- Every otter term type (`Integer`, `Binary`, `Atom`, `TypedTerm`, `RawTerm`, etc.) implements `Encoder`. `Encoder::encode` returns a `RawTerm<'a>` tied to the call's env.
- `Result<T: Encoder, E: Encoder>` implements `Encoder`: `Ok(v)` encodes `v` and returns the term, `Err(e)` encodes `e`, calls `enif_raise_exception` with the encoded term, and returns the resulting exception term. The BEAM treats this as a class-`error` raise of the encoded reason.

Because the dispatch is by type (not by token-stream string matching on `Result`), a user type that happens to be named `Result` does not silently inherit the raise-on-`Err` behavior — it gets whatever `Encoder` impl it has, or a compile error if none.

If the user's return type does not implement `Encoder`, the macro inserts an explicit bound assertion that surfaces the failure as "the trait `otter::Encoder` is not implemented for `<your type>`" rather than as a `method not found` error deep in the wrapper.

**Input:**
```rust
#[otter::nif]
fn add(_env: Env, a: Integer, b: Integer) -> Integer {
    a + b
}
```

**Generated code (conceptually):**
```rust
unsafe extern "C" fn add_nif(
    nif_env: *mut ErlNifEnv,
    argc: c_int,
    argv: *const NIF_TERM,
) -> NIF_TERM {
    let lifetime = ();
    let env = Env::new(&lifetime, nif_env);

    if argc != 2 { return env.raise_badarg().as_raw(); }

    fn assert_encoder<T: Encoder>(t: T) -> T { t }

    let result = std::panic::catch_unwind(|| {
        let a = Integer::decode(RawTerm::new(env, argv[0]).resolve())?;
        let b = Integer::decode(RawTerm::new(env, argv[1]).resolve())?;
        Ok::<_, CodecError>(assert_encoder(add(env, a, b)))
    });

    match result {
        Ok(Ok(val))  => val.encode(env).as_raw(),
        Ok(Err(_))   => env.raise_badarg().as_raw(),
        Err(_panic)  => env.raise(Atom::new(env, "nif_panicked").unwrap().encode(env)).as_raw(),
    }
}
```

The `?` propagation of `CodecError` is an internal detail of the generated code. The user writes a plain Rust function. Argument decoding, error handling, and panic catching are all handled by the macro.

**Examples of all return type forms:**
```rust
// T: Encoder — macro encodes the return value
#[otter::nif]
fn add(_env: Env, a: Integer, b: Integer) -> Integer { a + b }

// TypedTerm — macro passes it through unchanged
#[otter::nif]
fn identity(_env: Env, val: TypedTerm) -> TypedTerm { val }

// Result — Ok encodes and returns, Err raises as exception
#[otter::nif]
fn divide(env: Env, a: Integer, b: Integer) -> Result<Integer, Atom> {
    if i64::try_from(b)? == 0 {
        Err(Atom::new(env, "division_by_zero").unwrap())
    } else {
        Ok(a / b)
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

**Panic safety:** Every NIF wrapper catches panics via `std::panic::catch_unwind`. A panicking NIF raises a `nif_panicked` atom exception in the calling process rather than crashing the VM.

---

### `otter::init!`

Generates the NIF library entry point — the `nif_init` symbol the BEAM searches for when loading a `.so`.

```rust
otter::init!("my_module", [
    add,
    subtract,
    lookup,
], load = on_load);
```

**The NIF list is explicit.** The user lists every NIF. This is consistent with how Erlang itself declares NIFs and makes the registration visible and auditable.

**Generated entry point:** `extern "C" fn nif_init() -> *const ErlNifEntry`
(Unix only — otter is Unix-only at present; see the core `DESIGN.md`).
`nif_init` first calls `otter::init()` to populate the `enif_*` function
pointers via `dlsym`, then builds and leaks the `ErlNifEntry`.

The generated load callback (emitted only when `load = ...` is given) does one
thing: it calls the user's `load` callback. It receives `(Env, TypedTerm)` — the env
and the `load_info` term passed by `erlang:load_nif/2`. Resource types are
**not** registered automatically; the user does that explicitly inside their
`load` callback (see below).

---

### `#[otter::resource_impl]`

Applied to `impl Resource for T`. Currently a pass-through that validates the impl block. Reserved for future use (e.g. derive-style code generation for resource callbacks).

**Registration is not automatic.** The user explicitly registers each resource
type in their `init!` load callback, by calling the free function
`otter::resource::register_resource_type::<T>(env, name)`:

```rust
fn on_load(env: Env<'_>, _load_info: TypedTerm<'_>) -> bool {
    otter::resource::register_resource_type::<MyResource>(env, "MyResource");
    true
}
```

---

## Code Generation Approach

All macros use `syn` to parse input token streams and `quote` to generate output token streams. The generated code is intentionally straightforward — no clever tricks, no hidden state. If a user wants to understand what the macro produced, they can run `cargo expand` and read plain Rust.

---

## Deferred to v2

- **Derive macros** (`NifRecord`, `NifTuple`, `NifMap`, `NifUnitEnum`, `NifTaggedEnum`) — generate `Encoder`/`Decoder` for user-defined Rust structs and enums. Deferred because: NIF argument and return types are otter term types; native Rust types do not implement `Encoder`/`Decoder`; user-defined struct mapping is a convenience, not a core need.

---

## What is deliberately excluded

- **`NifUntaggedEnum`** — try-each structural dispatch has no Erlang equivalent. Users needing structural dispatch receive a `TypedTerm` and pattern match explicitly.
- **`NifStruct`** — Elixir struct with `__struct__` key. Not an Erlang concept.
- **`NifException`** — Elixir exception struct. Not an Erlang concept.
