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

rustler::init!("Elixir.MyModule");
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
- `Env` is explicit. Rustler hides it; otter passes it as the first argument.
- Arguments are BEAM types (`Integer`), not Rust primitives. Rustler auto-converts `i64`; otter gives you the BEAM term and you extract when ready.
- NIFs are listed explicitly in `init!`. Rustler collects them via linker magic (`inventory` crate).
- Module name is the Erlang module name, not an Elixir module name.

---

## Term Handling

**Rustler:**
```rust
// One type: Term<'a>, opaque wrapper around NIF_TERM
fn example(term: Term) -> Term {
    term
}
```

**Otter:**
```rust
// Three resolution levels, each at a different cost:
//   RawTerm  — bare machine word, zero work
//   Term     — typed enum (one enif_term_type call)
//   data     — extraction methods on concrete types
//
// Every #[otter::nif] takes Env as its first argument. Subsequent
// arguments are decoded through Decoder; both Term and concrete types
// implement Decoder.

#[otter::nif]
fn example(_env: Env, val: Term) -> Term {  // Term = typed enum
    val
}
```

You choose the resolution level: `Term` when you need to branch on type, concrete types when you need the data. (`RawTerm` is supported as a return type but not as an argument — argument-side resolution always goes through `Decoder`, which is a no-op for `Term`.)

---

## Type Conversions

### Rustler auto-converts Rust primitives. Otter uses BEAM types.

| Rustler | Otter | Notes |
|---|---|---|
| `i64`, `i32`, etc. | `Integer<'a>` | `i64::try_from(integer)` to extract |
| `f64` | `Float<'a>` | `f64::from(float)` to extract |
| `String` | `Binary<'a>` | Call `.as_bytes()` or `.try_str()` |
| `&str` | `Binary<'a>` | Same — binaries are the Erlang string type |
| `bool` | `Atom` | Construct `Atom::new(env, "true")` |
| `Vec<T>` | `List<'a>` | Walk with `iter()`, build with `List::from_terms()` |
| `(A, B)` | `Tuple<'a>` | Access with `.element(i)`, build with `Tuple::from_terms()` |
| `HashMap<K,V>` | `Map<'a>` | Use `.get()`, `.put()`, `.iter()` |
| `rustler::Atom` | `Atom` | `Atom::new(env, "name")` |
| `rustler::Binary` | `Binary<'a>` | `Binary::from_bytes(env, &[u8])` |
| `rustler::Term` | `Term<'a>` | Typed enum, not opaque |
| `rustler::Error` | *(none)* | Use `env.raise()` or `env.raise_badarg()` directly |
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

For one-off atoms, you can also create them directly:

```rust
Atom::new(env, "ok").unwrap()

// Look up without creating
Atom::try_existing(env, "not_found")  // None if atom doesn't exist

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

// Iterator — yields RawTerm heads, one enif_get_list_cell per step
for head in list.iter() {
    let h: Term = head.resolve();
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

Lists are cons cells — `iter()` wraps `enif_get_list_cell` and exposes the terminal tail. For low-level decomposition, `node()` gives direct `Nil` / `Cell(RawTerm, RawTerm)` access.

---

## Tuples

**Rustler:**
```rust
let (a, b, c): (i64, String, Atom) = term.decode()?;
let tuple = (1, "hello", atoms::ok()).encode(env);
```

**Otter:**
```rust
let Term::Tuple(tup) = term else { return env.raise_badarg() };
let a = tup.element(0);  // -> Term
let b = tup.element(1);
let c = tup.element(2);

let tup = Tuple::from_terms(env, [
    Integer::from_i64(env, 1).into(),
    Binary::from_bytes(env, b"hello").into(),
    Atom::new(env, "ok").unwrap().into(),
]);
```

Elements are `Term` values. You resolve and decode them yourself. Construction uses `Tuple::from_terms` with any iterable of `impl TermIn` values — concrete types can be passed directly for homogeneous tuples, or use `.into()` to convert to `Term` for mixed types.

---

## Maps

**Rustler:**
```rust
let map: HashMap<String, i64> = term.decode()?;
let term = map.encode(env);
```

**Otter:**
```rust
let Term::Map(map) = term else { return env.raise_badarg() };

