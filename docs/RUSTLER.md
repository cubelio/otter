# Otter vs Rustler

Otter was designed after studying rustler closely. This document describes the relationship between the two projects — what otter takes from rustler, what it changes, and why.

---

## What rustler is

Rustler is a library for writing Erlang NIFs in Rust. The library is callable from both Erlang and Elixir; rustler's own README states that "Elixir is favored as of now," operationalized through the `rustler_mix` build tool, the `mix rustler.new` getting-started flow, Elixir-flavored examples in the documentation, and Elixir-specific derive macros (`NifStruct`, `NifException`).

Repository: https://github.com/rusterlium/rustler

---

## What otter takes from rustler

### The lifetime safety mechanism

Rustler's core insight — making `Env` invariant over a synthetic lifetime so `TypedTerm` values can't escape a NIF call at compile time with zero runtime cost — is correct and elegant. Otter keeps the invariance (`PhantomData<*mut &'id ()>`) but mints the brand differently. In otter, `Env` and `Term` are sealed *traits*, not structs; each entry point (`with_call_env`, `with_init_env`, …) introduces a fresh, invariant brand `'id` through a `for<'id>` closure — the GhostCell construction — rather than synthesizing a per-call lifetime from a stack borrow. The result is the same escape prevention via a generative brand instead of a per-call lifetime: two independently entered envs carry distinct brands, so a term from one cannot be used with the other.

### The `OwnedEnv` generation token

Rustler uses `Arc<NIF_ENV>`/`Weak<NIF_ENV>` as a generation token to detect use-after-clear of a `SavedTerm` at runtime. Otter keeps the reusable arena shape — `OwnedEnvArena` with `run`/`clear`, `export`/`import`, and a heap-stealing `send` — but replaces the `Arc`/`Weak` token with a process-global monotonic `AtomicU64` stamp: each `OwnedEnvTerm` records the arena-generation that produced it, and `clear` takes a fresh stamp, so a term used after its arena was cleared (or against a different arena) fails a single `u64` compare. No reference counting, no `Weak` upgrade — one atomic increment per generation and one equality check per use.

### The layered architecture

Layering that concentrates unsafety in the lower layers behind a safe public surface is sound. Otter follows the same shape: the external **`enif-ffi`** crate (raw types + the 1:1 `unsafe` shims + the load-time loader — the whole FFI floor) → the safe env-as-receiver layer. Otter consumes enif-ffi as a dependency and adds only the safe layer on top.

### Panic catching at the C boundary

Every NIF wrapper must catch panics via `std::panic::catch_unwind`. A panicking NIF raises a BEAM exception rather than triggering undefined behavior. Otter does the same.

### Dynamic symbol loading

`enif_*` functions are resolved at NIF load time via `dlsym` on Unix and a callback table on Windows. Otter follows the same approach through the `enif-ffi` crate: `enif_ffi::nif_init!` emits the platform-correct entry point and populates the function-pointer table at load — `dlsym` on Unix, the BEAM-supplied callback table on Windows. Both platforms are supported.

---

## What otter changes

The unifying axis: rustler maps BEAM concepts onto Rust shapes. Otter mirrors the NIF C API as it is. Each subsection below makes this concrete.

### Erlang conventions throughout

Rustler's examples, getting-started flow, and derives default to Elixir conventions (`init!("Elixir.MyMod")` in examples, `NifStruct` producing `__struct__`-keyed maps, Mix-native onramp). Otter's surface uses Erlang conventions throughout. The two libraries' technical capabilities overlap; the difference is which conventions are first-class.

### Three term resolution levels

Rustler exposes a single `TypedTerm<'a>` type — a thin wrapper around `NIF_TERM` that defers all type information. Otter exposes three levels:

- `AnyTerm<'id>` — zero work, bare machine word (`Term` is the trait it implements)
- `TypedTerm<'id>` — typed enum, one `enif_term_type` call
- Concrete types (`Integer<'id>`, `Bitstring<'id>`, etc.) — type known, data still lazy

This gives users explicit control over how much work is done at argument receipt.

### List as a cons cell

Rustler exposes lists with an iterator interface. Otter exposes `List<'id>` as a cons cell: `node(env)` decomposes it into `Node::Nil` or `Node::Cell(head, tail)` with one `enif_get_list_cell` — matching Erlang's actual data model. Improper lists are handled naturally (the tail is just another term). An iterator is also offered (`iter`) for the common walk, but the cons cell is the primitive.

### No `Error` enum at the NIF boundary

Rustler has an `Error` enum with five variants: `BadArg`, `Atom(&str)` and `TypedTerm(Box<dyn Encoder>)` (which *return* — the latter as `{error, term}`), and `RaiseAtom(&str)` and `RaiseTerm(Box<dyn Encoder>)` (which *raise*). The same return type encodes two different control-flow behaviors; which one happens depends on which variant you picked.

