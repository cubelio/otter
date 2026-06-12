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
    -> TypedTerm      + type tag (one enif_term_type call)
      -> data    extraction methods on concrete types
```

`Term` is what you receive from the BEAM. Call `.resolve()` to get a `TypedTerm` (typed enum). Call methods like `i64::try_from(integer)` or `.as_bytes()` to extract actual data. Each step is explicit.

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
| `Result<T, E>` where `T: Encoder, E: Encoder` | `Ok(v)` encodes `v` and returns; `Err(e)` encodes `e` and raises it as a class-`error` exception |
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

Types without lifetimes (`Atom`, `Pid`, `Port`) don't need this.

### The `init!` Macro

Registers all NIFs with the BEAM.

```rust
otter::init!("my_module", [add, subtract, hello]);
```

With an optional load callback:

```rust
fn on_load(env: Env, _load_info: Term) -> bool {
    otter::init_atoms!(env);  // initialize pre-declared atoms
    otter::resource::register_resource_type::<MyResource>(env);
    true
}

otter::init!("my_module", [add, subtract], load = on_load);
```

The load callback receives `Env` (with `EnvKind::Init`) and the load info term. The second parameter can be any type that implements `Decoder` — `Term<'a>` is the zero-cost choice when you don't inspect the value, `TypedTerm<'a>` adds an `enif_term_type` call, and a concrete type (e.g. `Integer<'a>`) lets you reject mismatched `LoadInfo` at the type level. Return `true` for success, `false` to abort loading. Panics are caught and treated as failure.

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

**Use `declare_atoms!` + `atom![…]` for literal atom names** — it interns each name exactly once at NIF load and retrieves it at use as a single atomic load, with no NIF call:

```rust
otter::declare_atoms![ok, error, not_found, content_type = "content-type"];

fn on_load(env: Env, _load_info: Term) -> bool {
    otter::init_atoms!(env);
    true
}

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
- **For compile-time-known names, use `declare_atoms!`** rather than `Atom::intern`. Same atom, but with no chance of leaking growth from a mistaken hot-path call.
- `Atom::intern` returns `None` if the atom table is full or `name` is not valid UTF-8. The full case is a soft signal that something has been mishandled upstream — by the time you observe it, the VM is close to crashing.

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
// and Bitstring::try_into_binary refines to Binary if byte-aligned:
let TypedTerm::Bitstring(bs) = term else { ... };
let bin = bs.try_into_binary().ok_or(...)?;

// Read
let bytes: &[u8] = bin.as_bytes();
let len: usize = bin.len();
let text: &str = bin.try_str()?;      // UTF-8 validation

// Sub-binary (zero-copy slice)
let sub = bin.sub(0, 5);

// Construct from bytes
let new_bin = Binary::from_bytes(env, b"hello");
```

**BinaryBuilder** — growable buffer for constructing binaries, mirrors `Vec<u8>`:

```rust
// Append-style (unknown size)
let mut builder = BinaryBuilder::new();
builder.extend_from_slice(b"hel");
builder.extend_from_slice(b"lo");
let bin: Binary = builder.finish(env);

// Pre-sized with indexed writes (known size)
let mut builder = BinaryBuilder::with_capacity(5);
builder.resize(5, 0);
let buf: &mut [u8] = builder.as_mut_slice();
buf[0] = b'h';
buf[1] = b'e';
buf[2] = b'l';
buf[3] = b'l';
buf[4] = b'o';
let bin: Binary = builder.finish(env);
```

`BinaryBuilder` allocates via `enif_alloc_binary` and grows via `enif_realloc_binary`. `finish()` shrinks to the written length and transfers ownership to the BEAM. If dropped without calling `finish()`, the allocation is released. Implements `std::io::Write`.

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
let val: Option<TypedTerm> = map.get(atom_key);

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
    // key and value are TypedTerm<'a>
}
```

### Pid

No lifetime — pids are tagged immediates.

```rust
let TypedTerm::Pid(pid) = term else { ... };

// Current process
let self_pid = Pid::self_(env);

// Liveness check
let alive: bool = pid.is_alive(env);

// Registered name lookup
let pid = Pid::whereis(env, name_atom);
```

### Port

```rust
let TypedTerm::Port(port) = term else { ... };

