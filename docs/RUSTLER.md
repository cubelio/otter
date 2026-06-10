# Otter vs Rustler

Otter was designed after studying rustler closely. This document describes the relationship between the two projects — what otter takes from rustler, what it changes, and why.

---

## What rustler is

Rustler is a library for writing Erlang NIFs in Rust. It was designed primarily for Elixir and the Mix build system. The rustler maintainers have explicitly stated they do not support Erlang.

Repository: https://github.com/rusterlium/rustler

---

## What otter takes from rustler

### The lifetime safety mechanism

Rustler's core insight — using `PhantomData<*mut &'a u8>` to make `Env<'a>` invariant over `'a`, synthesising a unique per-call lifetime from a stack borrow — is correct and elegant. Otter preserves this mechanism unchanged. It is the right way to prevent `Term` values from escaping a NIF call at compile time with zero runtime cost.

### The `OwnedEnv` / `SavedTerm` pattern

Using `Arc<NIF_ENV>` and `Weak<NIF_ENV>` as a generation token to safely detect use-after-clear at runtime is a clean solution. Otter preserves this.

### The layered architecture

The `sys/` → `wrapper/` → public API layering, with unsafety concentrated in the lower layers and a safe public surface, is sound. Otter follows the same pattern.

### Panic catching at the C boundary

Every NIF wrapper must catch panics via `std::panic::catch_unwind`. A panicking NIF raises a BEAM exception rather than triggering undefined behavior. Otter does the same.

### Dynamic symbol loading

`enif_*` functions are resolved at NIF load time via `dlsym` on Unix and a callback table on Windows. Otter follows the same approach on Unix — the `enif` module holds a complete function pointer table populated by `enif::init()` via `dlsym` at load time. Windows is not supported; the crate emits a `compile_error!` on non-Unix targets.

---

## What otter changes

### Erlang-first, not Elixir-first

Rustler targets Elixir and Mix. Otter targets Erlang and rebar3. This affects naming conventions, the build tooling, and which abstractions are included.

### Three term resolution levels

Rustler exposes a single `Term<'a>` type — a thin wrapper around `NIF_TERM` that defers all type information. Otter exposes three levels:

- `RawTerm<'a>` — zero work, bare machine word
- `Term<'a>` — typed enum, one `enif_term_type` call
- Concrete types (`Integer<'a>`, `Bitstring<'a>`, etc.) — type known, data still lazy

This gives users explicit control over how much work is done at argument receipt.

### List as a cons cell

Rustler exposes lists with an iterator interface. Otter exposes `List<'a>` as a cons cell with `head()` and `tail()` — matching Erlang's actual data model. Improper lists are handled naturally. No iterator abstraction is imposed.

### No `Error` enum at the NIF boundary

Rustler has an `Error` enum with variants that do different things (`Atom` returns a bare atom, `Term` returns `{error, term}`, `RaiseAtom` raises an exception). Otter has no `Error` enum at the NIF boundary. Raising is done via `Env::raise(term)` and `Env::raise_badarg()`, which are direct wrappers of `enif_raise_exception` and `enif_make_badarg`. The NIF C API exposes only these two mechanisms; otter reflects that honestly.

### Explicit NIF registration

Rustler uses the `inventory` crate, which exploits linker sections (`.init_array` / `.ctors`) to automatically collect NIFs at link time. Users annotate functions with `#[rustler::nif]` and never maintain a list. Otter requires the user to list every NIF explicitly in `init!`. This is consistent with how Erlang declares NIFs and makes registration visible and auditable.

### No `static mut` resource registry

Rustler's resource type registry uses a `static mut OnceLock<HashMap<TypeId, usize>>` with suppressed lint warnings. Otter uses a safe alternative.

### Inverted NIF version defaults

Rustler defaults to NIF 2.15 (OTP 22) and users opt up for newer APIs. Otter defaults to the latest known version and users opt down for older OTP compatibility. Most users want the latest; they should not need to think about versioning.

---

## What otter deliberately excludes

### Build tooling for Elixir/Mix

`rustler_mix` is entirely Elixir/Mix-specific. Otter provides `rebar3_otter` instead.

### Elixir-specific derives

`NifStruct` (maps to Elixir structs with `__struct__` key) and `NifException` (Elixir exception structs) have no Erlang equivalent and are not included.

### `NifUntaggedEnum`

Try-each structural dispatch has no idiomatic Erlang equivalent. Users needing to handle multiple term shapes receive a `Term` and pattern match explicitly.

### Serde integration

Rustler optionally integrates with serde's `Serialize`/`Deserialize` traits. Otter does not. The serde data model does not map cleanly to Erlang terms (no atoms, no records, strings vs binaries). Users implement `Encoder`/`Decoder` directly.

### Inventory/linker magic

The `inventory` crate is not a dependency. Registration is explicit.
