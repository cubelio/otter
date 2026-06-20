# Otter Usage Guide

## Overview

Otter is a Rust library for writing Erlang NIFs. It maps the NIF C ABI directly into Rust types — no abstractions that an Erlang programmer wouldn't recognize.

Three crates work together:

- **`otter`** — the Rust library (types, codecs, environment, resources)
- **`otter_codegen`** — proc macros (`#[otter::nif]` and `otter::init!`)
- **`rebar3_otter`** — rebar3 plugin that drives `cargo build`

You only depend on `otter`. The codegen macros are re-exported through it.

---

## Project Setup

### Cargo.toml

```toml
[package]
name = "my_nifs"
version = "0.1.0"
edition = "2024"

[lib]
crate-type = ["cdylib"]

[dependencies]
otter = { git = "https://github.com/cubelio/otter.git" }
```

The crate must be `cdylib` — this produces a shared library the BEAM can load.

### rebar.config

```erlang
{erl_opts, [debug_info]}.
{plugins, [
    {rebar3_otter, {git_subdir, "https://github.com/cubelio/otter.git", {branch, "master"}, "rebar3_otter"}}
]}.
{provider_hooks, [
    {pre, [{compile, otter_compile}, {clean, otter_clean}]}
]}.
{otter_crates, [
    #{name => my_nifs, path => "native/my_nifs"}
]}.
```

### Erlang module

```erlang
-module(my_nifs).
-on_load(init/0).
-export([add/2]).

init() ->
    erlang:load_nif(filename:join(code:priv_dir(my_app), "native/my_nifs"), 0).

add(_A, _B) -> exit(nif_not_loaded).
```

---

## Core Concepts

### TypedTerm Resolution

Terms are resolved lazily. Each step costs one NIF call, and you only pay for what you use.

```
NifTerm          bare machine word, no metadata
  -> Term     + Env and lifetime, zero work
    -> Option<TypedTerm>   + type tag (one enif_term_type call)
      -> data    extraction methods on concrete types
```

`Term` is what you receive from the BEAM. Call `.resolve()` to get an `Option<TypedTerm>` (typed enum) — `None` only if the term's type is one this otter build does not recognize (a type added by a newer OTP); the `Term` you called it on is still valid to use. Call methods like `i64::try_from(integer)` or `.as_bytes()` to extract actual data. Each step is explicit.

### Env and Lifetimes

`Env<'a>` ties every term to the NIF call that created it. When the NIF returns, the `Env` is gone and no `TypedTerm<'a>` can outlive it. This is enforced at compile time — there is no runtime check.

```rust
#[otter::nif]
fn example(env: Env, val: TypedTerm) -> TypedTerm {
    // env and val share lifetime 'a
    // both are valid until this function returns
    val
}
```

`Env` is `Copy`. Pass it by value everywhere.

### The `#[otter::nif]` Macro

Transforms a Rust function into a NIF. Generates the `extern "C"` wrapper, argument unpacking, panic catching, and return encoding.

```rust
#[otter::nif]
fn add<'a>(env: Env<'a>, a: Integer<'a>, b: Integer<'a>) -> Integer<'a> {
    let sum = i64::try_from(a).unwrap() + i64::try_from(b).unwrap();
    Integer::from_i64(env, sum)
}
```

**Argument types and their cost:**

| Type | What happens | Cost |
|---|---|---|
| `Env<'a>` | Passed through, must be first, does not count toward arity | 0 |
| `Term<'a>` | Wraps argv[i]. `Decoder` is identity | 0 NIF calls |
| `TypedTerm<'a>` | Wraps + `.resolve()` (`enif_term_type`) | 1 NIF call |
| `T: Decoder` (concrete type) | Wraps + `enif_is_*` (or `enif_term_type`) check, badarg on failure | 1 NIF call |

Every argument goes through `Decoder::decode(term: Term<'a>)`. `Term::decode` is the identity (zero cost — pick this when you want the raw word with a lifetime and no type discrimination). `TypedTerm::decode` calls `.resolve()` internally. Concrete-type decoders (`Integer`, `Binary`, `Atom`, …) call the dedicated `enif_is_*` check directly off the `Term`, so each is a single NIF call with no eager discriminator.

**Return type:**

The user's return type must implement `Encoder`. The macro emits a single `Encoder::encode(&val, env).as_raw()` call — no inspection of the return type, no per-shape branching. Trait dispatch picks the right impl:

| Type | What happens |
|---|---|
| `T: Encoder` (any otter term type) | `.encode(env).as_raw()` — one NIF call to build the term, plus the BEAM-bound machine word |
| `Result<T, Raised>` where `T: Encoder` | `Ok(v)` encodes `v` and returns; `Err(Raised)` returns the already-pending exception's marker word (the BEAM raises it on return — never re-raised) |
| `TypedTerm<'a>` / `Term<'a>` | Same path — both implement `Encoder`. Same-env (the macro return path is always same-env) is a zero-copy passthrough; cross-env falls back to `enif_make_copy` |