// Registered name lookup
let port = Port::whereis(env, name_atom);

// Send a command
let ok: bool = port.command(env, msg_term);
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

At module scope, list the atoms you need:

```rust
otter::declare_atoms![ok, error, not_found];
```

For atom names that are not valid Rust identifiers, use `ident = "name"` syntax:

```rust
otter::declare_atoms![ok, error, content_type = "content-type"];
```

This generates a hidden `__otter_atoms` module containing one `StaticAtom` per entry and an `init` function.

### Step 2: Initialize

Call `init_atoms!` from your `on_load` callback:

```rust
fn on_load(env: Env, _load_info: Term) -> bool {
    otter::init_atoms!(env);
    true
}

otter::init!("my_module", [my_nif], load = on_load);
```

### Step 3: Use

Retrieve any declared atom by name:

```rust
#[otter::nif]
fn example(_env: Env) -> Atom {
    otter::atom![ok]
}
```

`atom!` returns an `Atom` — it works anywhere an `Atom` is expected.

### What the macros generate

The macros generate code you could write by hand. Nothing is hidden:

```rust
// otter::declare_atoms![ok, error, content_type = "content-type"];
// expands to:
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

// otter::init_atoms!(env);  →  __otter_atoms::init(env);
// otter::atom![ok]          →  __otter_atoms::ok.get()
```

`StaticAtom::get()` is a single `AtomicUsize` load with `Relaxed` ordering. In debug builds, it panics if called before `init`.

### Notes

A few rules follow from what the macros expand to:

- **Initialize in `on_load`.** Call `init_atoms!(env)` before any NIF runs. Forgetting it panics on the first `atom![…]` call (release builds too — the check is unconditional). Calling it more than once is harmless; it re-interns the same names.
- **One `declare_atoms!` per crate, in the module that hosts your `on_load` callback.** The generated `__otter_atoms` submodule has a fixed name, so a second `declare_atoms!` in the same module would collide. The atom system is a deliberately small convenience — declare all your literal atoms in one place; use `ident = "name"` to disambiguate when two atom *strings* would otherwise produce the same identifier.
- **`atom![…]` and `init_atoms!` resolve `__otter_atoms` by ordinary name lookup, so it must be in scope.** In the module that invoked `declare_atoms!`, it's already in scope. To use the atoms from a sibling or descendant module, bring the module in with a `use` statement:

  ```rust
  // lib.rs — declares atoms here, hosts on_load:
  otter::declare_atoms![ok, error, not_found];

  fn on_load(env: Env, _info: Term) -> bool {
      otter::init_atoms!(env);
      true
  }
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
  otter::declare_atoms![ok, content_type = "content-type"];
  let ct = otter::atom![content_type];  // the atom "content-type"
  ```

- **Duplicates.** Two entries with the same identifier are a compile error. Two different identifiers mapped to the same string (`ok` and `okay = "ok"`) are fine — both intern the same BEAM atom and compare equal.
- **Atom name length.** Erlang atoms cap at 255 characters; over-length names fail at `init_atoms!`, not mid-NIF.
- **Thread- and env-safe.** `atom![…]` is safe from any scheduler thread, including dirty NIFs. The returned `Atom` is valid in any environment, including an `OwnedEnv`.

---

## Encoder and Decoder

All otter types implement `Encoder` and `Decoder`. These traits are what the `#[otter::nif]` macro uses for automatic argument decoding and return encoding.

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

`Result<T, E>` implements `Encoder` when both `T` and `E` do: `Ok(v)` encodes `v`, `Err(e)` encodes `e` and raises it via `enif_raise_exception`. This is how `Result`-returning NIFs work — through normal trait dispatch on the return type, not through any macro-level special case. A user type happening to be called `Result` does not inherit this behavior.

**CodecError variants:**

| Variant | Meaning |
|---|---|
| `WrongType` | TypedTerm is not the expected type |
| `IntegerOverflow` | Integer doesn't fit in the target Rust type |
| `InvalidCodepoint` | Integer is not a valid Unicode codepoint |

---

## Error Handling

### Result return type