The NIF C API exposes exactly two exception mechanisms: `enif_make_badarg` and `enif_raise_exception`. Both *raise* — they set a pending exception on the env, which the BEAM raises on return. Otter exposes them as `CallEnv::badarg()` and `CallEnv::raise(reason)`, each returning `Result<T, Raised<'id>>` (always `Err`, generic over the success type). A NIF's idiomatic shape is `Result<T, Raised<'id>>`: `Ok(value)` returns; `Err(Raised)` carries the already-pending exception straight out. `Raised<'id>` is a term-less typestate token — it can only exist *after* a real raise — so exit never re-raises, and there is no double-raise and no enum dispatch. (The encode side mirrors this: a failed `Encoder` raises `badret`, symmetric to the `badarg` a failed decode raises.)

### Explicit NIF registration

Rustler uses the `inventory` crate to collect NIFs. Each `#[rustler::nif]` expands into an `inventory::submit!` that writes a `Nif` record into a linker section (`.init_array` / `.ctors`); at NIF load time, `inventory::iter::<Nif>()` walks that section to discover what was registered. The source code never names the list — registration is whatever survived linking.

Reconstructing compile-time-known facts by walking pre-linked memory regions at runtime is a code smell. There is no compile-time check that all NIFs are registered, no greppable list of what the module exports, and the mechanism depends on the linker preserving inserted symbols across optimization modes and link types.

Otter requires the user to list every NIF explicitly in `init!`. Registration is visible, auditable, and verified at compile time — the way Erlang itself declares NIFs.

### No `static mut` resource registry

Rustler's resource type registry uses a `static mut OnceLock<HashMap<TypeId, usize>>` with suppressed lint warnings. Otter uses a safe alternative.

### Minimum NIF version follows from API usage

Rustler defaults to NIF 2.15 (OTP 22) and exposes Cargo features to opt up to 2.16 or 2.17. Otter requires NIF 2.17 (OTP 26) because the library calls 2.17 APIs (`enif_select_x`, `enif_set_option`, and others) as part of its core surface, with an optional `nif_2_18` feature for 2.18 additions. The version floor in each library follows from which APIs it calls.

### Atom encoding

Default-configured rustler can only create Latin-1 atoms. Passing UTF-8 bytes silently produces the wrong atom — `"é"` becomes `Ã©` (two Latin-1 chars), with no error returned. Enabling the `nif_version_2_17` feature switches to the same `enif_make_new_atom_len` call otter uses unconditionally.

This is one specimen of a broader pattern: rustler papers over the NIF C API with assumed defaults that have hidden edge cases. Otter takes the opposite approach — `Atom::intern` always calls `enif_make_new_atom_len(... ERL_NIF_UTF8)`, no Latin-1 path, and returns `Result<Atom, AtomError>` so the one reachable failure (`NameTooLong`, >255 characters) is a typed error rather than a silent mis-encoding. The BEAM team designed the NIF surface deliberately; otter's job is to reflect it faithfully, not abridge it.

---

## What otter adds

Capabilities in otter that have no equivalent in rustler's current public surface.

### `Bitstring` as a distinct type

Erlang distinguishes byte-aligned binaries from arbitrary-length bitstrings. Otter exposes `Bitstring<'id>` as a separate decodable type from `Binary<'id>`. Rustler's surface only goes through `enif_inspect_binary`; non-byte-aligned bitstrings are not first-class.

### `Port` and `Fun` decode

Otter exposes `Port<'id>` and `Fun<'id>` as decodable term types. Rustler's public surface includes neither — a NIF receiving a port or fun argument keeps it as a generic `AnyTerm<'id>` and operates on it opaquely.

### `enif_select` and `enif_select_x`

Otter wraps `enif_select` and `enif_select_x` for integrating async file descriptors with the BEAM scheduler. Rustler does not expose a safe wrapper.

### `enif_set_option`

Otter wraps `enif_set_option` for tuning per-NIF options such as `delay_halt`. Rustler does not expose this.

### Atoms initialized at NIF load

Otter declares atoms statically via the `atoms = [...]` list in `init!`; the generated load (and upgrade) scaffolding interns them all once and stores each `Atom` in a `OnceLock<Atom>` (set once at load, no term-representation assumption). Rustler's `atoms!` macro caches lazily via `OnceLock::get_or_init` — first call creates them, subsequent calls return the cached value. Both avoid NIF calls in steady state and the retrieval cost is comparable. The difference is structural: otter pushes initialization to load time, rustler defers it to first call.

### `rebar3_otter` build plugin

Otter ships a first-party rebar3 plugin that orchestrates `cargo build` on `rebar3 compile` and places the resulting `.so` where `erlang:load_nif` will find it. Rustler ships `rustler_mix` for the Mix side (which additionally generates the Elixir module stubs from the Rust crate's NIF list, keeping shim and Rust in sync); for Erlang users, rustler ships no build integration and the build glue is hand-rolled.

---

## What otter deliberately excludes

### Elixir-specific derives

`NifStruct` (maps to Elixir structs with `__struct__` key) and `NifException` (Elixir exception structs) have no Erlang equivalent and are not included.

### `NifUntaggedEnum`

Try-each structural dispatch has no idiomatic Erlang equivalent. Users needing to handle multiple term shapes receive a `TypedTerm` and pattern match explicitly.

### Serde integration

Rustler optionally integrates with serde's `Serialize`/`Deserialize` traits. Otter does not. The serde data model does not map cleanly to Erlang terms (no atoms, no records, strings vs binaries). Users implement `Encoder`/`Decoder` directly.