**Attributes:**

```rust
#[otter::nif(name = "my_name")]           // override NIF name
#[otter::nif(schedule = "DirtyCpu")]      // dirty CPU scheduler
#[otter::nif(schedule = "DirtyIo")]       // dirty I/O scheduler
```

**Lifetime annotations:** When multiple arguments carry lifetimes, Rust's elision rules fail. You must add explicit `<'a>`:

```rust
// Won't compile — ambiguous lifetimes:
fn add(env: Env, a: Integer, b: Integer) -> Integer { ... }

// Correct:
fn add<'a>(env: Env<'a>, a: Integer<'a>, b: Integer<'a>) -> Integer<'a> { ... }
```

Types without lifetimes (`Atom`, `LocalPid`, `LocalPort`) don't need this.

### The `init!` Macro

Registers all NIFs with the BEAM.

```rust
otter::init!("my_module", [add, subtract, hello]);
```

With optional resource types and lifecycle callbacks (all keyword arguments
after the NIF list are order-independent):

```rust
otter::init!("my_module", [add, subtract],
    atoms = [ok, error],        // interned automatically; see "Pre-Declared Atoms"
    resources = [MyResource],   // registered automatically; see "Resources"
    load = on_load);            // also: upgrade = f, unload = f
```

`load` and `upgrade` are `fn(Env, Term) -> bool`; `unload` is `fn(Env)`. otter
always generates the `load`/`upgrade`/`unload` NIF callbacks (even with none of
these arguments), so every otter module is hot-upgradeable.

Under the `raw` feature, the `load_raw`/`upgrade_raw`/`unload_raw` variants
(mutually exclusive with the plain forms) instead hand you the library's
`priv_data` `void*` directly — `&mut *mut c_void` to manage yourself, faithfully
mirroring the enif contract. This is the tier-2 escape hatch for state you need
to carry across a hot upgrade by hand; see `docs/UPGRADE.md`.

**Panic strategy.** otter keeps a panic in a NIF or callback from crossing the
C-ABI boundary and crashing the BEAM by catching it with `catch_unwind`, which
only works while panics unwind. A crate built with `panic = "abort"` aborts the
whole emulator at the panic site, silently removing this protection, so `init!`
fails to compile under that profile. If you accept the trade-off (e.g. NIFs you
have proven panic-free), pass the bare flag `allow_panic_abort` to `init!` to opt
out of the check:

```rust
otter::init!("my_module", [add, subtract],
    load = on_load,
    allow_panic_abort);     // build with panic = "abort"; panics will abort the VM
```

The load callback receives `Env` (with `EnvKind::Load`) and the load info term. The second parameter can be any type that implements `Decoder` — `Term<'a>` is the zero-cost choice when you don't inspect the value, `TypedTerm<'a>` adds an `enif_term_type` call, and a concrete type (e.g. `Integer<'a>`) lets you reject mismatched `LoadInfo` at the type level. Return `true` for success, `false` to abort loading. Panics are caught and treated as failure.

**Load failure return codes.** When the load callback returns non-zero, BEAM aborts the library load and `erlang:load_nif(Path, LoadInfo)` returns `{error, {load_failed, "Library load-call unsuccessful (N)."}}`. The integer `N` carries the cause:

| `N` | Cause |
|---|---|
| 0 | Success (`erlang:load_nif/2` returns `ok`) |
| 1 | User load callback returned `false` |
| 2 | User load callback panicked (caught at the FFI boundary) |
| 3 | `Decoder::decode` rejected the `LoadInfo` term — the type declared for the second parameter of the load callback did not match what `erlang:load_nif/2` was given |

There is no structured channel back to Erlang for the decode-failure reason; the integer is the only signal. The codes live in `otter::__codegen` as named constants (`LOAD_OK`, `LOAD_FAILED_USER_FALSE`, `LOAD_FAILED_PANIC`, `LOAD_FAILED_DECODE`).

---

## Working with Types

### Atom

Atoms are tagged immediates — no lifetime needed. They are valid across environments.

**Declare literal atom names in `init!` and retrieve them with `atom![…]`** — each name is interned exactly once at NIF load and retrieved at use as a single atomic load, with no NIF call:

```rust
otter::init!("my_module", [my_nif],
    atoms = [ok, error, not_found, content_type = "content-type"]);

// In any NIF — zero-cost retrieval:
let ok = otter::atom![ok];
```

Bare identifiers use the identifier as the atom name. For names that aren't valid Rust identifiers, use `ident = "name"` syntax. See the [Pre-Declared Atoms](#pre-declared-atoms) section for details.

**Lookup and inspection:**

```rust
// Look up by name without creating — None if it doesn't exist
let existing = Atom::try_existing(env, "error");

// Extract name
let name: String = ok.name(env);
```