The idiomatic shape is a `Result<T, E>` return type where both `T: Encoder` and `E: Encoder`. `Ok(val)` encodes and returns; `Err(reason)` encodes the reason and raises it as a class-`error` exception (via `enif_raise_exception`):

```rust
otter::declare_atoms![division_by_zero];

#[otter::nif]
fn divide<'a>(env: Env<'a>, a: Integer<'a>, b: Integer<'a>) -> Result<Integer<'a>, Atom> {
    let bv = i64::try_from(b).unwrap();
    if bv == 0 {
        Err(otter::atom![division_by_zero])
    } else {
        let av = i64::try_from(a).unwrap();
        Ok(Integer::from_i64(env, av / bv))
    }
}
```

This is normal `Encoder` trait dispatch on the return type — `Result<T, E>` has a blanket impl. No macro-level special case; a user type happening to be named `Result` does not inherit this behavior.

### Raising exceptions explicitly

For the cases where a `Result` return type doesn't fit — raising from a helper, or building the error term mid-function — call the raise primitives directly. Both produce a `TypedTerm<'a>` that you return from the NIF:

```rust
// badarg
return env.raise_badarg();

// arbitrary reason — accepts any AsNifTerm, no .encode(env) needed.
// `my_error` is pre-declared via `declare_atoms![my_error]` at module scope.
return env.raise(otter::atom![my_error]);
```

These are the only two exception mechanisms in the NIF C API (`enif_make_badarg` and `enif_raise_exception`). The `Err` branch of a `Result` return goes through `enif_raise_exception` under the hood.

---

## Resources

Resources let you own Rust data from the BEAM side. The BEAM manages the lifetime via reference counting — when no Erlang term references the resource, the destructor runs.

### Defining a resource

```rust
use otter::resource::{Resource, ResourceArc, ResourceTypeHandle};
use std::sync::OnceLock;

struct MyState {
    counter: std::sync::atomic::AtomicU64,
}

static MY_STATE_TYPE: OnceLock<ResourceTypeHandle> = OnceLock::new();

impl Resource for MyState {
    fn resource_type_handle() -> &'static OnceLock<ResourceTypeHandle> {
        &MY_STATE_TYPE
    }
}
```

### Registering

Registration must happen in the load callback:

```rust
fn on_load(env: Env, _load_info: Term) -> bool {
    otter::resource::register_resource_type::<MyState>(env);
    true
}

otter::init!("my_module", [create, increment, read], load = on_load);
```

### Creating and using

```rust
#[otter::nif]
fn create(env: Env) -> ResourceArc<MyState> {
    ResourceArc::from(MyState {
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

### Destructors and monitors

```rust
impl Resource for MyState {
    fn resource_type_handle() -> &'static OnceLock<ResourceTypeHandle> {
        &MY_STATE_TYPE
    }

    fn destructor(self, _env: Env<'_>) {
        // cleanup when reference count hits zero
    }

    fn down<'a>(&'a self, _env: Env<'a>, _pid: Pid, _monitor: Monitor) {
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

## OwnedEnv and Message Passing

`OwnedEnv` lets you build terms and send messages from outside a NIF call — typically from a spawned OS thread.

```rust
use std::thread;

#[otter::nif]
fn start_worker(env: Env) -> Atom {
    let pid = Pid::self_(env);
    thread::spawn(move || {
        let mut owned = OwnedEnv::new();
        let result = do_heavy_work();
        owned.send(&pid, |env| {
            Integer::from_i64(env, result).into()
        });
    });
    otter::atom![ok]  // assuming `ok` is pre-declared
}
```

The closure passed to `send` receives a temporary `Env`. Terms built inside cannot escape — the lifetime is bound to the closure. After `send`, the environment is automatically cleared.

Call `owned.clear()` to reuse the environment for multiple sends without reallocating.

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
use otter::types::{Atom, Binary, BinaryBuilder, Integer, List};

otter::declare_atoms![world, ok];

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
    let mut builder = BinaryBuilder::with_capacity(bytes.len());
    for &b in bytes.iter().rev() {
        builder.push(b);
    }
    builder.finish(env)
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

fn on_load(env: Env, _load_info: Term) -> bool {
    otter::init_atoms!(env);
    true
}

otter::init!("my_nifs", [hello, add, echo, reverse_binary, sum_list], load = on_load);
```
