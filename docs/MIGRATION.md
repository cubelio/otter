# Migrating from Rustler to Otter

A side-by-side guide for converting existing Rustler NIFs to Otter.

---

## NIF Declaration

**Rustler:**
```rust
#[rustler::nif]
fn add(a: i64, b: i64) -> i64 {
    a + b
}

rustler::init!("my_module");
```

**Otter:**
```rust
use otter::env::Env;
use otter::types::Integer;

#[otter::nif]
fn add<'a>(env: Env<'a>, a: Integer<'a>, b: Integer<'a>) -> Integer<'a> {
    let sum = i64::try_from(a).unwrap() + i64::try_from(b).unwrap();
    Integer::from_i64(env, sum)
}

otter::init!("my_module", [add]);
```

Key differences:
- `Env` is required. Rustler's macro detects `Env` and `TypedTerm` by matching the *unqualified identifier string* of the argument type (see `rustler_codegen/src/nif.rs`), so an alias like `use rustler::Env as MyEnv` silently changes the macro's behavior. Otter requires `Env` as the first positional argument and routes all other arguments through `Decoder` — no name-based dispatch.
- Arguments are BEAM types (`Integer`), not Rust primitives. Rustler auto-converts `i64`; otter gives you the BEAM term and you extract when ready.
- NIFs are listed explicitly in `init!`. Rustler collects them via linker magic (`inventory` crate).
- Module name is the bare Erlang module name. Rustler's `init!` accepts both styles (`"Elixir.MyModule"` and `"my_module"`); otter uses bare names.

---

## TypedTerm Handling

**Rustler:**
```rust
// One type: TypedTerm<'a>, opaque wrapper around NIF_TERM
fn example(term: TypedTerm) -> TypedTerm {
    term
}
```

**Otter:**
```rust
// Three resolution levels, each at a different cost:
//   Term  — bare machine word, zero work
//   TypedTerm     — typed enum (one enif_term_type call)
//   data     — extraction methods on concrete types
//
// Every #[otter::nif] takes Env as its first argument. Subsequent
// arguments are decoded through Decoder; both TypedTerm and concrete types
// implement Decoder.

#[otter::nif]
fn example(_env: Env, val: TypedTerm) -> TypedTerm {  // TypedTerm = typed enum
    val
}
```

You choose the resolution level: `TypedTerm` when you need to branch on type, concrete types when you need the data. (`Term` is supported as a return type but not as an argument — argument-side resolution always goes through `Decoder`, which is a no-op for `TypedTerm`.)

---

## Type Conversions

### Rustler auto-converts Rust primitives. Otter uses BEAM types.