// Lookup
let val: Option<Term> = map.get(key_term);

// Insert (returns new map)
let map2 = map.put(key_term, val_term);

// Update existing key
let map3: Option<Map> = map.update(key_term, new_val);

// Iterate
for (k, v) in map.iter() {
    // k, v are Term<'a>
}

// Construct empty then build up — no .encode(env) needed
let mut m = Map::new(env);
m = m.put(
    Atom::new(env, "key").unwrap(),
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
        Err(Error::Term(Box::new("division_by_zero")))  // returns {error, reason}?
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
#[otter::nif]
fn divide<'a>(env: Env<'a>, a: Integer<'a>, b: Integer<'a>) -> Result<Integer<'a>, Atom> {
    let bv = i64::try_from(b).unwrap();
    if bv == 0 {
        Err(Atom::new(env, "badarith").unwrap())  // raises exception
    } else {
        let av = i64::try_from(a).unwrap();
        Ok(Integer::from_i64(env, av / bv))
    }
}
```

`Result<T, E>` where both `T: Encoder` and `E: Encoder`. `Ok` returns normally, `Err` always raises. One behavior, no ambiguity.

For direct control:
```rust
env.raise_badarg()              // enif_make_badarg
env.raise(reason)               // enif_raise_exception — accepts impl TermIn
```

These are the only two exception mechanisms in the NIF C API. Otter exposes exactly those.

---

## Resources

**Rustler:**
```rust
pub struct MyResource { /* ... */ }

#[rustler::resource_impl]
impl rustler::Resource for MyResource {}

// In init
fn load(env: Env, _info: Term) -> bool {
    rustler::resource!(MyResource, env);
    true
}

rustler::init!("Elixir.MyMod", load = load);
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
    otter::resource::register_resource_type::<MyResource>(env, "my_resource");
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
    owned.send(&pid, |env| {
        Tuple::from_terms(env, [
            Atom::new(env, "result").unwrap().into(),
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

**Rustler (Mix):**
```elixir
# mix.exs
defp deps do
  [{:rustler, "~> 0.30"}]
end
```

**Otter (rebar3):**
```erlang
%% rebar.config
{plugins, [
    {rebar3_otter, {git, "https://github.com/cubelio/otter.git", {branch, "master"}}}
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
| `Error` enum | `env.raise()` and `env.raise_badarg()` — the actual NIF API |
| Rust primitive args (`i64`, `String`) | BEAM types — you decide when to extract |

---

## Migration Checklist

1. Replace `rustler::init!` with `otter::init!("module_name", [nif1, nif2, ...])` — list all NIFs explicitly
2. Add `env: Env` as first argument to every NIF that needs it
3. Replace Rust primitive arguments with BEAM types (`i64` -> `Integer`, `String` -> `Binary`, etc.)
4. Add explicit lifetime `<'a>` when multiple arguments carry lifetimes
5. Replace `rustler::Error` returns with `Result<T, E>` where both implement `Encoder`, or use `env.raise()` / `env.raise_badarg()` directly
6. Replace `atoms! {}` blocks with `declare_atoms!` / `init_atoms!` / `atom!`, or `Atom::new(env, "name")` for one-off atoms
7. Replace `Vec<T>` list handling with `list.iter()` iterator
8. Replace `resource!` macro with `OnceLock<ResourceTypeHandle>` + `Resource` trait impl + `register_resource_type` in load callback
9. Replace `OwnedEnv::send_and_clear` with `OwnedEnv::send`
10. Update `Cargo.toml`: replace `rustler` dependency with `otter`
11. Update build config: replace Mix/rustler config with `rebar.config` + `rebar3_otter`