**`Atom::intern(env, name)` exists for the rare case where you need to construct an atom from a runtime string, but read [Atom-table safety](#atom-table-safety) first.**

#### Atom-table safety

The BEAM atom table is global, has a fixed maximum size (default 1,048,576), and **never shrinks** — every interned name persists for the life of the VM. Calling `Atom::intern` on attacker-influenced input (a network protocol field, a binary parsed from a file, etc.) turns each unique string into a permanent atom-table entry. Eventually the table fills, and the entire VM crashes — not just the NIF, the whole node.

This is a well-known BEAM DoS vector. The rule:

- **Never call `Atom::intern` on untrusted strings.** For input handling, use `Atom::try_existing` and treat `None` as "atom not recognized, reject input."
- **For compile-time-known names, declare them in `init!`'s `atoms = [...]`** rather than calling `Atom::intern`. Same atom, but with no chance of leaking growth from a mistaken hot-path call.
- `Atom::intern` returns `Result<Atom, AtomError>`, and `AtomError::NameTooLong` (name over 255 characters) is its **only** failure: a Rust `&str` is always valid UTF-8 so encoding never fails, and atom-table exhaustion does not return an error at all — it aborts the whole VM (`erts_exit`) before `intern` can return. In other words, the table-full case is never something you get to observe and recover from; the only defense is not interning untrusted input in the first place.

### Integer

```rust
// Decode from a TypedTerm
let TypedTerm::Integer(i) = term else { ... };

// Extract value
let val: i64 = i.try_into()?;          // may overflow
let val: u64 = i.try_into()?;         // negative -> overflow
let val: i128 = i.try_into()?;        // covers i64 | u64 range

// Construct
let three = Integer::from_i64(env, 3);
let big = Integer::from_u64(env, u64::MAX);
```

### Float

```rust
let TypedTerm::Float(f) = term else { ... };

// Extract (always succeeds — Erlang floats are f64)
let val: f64 = f.into();

// Construct
let pi = Float::from_f64(env, 3.14159);
```

### Binary

Zero-copy access to BEAM-heap binaries.

```rust
// Decode an argument as Binary (rejects sub-byte bitstrings):
//   fn read<'a>(_env: Env<'a>, bin: Binary<'a>) -> ...
//
// Or refine from a TypedTerm — every binary surfaces as TypedTerm::Bitstring,
// and Bitstring::to_binary refines to Binary if byte-aligned:
let TypedTerm::Bitstring(bs) = term else { ... };
let bin = bs.to_binary().ok_or(...)?;

// Read
let bytes: &[u8] = bin.as_bytes();
let len: usize = bin.len();
let text: &str = bin.try_str()?;      // UTF-8 validation

// Sub-binary (zero-copy slice)
let sub = bin.sub(0, 5);

// Construct from bytes
let new_bin = Binary::from_bytes(env, b"hello");
```

**BinaryBuf** — growable buffer for constructing binaries, mirrors `Vec<u8>`:

```rust
// Append-style (unknown size)
let mut builder = BinaryBuf::new();
builder.extend_from_slice(b"hel");
builder.extend_from_slice(b"lo");
let bin: Binary = builder.into_binary(env);

// Pre-sized with indexed writes (known size)
let mut builder = BinaryBuf::with_capacity(5);
builder.resize(5, 0);
let buf: &mut [u8] = builder.as_bytes_mut();
buf[0] = b'h';
buf[1] = b'e';
buf[2] = b'l';
buf[3] = b'l';
buf[4] = b'o';
let bin: Binary = builder.into_binary(env);
```

`BinaryBuf` allocates via `enif_alloc_binary` and grows via `enif_realloc_binary`. `into_binary()` shrinks to the written length and transfers ownership to the BEAM. If dropped without calling `into_binary()`, the allocation is released. Implements `std::io::Write`.

### Serialize / deserialize (external term format)

`Term::serialize` is the `term_to_binary/1` codec — it works on any term (no `resolve`) and returns a `BinaryBuf`, so you choose whether you want the bytes or an Erlang binary term. `deserialize` is the `binary_to_term/1` inverse.

```rust
// term -> ETF
let buf = term.serialize().expect("serializable");
let bytes: &[u8] = buf.as_bytes();        // use the bytes in Rust
let bin: Binary = buf.into_binary(env);   // ...or hand them back as a binary term

// ETF -> term (safe = reject unknown atoms)
let term: Term = env.deserialize(bytes, true).ok_or(...)?;
let term: Term = binary.deserialize(env, true).ok_or(...)?;
```

### Bitstring

Sub-byte bitstrings. Received via `TypedTerm::Bitstring`. No inspection API exists in the NIF interface — you can pass them through or encode them, but you cannot read the bits.

### List

Lists in the BEAM are cons cells or nil (`[]`). Use `iter()` to walk a list:

```rust
// Sum all integers in a list
let sum: i64 = list.iter()
    .filter_map(|raw| match raw.resolve() {
        TypedTerm::Integer(i) => Some(i64::try_from(i).unwrap()),
        _ => None,
    })
    .sum();
```

`iter()` yields heads as `Term` — one `enif_get_list_cell` per step. After iteration, call `tail()` to inspect the terminal value:

```rust
let mut iter = list.iter();
for head in &mut iter {
    // process head.resolve()
}
match iter.tail().unwrap() {
    TypedTerm::List(_) => { /* proper list — tail is [] */ }
    other => { /* improper list — tail is some other term */ }
}
```

For low-level decomposition, `node()` gives direct access to the cons cell:

```rust
use otter::types::Node;

match list.node() {
    Node::Nil => { /* empty list [] */ }
    Node::Cell(head, tail) => {
        // head and tail are Term — resolve when needed
    }
}
```

**Constructing lists:**

```rust
// From a slice of terms
let list = List::from_terms(env, &[term1, term2, term3]);

// From a UTF-8 string (creates a list of codepoints)
let charlist = List::from_str(env, "hello");

// Cons cell
let cell = List::cons(env, head_term, tail_term);

// List length (O(n), None for improper lists)
let len: Option<usize> = list.len();

// Reverse (None for improper lists)
let rev: Option<List> = list.reverse();

// Collect codepoints into a String
let s: String = list.try_string()?;
```

### Tuple

```rust
let TypedTerm::Tuple(tup) = term else { ... };

// Arity
let len: usize = tup.len();

// Element access (0-indexed)
let first: TypedTerm = tup.element(0);
let second: TypedTerm = tup.element(1);

// Construct from a slice
let tup = Tuple::from_terms(env, &[term1, term2]);
```

### Map

```rust
let TypedTerm::Map(map) = term else { ... };

// Size
let n: usize = map.size();

// Lookup — accepts any AsNifTerm (Atom, Integer, TypedTerm, etc.)
let val: Option<Term> = map.get(atom_key);

// Insert (returns a new map — maps are immutable)
let map2: Map = map.put(atom_key, integer_val);

// Update existing key (None if key not found)
let map3: Option<Map> = map.update(atom_key, new_val);

// Remove (None if key not found)
let map4: Option<Map> = map.remove(atom_key);

// Construct empty
let empty = Map::new(env);

// Iterate
for (key, value) in map.iter() {
    // key and value are Term<'a> — call .resolve() to type them
}
```

### Pid

`Pid<'a>` is a pid of unestablished locality, tied to its env: an external
(remote-node) pid is heap-boxed, so it must not outlive `'a`. It supports
identity and encoding. To *act* on the process, refine it to a `LocalPid`
(`Copy`, no lifetime, storable) with `to_local()` — only local processes can
be sent to, monitored, or checked. NIF arguments can be decoded directly as
`LocalPid` (an external pid then fails with badarg).

```rust
let TypedTerm::Pid(pid) = term else { ... };   // pid: Pid<'a>
let Some(local) = pid.to_local() else { ... }; // None if external

// Current process — always local
let self_pid = LocalPid::self_(env);

// Liveness check / registered-name lookup
let alive: bool = local.is_alive(env);
let pid = LocalPid::whereis(env, name_atom);

// Send (in-NIF): a method on the recipient pid, with the caller env
local.send_from(env, msg_term);
```

### Port

Symmetric to `Pid`: `Port<'a>` (any port) refines to `LocalPort` via
`to_local()`; `enif_port_command`/`enif_is_port_alive` take a local port.

```rust
let TypedTerm::Port(port) = term else { ... };   // port: Port<'a>
let Some(local) = port.to_local() else { ... };

let port = LocalPort::whereis(env, name_atom);

// Send a command — takes a &LocalPort
let ok: bool = env.port_command(&local, msg_term);
```

### Fun

Received via `TypedTerm::Fun`. Can be passed through or encoded, but there is no NIF API to call or inspect funs.

### Reference

```rust
let TypedTerm::Reference(r) = term else { ... };

// Create a new unique reference
let new_ref = Reference::new(env);
```

---

## Pre-Declared Atoms

For atoms used frequently across NIFs, pre-declaration avoids repeated `Atom::intern` calls. Pre-declared atoms are interned once at NIF load time and retrieved thereafter as a single atomic load — no NIF call, no lookup.

### Step 1: Declare

List the atoms you need in the `atoms = [...]` argument of [`init!`](#registration), alongside your NIFs and resources:

```rust
otter::init!("my_module", [my_nif], atoms = [ok, error, not_found]);
```

For atom names that are not valid Rust identifiers, use `ident = "name"` syntax:

```rust
otter::init!("my_module", [my_nif], atoms = [ok, error, content_type = "content-type"]);
```

This generates a hidden `__otter_atoms` module containing one `StaticAtom` per entry, which the load scaffolding interns automatically — there is no separate initialization step to remember.

### Step 2: Use

Retrieve any declared atom by name:

```rust
#[otter::nif]
fn example(_env: Env) -> Atom {
    otter::atom![ok]
}
```

`atom!` returns an `Atom` — it works anywhere an `Atom` is expected.

### What the macro generates

`init!` generates code you could write by hand. Nothing is hidden:

```rust
// atoms = [ok, error, content_type = "content-type"] expands to:
mod __otter_atoms {
    use otter::types::atom::StaticAtom;

    pub static ok: StaticAtom = StaticAtom::new("ok");
    pub static error: StaticAtom = StaticAtom::new("error");
    pub static content_type: StaticAtom = StaticAtom::new("content-type");

    pub fn init(env: otter::env::Env<'_>) {
        ok.init(env);
        error.init(env);
        content_type.init(env);
    }
}

// otter::atom![ok]  →  __otter_atoms::ok.get()
```

The generated load **and** upgrade callbacks call `__otter_atoms::init(env)` before dispatching your own `load`/`upgrade` callback, so the atoms are ready before any NIF runs. `StaticAtom::get()` is a single `AtomicUsize` load with `Relaxed` ordering; it panics (in release builds too) if called before init.

### Hot upgrade

Atom pre-declaration is upgrade-safe with no extra machinery. An atom term is a VM-global tagged immediate — an index into the global atom table, which never shrinks and outlives every code version. So each build owns its own `__otter_atoms` statics, and the scaffolding re-interns them in the **upgrade** callback exactly as it does in load. Re-interning hits the existing table entries (idempotent, cheap) and touches no cross-build state. Nothing is shared across the upgrade boundary, so atoms never participate in the ABI concerns that govern `priv_data` and resource payloads.

### Notes

A few rules follow from what the macro expands to:

- **Declaration lives in `init!`.** All your literal atoms go in one `atoms = [...]` list. The generated `__otter_atoms` module is emitted at the `init!` site (conventionally the crate root). For full manual control over the declaration site or interning timing, construct `StaticAtom`s yourself and call `init` from your `load`/`upgrade` callback.
- **`atom![…]` resolves `__otter_atoms` by ordinary name lookup, so it must be in scope.** In the module that invokes `init!`, it's already in scope. To use the atoms from a sibling or descendant module, bring the module in with a `use` statement:

  ```rust
  // lib.rs — invokes init! here:
  otter::init!("my_module", [my_nif], atoms = [ok, error, not_found]);
  mod handlers;

  // handlers.rs — uses the atoms from a sibling module:
  use crate::__otter_atoms;     // bring the generated module into scope

  pub fn handle() -> otter::types::Atom {
      otter::atom![ok]          // resolves to __otter_atoms::ok.get()
  }
  ```

  The `__` prefix marks `__otter_atoms` as framework-generated, but the module is `pub` and the `use` line is ordinary Rust — bring it in wherever you need `atom![…]`.
- **Non-identifier names need `ident = "name"`.** For hyphens, leading digits, reserved words, non-ASCII — pick a valid identifier and map it to the string you want:

  ```rust
  otter::init!("my_module", [my_nif], atoms = [ok, content_type = "content-type"]);
  let ct = otter::atom![content_type];  // the atom "content-type"
  ```

- **Duplicates.** Two entries with the same identifier are a compile error. Two different identifiers mapped to the same string (`ok` and `okay = "ok"`) are fine — both intern the same BEAM atom and compare equal.
- **Atom name length.** Erlang atoms cap at 255 characters; over-length names fail at load, not mid-NIF.
- **Thread- and env-safe.** `atom![…]` is safe from any scheduler thread, including dirty NIFs. The returned `Atom` is valid in any environment, including a process-independent one (e.g. an `OwnedTermBuilder`'s).

---

## Encoder and Decoder

All otter term types implement `Encoder` and `Decoder`, and so do the common native Rust types — integers, floats, `bool`, `str`/`String`, tuples (arity 1–12), `Vec<T>`, and `HashMap<K, V>` — so a NIF can take and return them directly. These traits are what the `#[otter::nif]` macro uses for automatic argument decoding and return encoding. Both directions are fallible: a failed decode on an argument raises `badarg`, a failed encode on a return raises `badret` (e.g. returning a non-finite `f64`). otter term types never fail to encode; only the native conversions can.

The native integer codecs cover `i8`…`i64`/`isize` and `u8`…`u64`/`usize`; an integer outside the target type's range fails to decode (`badarg`). To read or write **arbitrary-precision integers** — Erlang bignums beyond `i64`/`u64` — enable the off-by-default `bigint` feature and use `otter::num_bigint::BigInt`, which then implements `Encoder`/`Decoder`. `BigInt` round-trips every Erlang integer (it goes through the external term format for the >64-bit cases, since the NIF API has no bignum accessor); the same conversions are available directly as `Integer::to_bigint(env)` and `Integer::from_bigint(env, &big)`. Name `BigInt` through otter's re-export (`otter::num_bigint`) so your NIF shares otter's exact `num-bigint` version — the trait impls are tied to it.

```rust
pub trait Encoder {
    fn encode<'a>(&self, env: Env<'a>) -> Term<'a>;
}

pub trait Decoder<'a>: Sized {
    fn decode(term: Term<'a>) -> Result<Self, CodecError>;
}
```

`Decoder::decode` is called on `Term<'a>` — the env-bound wrapper around a raw NIF word, with no type tag attached. Each impl calls its own type-specific `enif_is_*` (or `enif_term_type`) check directly, so a decode is one NIF call regardless of which concrete type you ask for. If the term doesn't match the expected type, it returns `CodecError::WrongType` and the generated wrapper converts that to a `badarg` exception.

`Encoder::encode` converts a value back into a `Term` tied to the target env's lifetime. For types that already hold a NIF term (like `Integer`, `Binary`), the impl compares the source and target env pointers: same-env is a zero-copy passthrough; cross-env falls back to `enif_make_copy`. The macro return path is always same-env, so the common case is free.

`Result<T, Raised>` implements `Encoder`: `Ok(v)` encodes `v`; `Err(Raised)` returns the already-pending exception's marker word (the BEAM raises it on return — never re-raised). This is how `Result`-returning NIFs raise — through normal trait dispatch on the return type, not any macro-level special case. See [Raising exceptions](#raising-exceptions-raised-and-resultt-raised). A user type happening to be called `Result` does not inherit this behavior.

**CodecError variants:**

| Variant | Meaning |
|---|---|
| `WrongType` | The term is not the expected type |
| `IntegerOverflow` | Integer doesn't fit in the target Rust integer type |
| `NotFinite` | A non-finite `f64`/`f32` cannot be encoded as an Erlang float (encode side) |
| `FloatRange` | A finite float is outside the target Rust float's range (`f32` decode) |
| `NotUtf8` | A binary's bytes are not valid UTF-8, or a list is not a valid string |
| `WrongArity` | An Erlang tuple's arity does not match the Rust tuple type |
| `UnknownTermType` | The term's type code is from a newer OTP than this otter build knows |

---

## Error Handling

### Raising exceptions: `Raised` and `Result<T, Raised>`

The NIF C API has exactly two exception mechanisms — `enif_make_badarg` and `enif_raise_exception` — and both *raise on the spot*: they set a pending exception on the environment and the BEAM raises it when the NIF returns. While an exception is pending, any further environment operation is undefined behaviour.

Otter models this with [`Raised<'a>`]: an opaque value that can only be produced by an operation that actually raised, so holding one is proof the env is already in the pending-exception state. You produce one with `Env::raise_exception` or `Env::make_badarg`, and propagate it out of the NIF:

```rust
// `division_by_zero` is declared in init!'s `atoms = [...]` list.

#[otter::nif]
fn divide<'a>(env: Env<'a>, a: Integer<'a>, b: Integer<'a>) -> Result<Integer<'a>, Raised<'a>> {
    let bv = i64::try_from(b).unwrap();
    if bv == 0 {
        return env.raise_exception(otter::atom![division_by_zero]);
    }
    let av = i64::try_from(a).unwrap();
    Ok(Integer::from_i64(env, av / bv))
}
```

Both primitives always fail and are generic over the success type, so they slot into any position — `raise_exception` accepts any `impl AsNifTerm<'a>` as the reason:

```rust
return env.make_badarg();                       // enif_make_badarg
return env.raise_exception(otter::atom![oops]); // enif_raise_exception

// in a `let`-`else` (the `return` makes the arm diverge):
let TypedTerm::Tuple(t) = term else { return env.make_badarg() };

// bridging a fallible decode to badarg:
let n: i64 = i64::try_from(int).or_else(|_| env.make_badarg())?;
```

A NIF's error type is always `Raised`. At return, the `Encoder` for `Result<T, Raised>` hands the marker word straight back — it never *re-*raises, because the exception is already pending — so propagating a `Raised` out of a NIF is sound even though the env is in the exception state.

### Builders that can raise, and `check_raised`

A few enif functions raise on bad input. The only one on otter's safe surface is `enif_make_double`, which raises `badarg` for `NaN`/infinity, so `Env::make_double` and `Float::from_f64` return `Result<Float<'a>, Raised<'a>>`:

```rust
let f = Float::from_f64(env, x)?;   // Err(Raised) if x is not finite
```

To call a `raw`-surface enif function that may raise and handle it the same way, pass its result through [`Env::check_raised`], which tests `enif_has_pending_exception` and returns `Err(Raised)` if one is pending.

---

## Resources

Resources let you own Rust data from the BEAM side. The BEAM manages the lifetime via reference counting — when no Erlang term references the resource, the destructor runs.

> **Hot upgrade caveat.** A resource's Rust payload is not assumed to survive a code upgrade across non-identical builds. Outside the `raw` feature, otter never assumes two builds share an allocator or a datatype layout — see `docs/UPGRADE.md` and `otter/DESIGN.md` "Core safety invariant". By default a resource type's BEAM-side name carries a hash of this build's binary, so a different build does *not* take over its resources; opt a type into cross-build takeover with `resources = [MyState: "v1"]` (a promise that its layout is stable).

### Defining a resource

```rust
use otter::resource::{Resource, ResourceArc};

struct MyState {
    counter: std::sync::atomic::AtomicU64,
}

impl Resource for MyState {}
```

The `Resource` trait has no required methods — `destructor`, `down`, and `stop`
are all optional (see below).

### Registering

List resource types in `init!`; otter registers them in the generated load and
upgrade callbacks:

```rust
otter::init!("my_module", [create, increment, read],
    resources = [MyState],
    load = on_load);
```

The registered type pointer lives in the library's private data, keyed by
`TypeId`. To opt a type into cross-build hot-upgrade takeover, give it a stable
tag: `resources = [MyState: "v1"]`. For dynamic cases you can still register
by hand inside `load`/`upgrade` with
`otter::resource::register::<MyState>(env, ResourceFlags::CREATE)`.

### Creating and using

Construction takes the env, which looks the type up in the registry:

```rust
#[otter::nif]
fn create(env: Env) -> ResourceArc<MyState> {
    env.make_resource(MyState {
        counter: std::sync::atomic::AtomicU64::new(0),
    })
}

#[otter::nif]
fn increment(_env: Env, state: ResourceArc<MyState>) -> Atom {
    state.counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    otter::atom![ok]
}

#[otter::nif]
fn read<'a>(env: Env<'a>, state: ResourceArc<MyState>) -> Integer<'a> {
    let val = state.counter.load(std::sync::atomic::Ordering::Relaxed);
    Integer::from_u64(env, val)
}
```

`ResourceArc<T>` implements `Deref<Target=T>`, `Encoder`, `Decoder`, `Clone`, and `Drop`. It is `Send + Sync`.

To create a resource off a NIF thread (e.g. inside an `OwnedTermBuilder` worker, where
`enif_priv_data` is unavailable), capture a `Send` handle from a module-bound
env first, then use it on the worker thread:

```rust
let handle = env.resource_handle::<MyState>();   // Send + Sync
std::thread::spawn(move || {
    let arc = handle.make(MyState {
        counter: std::sync::atomic::AtomicU64::new(0),
    });
    // ...
});
```

### Destructors and monitors

```rust
impl Resource for MyState {
    fn destructor(self, _env: Env<'_>) {
        // cleanup when reference count hits zero
    }

    fn down<'a>(&'a self, _env: Env<'a>, _pid: LocalPid, _monitor: Monitor) {
        // a monitored process went down
    }
}
```

### Monitoring processes

```rust
let monitor: Option<Monitor> = resource_arc.monitor(Some(env), &pid);
let success: bool = resource_arc.demonitor(Some(env), &monitor);
```

`Monitor` implements `PartialEq`/`Eq` and can be converted to a term with `monitor.to_term(env)`.

---

## OwnedTermBuilder and Message Passing

`OwnedTermBuilder` lets you build a term and send it to a process from outside a NIF call — typically from a spawned OS thread. Build terms on its environment, choose one as the message with `set`, `build` it into an `OwnedTerm`, then deliver it with `pid.send_owned(...)`.

```rust
use std::thread;
use otter::codec::Encoder;                       // for `.encode(...)`
use otter::env::OwnedTermBuilder;

#[otter::nif]
fn start_worker(env: Env) -> Atom {
    let pid = LocalPid::self_(env);
    thread::spawn(move || {
        let builder = OwnedTermBuilder::new();
        let result = do_heavy_work();
        let env = builder.env();
        builder.set(Integer::from_i64(env, result).encode(env));
        pid.send_owned(builder.build());          // off-thread: no caller env
    });
    otter::atom![ok]  // assuming `ok` is pre-declared
}
```

The sends are methods on the recipient [`LocalPid`], split by whether you hold a caller env:

| | copy | steal an `OwnedTerm` |
|---|---|---|
| off-thread (no caller env) | `pid.send(msg)` | `pid.send_owned(owned)` |
| in a NIF (caller env) | `pid.send_from(env, msg)` | `pid.send_owned_from(env, owned)` |

Terms are built on `builder.env()` and borrow the builder, so they cannot outlive it. `set` records the message — it must have been built in this builder's env (checked) — and `build` consumes the builder into a sendable `OwnedTerm`. A successful `send_owned` *steals* the builder's heap into the message, so the builder is single-use: there is no reuse-and-clear, and no off-thread `port_command` (`enif_port_command` aborts the VM when its caller env is NULL, and a non-scheduler thread has no process env to supply).

**From inside a NIF**, you already hold the process env, so copy a term directly — no `OwnedTermBuilder` needed:

```rust
#[otter::nif]
fn notify<'a>(env: Env<'a>, to: LocalPid, msg: TypedTerm<'a>) -> Atom {
    to.send_from(env, msg);   // msg is copied into to's mailbox
    otter::atom![ok]
}
```

`send_from` returns `true` if the target was alive. The matching port operation is the existing `Env::port_command`.

---

## Scheduling

### Dirty NIFs

For long-running work, schedule on dirty schedulers to avoid blocking normal ones:

```rust
#[otter::nif(schedule = "DirtyCpu")]
fn heavy_compute(env: Env) -> Integer {
    // CPU-bound work
}

#[otter::nif(schedule = "DirtyIo")]
fn read_file(env: Env, path: Binary) -> Binary {
    // I/O-bound work
}
```

### Rescheduling

For work that may exceed a timeslice, check and reschedule:

```rust
let exhausted: bool = env.consume_timeslice(50); // 50% consumed
```

For explicit rescheduling to a different scheduler type, use `env.schedule_nif()` (unsafe — requires a valid NIF function pointer and argument array).

---

## Time

```rust
use otter::time::{monotonic_time, time_offset, convert_time_unit, TimeUnit};

let now = monotonic_time(TimeUnit::Nanosecond);
let offset = time_offset(TimeUnit::Nanosecond);
let wall_clock = now + offset;

let ms = convert_time_unit(now, TimeUnit::Nanosecond, TimeUnit::Millisecond);
```

These map directly to `erlang:monotonic_time/1`, `erlang:time_offset/1`, and `erlang:convert_time_unit/3`.

---

## System Information

```rust
use otter::system::{thread_type, ThreadType};

match thread_type() {
    ThreadType::Scheduler => { /* normal scheduler thread */ }
    ThreadType::DirtyCpu => { /* dirty CPU scheduler */ }
    ThreadType::DirtyIo => { /* dirty I/O scheduler */ }
    ThreadType::NonScheduler => { /* not a scheduler thread */ }
    ThreadType::Unknown(n) => { /* future thread type */ }
}
```

---

## enif-backed global allocator

By default Rust allocations in your NIF use Rust's own global allocator. You can
instead route them through the BEAM allocator (`enif_alloc`/`enif_free`) by
installing otter's [`EnifAlloc`](https://docs.rs/otter) as the global allocator —
invoke the macro once in your cdylib:

```rust
otter::enif_global_allocator!();
```

Why bother: `enif_free` is the one free path valid across two independently
compiled builds of the library, so routing allocations through it is a building
block for carrying state across a hot upgrade (see `docs/UPGRADE.md`). The macro
is the only opt-in — `EnifAlloc` is otherwise inert and otter still links into
ordinary binaries. Once installed, the crate links **only** as a BEAM-hosted
cdylib (it direct-links `enif_alloc`/`enif_free`, which the VM resolves at load).

---

## I/O Select

For integrating OS-level I/O events (file descriptors, sockets) with the BEAM scheduler. Requires a resource to own the event lifecycle.

```rust
use otter::select;

// Register interest in a file descriptor
let result = select::select(
    env,
    fd,                         // OS event (fd on Unix)
    NifSelectFlags::READ,       // interest flags
    &resource_arc,              // resource that owns this event
    &pid,                       // process to notify
    ref_term,                   // reference for matching notifications
);
```

The BEAM sends a message to `pid` when the event fires. Use `NifSelectFlags::READ`, `WRITE`, `ERROR`, `CANCEL`, and `STOP` flags.

`select_x` is the extended version that allows custom messages and a message environment.

---

## Complete Example

```rust
use otter::env::Env;
use otter::term::TypedTerm;
use otter::types::{Atom, Binary, BinaryBuf, Integer, List};

#[otter::nif]
fn hello(_env: Env) -> Atom {
    otter::atom![world]
}

#[otter::nif]
fn add<'a>(env: Env<'a>, a: Integer<'a>, b: Integer<'a>) -> Integer<'a> {
    let sum = i64::try_from(a).unwrap() + i64::try_from(b).unwrap();
    Integer::from_i64(env, sum)
}

#[otter::nif]
fn echo(_env: Env, val: TypedTerm) -> TypedTerm {
    val
}

#[otter::nif]
fn reverse_binary<'a>(env: Env<'a>, bin: Binary<'a>) -> Binary<'a> {
    let bytes = bin.as_bytes();
    let mut builder = BinaryBuf::with_capacity(bytes.len());
    for &b in bytes.iter().rev() {
        builder.push(b);
    }
    builder.into_binary(env)
}

#[otter::nif]
fn sum_list<'a>(env: Env<'a>, list: List<'a>) -> Integer<'a> {
    let sum: i64 = list.iter()
        .filter_map(|raw| match raw.resolve() {
            TypedTerm::Integer(i) => Some(i64::try_from(i).unwrap()),
            _ => None,
        })
        .sum();
    Integer::from_i64(env, sum)
}

otter::init!("my_nifs", [hello, add, echo, reverse_binary, sum_list],
    atoms = [world, ok]);
```