| Rustler | Otter | Notes |
|---|---|---|
| `i64`, `i32`, etc. | `Integer<'a>` | `i64::try_from(integer)` to extract |
| `f64` | `Float<'a>` | `f64::from(float)` to extract |
| `String` | `Binary<'a>` | Call `.as_bytes()` or `.try_str()` |
| `&str` | `Binary<'a>` | Same — binaries are the Erlang string type |
| `bool` | `Atom` | `declare_atoms![true_ = "true", false_ = "false"]` + `atom![true_]` / `atom![false_]` (the bare `true` / `false` identifiers are Rust keywords) |
| `Vec<T>` | `List<'a>` | Walk with `iter()`, build with `List::from_terms()` |
| `(A, B)` | `Tuple<'a>` | Access with `.element(i)`, build with `Tuple::from_terms()` |
| `HashMap<K,V>` | `Map<'a>` | Use `.get()`, `.put()`, `.iter()` |
| `rustler::Atom` | `Atom` | `declare_atoms![name]` + `atom![name]` (or `Atom::intern` for runtime strings; see [Atom-table safety](USAGE.md#atom-table-safety)) |
| `rustler::Binary` | `Binary<'a>` | `Binary::from_bytes(env, &[u8])` |
| `rustler::TypedTerm` | `TypedTerm<'a>` | Typed enum, not opaque |
| `rustler::Error` | *(none)* | `Result<T, Raised>`; raise via `env.raise_exception()` / `env.make_badarg()` |
| `rustler::ResourceArc<T>` | `ResourceArc<T>` | Same concept, different registration |

---

## Atoms

**Rustler:**
```rust
mod atoms {
    rustler::atoms! {
        ok,
        error,
        not_found,
    }
}

// Usage
atoms::ok().encode(env)
```

**Otter:**
```rust
// Pre-declare atoms for zero-cost retrieval
otter::declare_atoms![ok, error, not_found];

fn on_load(env: Env, _load_info: Term) -> bool {
    otter::init_atoms!(env);
    true
}

// Usage — single atomic load, no NIF call
otter::atom![ok]
```

`declare_atoms!` pre-declares atoms that are interned once at NIF load time. `atom!` retrieves them with a single atomic load. For atom names that aren't valid Rust identifiers, use `ident = "name"` syntax: `content_type = "content-type"`.

For runtime atom strings (rare — prefer `declare_atoms!` for any compile-time-known name; **never** call `intern` on untrusted input — see [Atom-table safety](USAGE.md#atom-table-safety)):

```rust
Atom::intern(env, "ok").unwrap()

// Look up without creating — None if atom doesn't exist
Atom::try_existing(env, "not_found")

// Extract name
atom.name(env)  // -> String
```

---

## Lists

**Rustler:**
```rust
// Decode into Vec
let items: Vec<i64> = term.decode()?;

// Encode from Vec
let list = vec![1, 2, 3].encode(env);

// Iterator
let iter = term.decode::<ListIterator>()?;
for item in iter {
    let val: i64 = item.decode()?;
}
```

**Otter:**
```rust
use otter::types::List;

// Iterator — yields Term heads, one enif_get_list_cell per step
for head in list.iter() {
    let h: TypedTerm = head.resolve();
    // process h...
}

// Check for improper tail after iteration
let mut iter = list.iter();
while iter.next().is_some() { /* ... */ }
let tail = iter.tail().unwrap(); // [] for proper, other term for improper

// Construct from terms
let list = List::from_terms(env, &[t1, t2, t3]);

// Cons cell
let cell = List::cons(env, head, tail);
```

Lists are cons cells — `iter()` wraps `enif_get_list_cell` and exposes the terminal tail. For low-level decomposition, `node()` gives direct `Nil` / `Cell(Term, Term)` access.

---

## Tuples

**Rustler:**
```rust
let (a, b, c): (i64, String, Atom) = term.decode()?;
let tuple = (1, "hello", atoms::ok()).encode(env);
```

**Otter:**
```rust
let TypedTerm::Tuple(tup) = term else { return env.make_badarg() };
let a = tup.element(0);  // -> TypedTerm
let b = tup.element(1);
let c = tup.element(2);

// `ok` is pre-declared via `declare_atoms![ok]` at module scope.
let tup = Tuple::from_terms(env, [
    Integer::from_i64(env, 1).into(),
    Binary::from_bytes(env, b"hello").into(),
    otter::atom![ok].into(),
]);
```

Elements are `TypedTerm` values. You resolve and decode them yourself. Construction uses `Tuple::from_terms` with any iterable of `impl AsNifTerm<'a>` values — concrete types can be passed directly for homogeneous tuples, or use `.into()` to convert to `TypedTerm` for mixed types.

---

## Maps

**Rustler:**
```rust
let map: HashMap<String, i64> = term.decode()?;
let term = map.encode(env);
```

**Otter:**
```rust
let TypedTerm::Map(map) = term else { return env.make_badarg() };

// Lookup
let val: Option<TypedTerm> = map.get(key_term);

// Insert (returns new map)
let map2 = map.put(key_term, val_term);

// Update existing key
let map3: Option<Map> = map.update(key_term, new_val);

// Iterate
for (k, v) in map.iter() {
    // k, v are TypedTerm<'a>
}

// Construct empty then build up — no .encode(env) needed.
// `key` is pre-declared via `declare_atoms![key]` at module scope.
let mut m = Map::new(env);
m = m.put(
    otter::atom![key],
    Integer::from_i64(env, 42),
);
```

---

## Error Handling

**Rustler:**
```rust
use rustler::Error;

#[rustler::nif]
fn divide(a: i64, b: i64) -> Result<i64, Error> {
    if b == 0 {
        Err(Error::TypedTerm(Box::new("division_by_zero")))  // returns {error, reason}?
        // or
        Err(Error::RaiseAtom("badarith"))               // raises exception?
        // or
        Err(Error::Atom("error"))                       // returns atom?
    } else {
        Ok(a / b)
    }
}
```

Rustler's `Error` enum has multiple variants that do different things — some return values, some raise exceptions. The semantics aren't obvious from the code.

**Otter:**
```rust
// `badarith` is pre-declared via `declare_atoms![badarith]` at module scope.
#[otter::nif]
fn divide<'a>(env: Env<'a>, a: Integer<'a>, b: Integer<'a>) -> Result<Integer<'a>, Atom> {
    let bv = i64::try_from(b).unwrap();
    if bv == 0 {
        Err(otter::atom![badarith])  // raises exception
    } else {
        let av = i64::try_from(a).unwrap();
        Ok(Integer::from_i64(env, av / bv))
    }
}
```

A NIF returns `Result<T, Raised>`. `Ok` returns normally; `Err(Raised)` carries an already-pending exception straight out — it is never re-raised, so there is no double-raise. Produce the `Raised` and propagate it:
```rust
return env.make_badarg();           // enif_make_badarg
return env.raise_exception(reason); // enif_raise_exception — any AsNifTerm<'a>
```

These are the only two exception mechanisms in the NIF C API. Otter exposes exactly those, both generic over the success type so they fit `return`, `let`-`else`, and `.or_else` positions.

---

## Resources

**Rustler:**
```rust
pub struct MyResource { /* ... */ }

#[rustler::resource_impl]
impl rustler::Resource for MyResource {}

// Registration is automatic — `#[rustler::resource_impl]` submits the
// registration to `inventory`, and `rustler::init!` emits a load
// callback that calls `ResourceRegistration::register_all_collected`.
rustler::init!("my_module");
```

**Otter:**
```rust
use otter::resource::{Resource, ResourceArc, ResourceTypeHandle};
use std::sync::OnceLock;

struct MyResource { /* ... */ }

static MY_RESOURCE_TYPE: OnceLock<ResourceTypeHandle> = OnceLock::new();

impl Resource for MyResource {
    fn resource_type_handle() -> &'static OnceLock<ResourceTypeHandle> {
        &MY_RESOURCE_TYPE
    }
}

fn on_load(env: Env, _load_info: Term) -> bool {
    otter::resource::register_resource_type::<MyResource>(env);
    true
}

otter::init!("my_module", [create, use_it], load = on_load);
```

The `OnceLock<ResourceTypeHandle>` static is the **type registration** — one per type, not per instance. Every `ResourceArc::from(MyResource { ... })` allocates a new instance on the BEAM heap with its own refcount.

Creating and receiving resources:
```rust
// Create — returns opaque reference to Erlang
#[otter::nif]
fn create(env: Env) -> ResourceArc<MyResource> {
    ResourceArc::from(MyResource { /* ... */ })
}

// Receive — Decoder extracts ResourceArc from reference term
#[otter::nif]
fn use_it(_env: Env, res: ResourceArc<MyResource>) -> Atom {
    // Deref gives &MyResource
    res.do_something();
    // ...
}
```

Destructors and monitors:
```rust
impl Resource for MyResource {
    fn resource_type_handle() -> &'static OnceLock<ResourceTypeHandle> {
        &MY_RESOURCE_TYPE
    }

    fn destructor(self, _env: Env<'_>) {
        // all references gone — clean up
    }

    fn down<'a>(&'a self, _env: Env<'a>, _pid: Pid, _monitor: Monitor) {
        // monitored process exited
    }
}
```

---

## OwnedEnv / Message Passing

**Rustler:**
```rust
use rustler::{OwnedEnv, Encoder};

let pid = env.pid();
std::thread::spawn(move || {
    let mut msg_env = OwnedEnv::new();
    msg_env.send_and_clear(&pid, |env| {
        (atoms::result(), 42).encode(env)
    });
});
```

**Otter:**
```rust
use otter::env::OwnedEnv;

let pid = Pid::self_(env);
std::thread::spawn(move || {
    let mut owned = OwnedEnv::new();
    // `result` is pre-declared via `declare_atoms![result]` at module scope.
    owned.send(&pid, |env| {
        Tuple::from_terms(env, [
            otter::atom![result].into(),
            Integer::from_i64(env, 42).into(),
        ]).into()
    });
});
```

Same pattern. The closure gets a temporary `Env`; terms built inside cannot escape. Environment is cleared after send. Call `owned.clear()` to reuse for multiple sends.

---

## Dirty Scheduling

**Rustler:**
```rust
#[rustler::nif(schedule = "DirtyCpu")]
fn heavy(a: i64) -> i64 { /* ... */ }
```

**Otter:**
```rust
#[otter::nif(schedule = "DirtyCpu")]
fn heavy<'a>(env: Env<'a>, a: Integer<'a>) -> Integer<'a> { /* ... */ }
```

Same attribute, same values (`"DirtyCpu"`, `"DirtyIo"`).

---

## Build System

**Rustler (Erlang):** rustler ships no build integration for rebar3, so most Erlang users hand-roll a `pre_hooks` shell invocation or a Makefile that drives `cargo build` and copies the artifact into `priv/`. A minimal pre-hook approach:

```erlang
%% rebar.config
{pre_hooks, [
    {compile, "cargo build --release --manifest-path native/my_nifs/Cargo.toml"},
    {compile, "mkdir -p priv && cp native/my_nifs/target/release/libmy_nifs.so priv/"}
]}.
```

```toml
# native/my_nifs/Cargo.toml
[lib]
crate-type = ["cdylib"]

[dependencies]
rustler = "0.37"
```

**Otter (rebar3):**
```erlang
%% rebar.config
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

```toml
# native/my_nifs/Cargo.toml
[lib]
crate-type = ["cdylib"]

[dependencies]
otter = { git = "https://github.com/cubelio/otter.git" }
```

---

## What Otter Does Not Have

| Rustler feature | Why otter excludes it |
|---|---|
| `NifStruct` | Elixir structs (`__struct__` key) — no Erlang equivalent |
| `NifException` | Elixir exceptions — no Erlang equivalent |
| `NifUntaggedEnum` | Try-each dispatch — belongs in user code |
| Serde integration | Erlang terms don't map to serde's data model |
| `atoms!` macro | `declare_atoms!` + `atom!` — pre-declared atoms with zero-cost retrieval |
| `ListIterator` | Lists are cons cells, not iterators |
| Automatic NIF registration | Explicit `init!` — visible, auditable |
| `Error` enum | `Result<T, Raised>` + `env.raise_exception()` / `env.make_badarg()` — the actual NIF API |
| Rust primitive args (`i64`, `String`) | BEAM types — you decide when to extract |

---

## Migration Checklist

1. Replace `rustler::init!` with `otter::init!("module_name", [nif1, nif2, ...])` — list all NIFs explicitly
2. Add `env: Env` as first argument to every NIF that needs it
3. Replace Rust primitive arguments with BEAM types (`i64` -> `Integer`, `String` -> `Binary`, etc.)
4. Add explicit lifetime `<'a>` when multiple arguments carry lifetimes
5. Replace `rustler::Error` returns with `Result<T, Raised>`; raise via `env.raise_exception(reason)` / `env.make_badarg()`
6. Replace `atoms! {}` blocks with `declare_atoms!` / `init_atoms!` / `atom!`. Reserve `Atom::intern(env, "name")` for runtime strings — and never call it on untrusted input ([Atom-table safety](USAGE.md#atom-table-safety))
7. Replace `Vec<T>` list handling with `list.iter()` iterator
8. Replace `resource!` macro with `OnceLock<ResourceTypeHandle>` + `Resource` trait impl + `register_resource_type` in load callback
9. Replace `OwnedEnv::send_and_clear` with `OwnedEnv::send`
10. Update `Cargo.toml`: replace `rustler` dependency with `otter`
11. Update build config: replace Mix/rustler config with `rebar.config` + `rebar3_otter`
