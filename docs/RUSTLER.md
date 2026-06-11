# Otter vs Rustler

Otter was designed after studying rustler closely. This document describes the relationship between the two projects — what otter takes from rustler, what it changes, and why.

---

## What rustler is

Rustler is a library for writing Erlang NIFs in Rust. The library is callable from both Erlang and Elixir; rustler's own README states that "Elixir is favored as of now," operationalized through the `rustler_mix` build tool, the `mix rustler.new` getting-started flow, Elixir-flavored examples in the documentation, and Elixir-specific derive macros (`NifStruct`, `NifException`).

Repository: https://github.com/rusterlium/rustler

---

## What otter takes from rustler

### The lifetime safety mechanism

Rustler's core insight — using `PhantomData<*mut &'a u8>` to make `Env<'a>` invariant over `'a`, synthesizing a unique per-call lifetime from a stack borrow — is correct and elegant. Otter preserves this mechanism unchanged. It is the right way to prevent `Term` values from escaping a NIF call at compile time with zero runtime cost.

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

The unifying axis: rustler maps BEAM concepts onto Rust shapes. Otter mirrors the NIF C API as it is. Each subsection below makes this concrete.

### Erlang conventions throughout

Rustler's examples, getting-started flow, and derives default to Elixir conventions (`init!("Elixir.MyMod")` in examples, `NifStruct` producing `__struct__`-keyed maps, Mix-native onramp). Otter's surface uses Erlang conventions throughout. The two libraries' technical capabilities overlap; the difference is which conventions are first-class.

### Three term resolution levels

Rustler exposes a single `Term<'a>` type — a thin wrapper around `NIF_TERM` that defers all type information. Otter exposes three levels:

- `RawTerm<'a>` — zero work, bare machine word
- `Term<'a>` — typed enum, one `enif_term_type` call
- Concrete types (`Integer<'a>`, `Bitstring<'a>`, etc.) — type known, data still lazy

This gives users explicit control over how much work is done at argument receipt.

### List as a cons cell

Rustler exposes lists with an iterator interface. Otter exposes `List<'a>` as a cons cell with `head()` and `tail()` — matching Erlang's actual data model. Improper lists are handled naturally. No iterator abstraction is imposed.

### No `Error` enum at the NIF boundary

Rustler has an `Error` enum with five variants: `BadArg`, `Atom(&str)` and `Term(Box<dyn Encoder>)` (which *return* — the latter as `{error, term}`), and `RaiseAtom(&str)` and `RaiseTerm(Box<dyn Encoder>)` (which *raise*). The same return type encodes two different control-flow behaviors; which one happens depends on which variant you picked.

The NIF C API exposes exactly two exception mechanisms: `enif_make_badarg` and `enif_raise_exception`. Otter exposes those as `Env::raise_badarg()` and `Env::raise(term)` for direct use. The idiomatic shape is a `Result<T, E>` return type where both `T: Encoder` and `E: Encoder`: `Ok(value)` encodes and returns, `Err(reason)` encodes the reason and raises it via `enif_raise_exception`. One behavior per shape, no enum dispatch.

### Explicit NIF registration

Rustler uses the `inventory` crate to collect NIFs. Each `#[rustler::nif]` expands into an `inventory::submit!` that writes a `Nif` record into a linker section (`.init_array` / `.ctors`); at NIF load time, `inventory::iter::<Nif>()` walks that section to discover what was registered. The source code never names the list — registration is whatever survived linking.

Reconstructing compile-time-known facts by walking pre-linked memory regions at runtime is a code smell. There is no compile-time check that all NIFs are registered, no greppable list of what the module exports, and the mechanism depends on the linker preserving inserted symbols across optimization modes and link types.

Otter requires the user to list every NIF explicitly in `init!`. Registration is visible, auditable, and verified at compile time — the way Erlang itself declares NIFs.

### No `static mut` resource registry

Rustler's resource type registry uses a `static mut OnceLock<HashMap<TypeId, usize>>` with suppressed lint warnings. Otter uses a safe alternative.

### Minimum NIF version follows from API usage

Rustler defaults to NIF 2.15 (OTP 22) and exposes Cargo features to opt up to 2.16 or 2.17. Otter requires NIF 2.17 (OTP 26) because the library calls 2.17 APIs (`enif_select_x`, `enif_set_option`, and others) as part of its core surface, with an optional `nif_2_18` feature for 2.18 additions. The version floor in each library follows from which APIs it calls.

---

## What otter adds

Capabilities in otter that have no equivalent in rustler's current public surface.

### `Bitstring` as a distinct type

Erlang distinguishes byte-aligned binaries from arbitrary-length bitstrings. Otter exposes `Bitstring<'a>` as a separate decodable type from `Binary<'a>`. Rustler's surface only goes through `enif_inspect_binary`; non-byte-aligned bitstrings are not first-class.

### `Port` and `Fun` decode

Otter exposes `Port<'a>` and `Fun<'a>` as decodable term types. Rustler's public surface includes neither — a NIF receiving a port or fun argument keeps it as a generic `Term<'a>` and operates on it opaquely.

### `enif_select` and `enif_select_x`

Otter wraps `enif_select` and `enif_select_x` for integrating async file descriptors with the BEAM scheduler. Rustler does not expose a safe wrapper.

### `enif_set_option`

Otter wraps `enif_set_option` for tuning per-NIF options such as `delay_halt`. Rustler does not expose this.

### Atoms initialized at NIF load

Otter's `declare_atoms!` declares atoms statically; `init_atoms!(env)` in the load callback creates them all once and writes the terms to atomics. Rustler's `atoms!` macro caches lazily via `OnceLock::get_or_init` — first call creates them, subsequent calls return the cached value. Both avoid NIF calls in steady state and the retrieval cost is comparable. The difference is structural: otter pushes initialization to load time, rustler defers it to first call.

### `rebar3_otter` build plugin

Otter ships a first-party rebar3 plugin that orchestrates `cargo build` on `rebar3 compile` and places the resulting `.so` where `erlang:load_nif` will find it. Rustler ships `rustler_mix` for the Mix side (which additionally generates the Elixir module stubs from the Rust crate's NIF list, keeping shim and Rust in sync); for Erlang users, rustler ships no build integration and the build glue is hand-rolled.

---

## What otter deliberately excludes

### Elixir-specific derives

`NifStruct` (maps to Elixir structs with `__struct__` key) and `NifException` (Elixir exception structs) have no Erlang equivalent and are not included.

### `NifUntaggedEnum`

Try-each structural dispatch has no idiomatic Erlang equivalent. Users needing to handle multiple term shapes receive a `Term` and pattern match explicitly.

### Serde integration

Rustler optionally integrates with serde's `Serialize`/`Deserialize` traits. Otter does not. The serde data model does not map cleanly to Erlang terms (no atoms, no records, strings vs binaries). Users implement `Encoder`/`Decoder` directly.

