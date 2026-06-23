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
use otter::types::CallEnv;

#[otter::nif]
fn add(_env: CallEnv, a: i64, b: i64) -> i64 {
    a + b
}

otter::init!("my_module", [add]);
```

Key differences:
- The call env is required as the first argument, typed `CallEnv<'a>`. Rustler's macro detects `Env` and `TypedTerm` by matching the *unqualified identifier string* of the argument type (see `rustler_codegen/src/nif.rs`), so an alias like `use rustler::Env as MyEnv` silently changes the macro's behavior. Otter passes the first positional argument straight through and routes all other arguments through `Decoder` — no name-based dispatch.
- The body ports almost unchanged: otter decodes native Rust types (`i64`, `String`, `Vec<T>`, …) through the same `Decoder` the macro uses, so rustler's primitive arguments carry straight over — only the call env is added. When you want lazy, zero-copy access instead, take a BEAM term type (`Integer<'a>`) and extract when ready — see [Type Conversions](#type-conversions).
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
//   AnyTerm<'a>   — bare machine word, zero work
//   TypedTerm<'a> — typed enum (one enif_term_type call)
//   data          — extraction methods on concrete types (each takes env)
//
// Every #[otter::nif] takes the call env as its first argument. Subsequent
// arguments are decoded through Decoder; both AnyTerm and concrete types
// implement Decoder.

#[otter::nif]
fn example<'a>(_env: CallEnv<'a>, val: TypedTerm<'a>) -> TypedTerm<'a> {  // typed enum
    val
}
```

You choose the resolution level: `AnyTerm` for the raw word, `TypedTerm` when you need to branch on type, concrete types when you need the data. `AnyTerm` works as both an argument and a return type — its `Decoder` impl is the identity.

---

## Type Conversions

### Otter offers both BEAM term types and native-type codecs.

Otter's term types give you lazy, zero-copy access (extract when ready); the native codecs (since the codec suite landed) let many rustler primitive signatures port **unchanged** — pick per argument.

| Rustler | Otter (term type) | Otter (native codec) | Notes |
|---|---|---|---|
| `i64`, `i32`, … | `Integer<'a>` | `i64`, `u8`, `usize`, … | `integer.to_i64(env)` to extract from the term type; native ints en/decode directly |
| `f64` | `Float<'a>` | `f64`, `f32` | `float.to_f64(env)`; non-finite encode → `badret` |
| `String` / `&str` | `Binary<'a>` | `String` | term: `bin.as_bytes(env)` / `bin.try_str(env)`; native `String` decodes binary *or* charlist, encodes binary |
| `bool` | `Atom` | `bool` | native `bool` codec, or `atoms = [true_ = "true", false_ = "false"]` + `atom![true_]` (the bare keywords aren't identifiers) |
| `Vec<T>` | `List<'a>` | `Vec<T>` | term: `list.iter(env)` / `List::from_terms`; native `Vec<T>` ↔ Erlang list |
| `(A, B)` | `Tuple<'a>` | `(A, B)` (arity 1–12) | term: `tup.with_elements(env)`; native tuple codec |
| `HashMap<K,V>` | `Map<'a>` | `HashMap<K,V>` | term: `.get(env, k)` / `.put(env, k, v)` / `.iter(env)`; native map codec |
| *(bignum)* | `Integer<'a>` | `BigInt` (`bigint` feature) | `otter::types::BigInt`, ETF-based for the >64-bit cases |
| `rustler::Atom` | `Atom` | — | `atoms = [name]` + `atom![name]` (or `Atom::intern` for runtime strings; see [Atom-table safety](USAGE.md#atom-table-safety)) |
| `rustler::Binary` | `Binary<'a>` | `&[u8]`→ via `Vec<u8>` list / `String` | `Binary::from_bytes(env, &[u8])` |
| `rustler::TypedTerm` | `TypedTerm<'a>` | — | Typed enum, not opaque |
| `rustler::Error` | `Raised<'a>` | — | `Result<T, Raised<'a>>`; raise via `env.raise()` / `env.badarg()` |
| `rustler::ResourceArc<T>` | `ResourceArc<T>` | — | Same concept, different registration |

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
// Pre-declare atoms for zero-cost retrieval — in the init! call
otter::init!("my_module", [my_nif], atoms = [ok, error, not_found]);

// Usage — single atomic load, no NIF call
otter::atom![ok]
```

The `atoms = [...]` list pre-declares atoms that are interned once at NIF load time (and re-interned automatically on hot upgrade). `atom!` retrieves them with a single atomic load. For atom names that aren't valid Rust identifiers, use `ident = "name"` syntax: `content_type = "content-type"`.

For runtime atom strings (rare — prefer the `atoms = [...]` list for any compile-time-known name; **never** call `intern` on untrusted input — see [Atom-table safety](USAGE.md#atom-table-safety)):

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

// Iterator — yields AnyTerm heads, one enif_get_list_cell per step
for head in list.iter(env) {
    let h: Option<TypedTerm> = head.resolve(env);
    // process h...
}

// Check for improper tail after iteration
let mut iter = list.iter(env);
while iter.next().is_some() { /* ... */ }
let tail = iter.tail().unwrap(); // [] for proper, other term for improper

// Construct from any iterable of terms
let list = List::from_terms(env, [t1, t2, t3]);

// Cons cell
let cell = List::cons(env, head, tail);
```

Lists are cons cells — `iter(env)` wraps `enif_get_list_cell` and exposes the terminal tail. For low-level decomposition, `node(env)` gives direct `Nil` / `Cell(AnyTerm, AnyTerm)` access.

---

## Tuples

**Rustler:**
```rust
let (a, b, c): (i64, String, Atom) = term.decode()?;
let tuple = (1, "hello", atoms::ok()).encode(env);
```

**Otter:**
```rust
let TypedTerm::Tuple(tup) = term else { return env.badarg() };
let view = tup.with_elements(env);   // the single enif_get_tuple
let a = view[0];  // -> AnyTerm; resolve with a.resolve(env)
let b = view[1];
let c = view[2];

// `ok` is declared in init!'s `atoms = [...]` list.
let tup = Tuple::from_terms(env, [
    Integer::from_i64(env, 1).into(),
    Binary::from_bytes(env, b"hello").into(),
    otter::atom![ok].into(),
]);
```

Reading elements is an explicit `with_elements(env)` step yielding a `TupleView` you index/iterate; each element is an `AnyTerm` you resolve and decode yourself. Construction uses `Tuple::from_terms` with any iterable of `impl Term<'a>` values — concrete types can be passed directly for homogeneous tuples, or use `.into()` to convert to `TypedTerm` for mixed types.

---

## Maps

**Rustler:**
```rust
let map: HashMap<String, i64> = term.decode()?;
let term = map.encode(env);
```

**Otter:**
```rust
let TypedTerm::Map(map) = term else { return env.badarg() };

// Lookup
let val: Option<AnyTerm> = map.get(env, key_term);

// Insert (returns new map)
let map2 = map.put(env, key_term, val_term);

// Update existing key
let map3: Option<Map> = map.update(env, key_term, new_val);

// Iterate
for (k, v) in map.iter(env) {
    // k, v are AnyTerm<'a>
}

// Construct empty then build up — no .encode(env) needed.
// `key` is declared in init!'s `atoms = [...]` list.
let mut m = Map::new(env);
m = m.put(
    env,
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
// `badarith` is declared in init!'s `atoms = [...]` list.
#[otter::nif]
fn divide<'a>(env: CallEnv<'a>, a: i64, b: i64) -> Result<i64, Raised<'a>> {
    if b == 0 {
        env.raise(otter::atom![badarith])   // raises exception
    } else {
        Ok(a / b)
    }
}
```

A NIF returns `Result<T, Raised<'a>>`. `Ok` returns normally; `Err(Raised)` carries an already-pending exception straight out — it is never re-raised, so there is no double-raise. Produce the `Raised` and propagate it (the only `Encoder` for a `Result` is `Result<T, Raised>` — there is no `Err(atom) → {error, term}` shape; use a value if you want to *return* one):
```rust
return env.badarg();            // enif_make_badarg
return env.raise(reason);       // enif_raise_exception — any impl Term<'a>
```
The encode side mirrors this: a failed `Encoder` (e.g. a non-finite float return) raises `badret`, symmetric to `badarg`.

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
use otter::resource::{Resource, ResourceArc};

struct MyResource { /* ... */ }

impl Resource for MyResource {}

otter::init!("my_module", [create, use_it],
    resources = [MyResource]);
```

Listing a type in `resources = [...]` **is** the registration — one per type, not per instance — and otter registers it in the generated load and upgrade callbacks. (A version tag opts into cross-build hot-upgrade takeover: `resources = [MyResource: "v1"]`.) Every `make_resource(env, MyResource { ... })` then allocates a new instance on the BEAM heap with its own refcount.

Creating and receiving resources:
```rust
// Create — returns opaque reference to Erlang (free function)
#[otter::nif]
fn create(env: CallEnv) -> ResourceArc<MyResource> {
    otter::resource::make_resource(env, MyResource { /* ... */ })
}

// Receive — Decoder extracts ResourceArc from reference term
#[otter::nif]
fn use_it(_env: CallEnv, res: ResourceArc<MyResource>) -> Atom {
    // Deref gives &MyResource
    res.do_something();
    // ...
}
```

Callbacks (run with a `CallbackEnv`):
```rust
impl Resource for MyResource {
    fn destructor(self, _env: CallbackEnv<'_>) {
        // all references gone — clean up
    }

    fn down<'a>(&'a self, _env: CallbackEnv<'a>, _pid: LocalPid, _monitor: Monitor) {
        // monitored process exited (always a local process)
    }

    fn stop(&self, _env: CallbackEnv<'_>, _event: otter::select::Event, _is_direct_call: bool) {
        // the BEAM stopped monitoring a selected event on this resource
    }
}
```

---

## Message Passing

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
use otter::types::{LocalPid, OwnedEnvArena};

let pid = LocalPid::self_(env);
std::thread::spawn(move || {
    let mut arena = OwnedEnvArena::new();
    // `result` is declared in init!'s `atoms = [...]` list.
    let msg = arena.run(|oenv| oenv.export(Tuple::from_terms(oenv, [
        otter::atom![result].into(),
        Integer::from_i64(oenv, 42).into(),
    ])));
    otter::types::send_move(&pid, &mut arena, msg);
});
```

Otter mirrors rustler's reusable `OwnedEnv` with `OwnedEnvArena`: build terms inside `arena.run(|oenv| …)` (the branded `oenv` keeps them from escaping), `export` one to a portable `OwnedEnvTerm`, then `send_move(&pid, &mut arena, oterm)` — which steals the arena heap into the message. `clear` resets the arena for reuse. Where rustler uses an `Arc`/`Weak` token to guard a stale `SavedTerm`, otter uses a process-global generation stamp on the `OwnedEnvTerm`.

Sends are four free verbs in `otter::types` — a 2×2 of copy vs. move × caller-attributed (`_from`, in-NIF) vs. not (plain, off-thread). The plain `send_move`/`send_copy` send with a NULL caller; `send_copy_from(env, &pid, msg)` (copy a live term) and `send_move_from(env, &pid, &mut arena, oterm)` (steal an arena heap) take the calling env and attribute the message to the calling process.

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
fn heavy<'a>(env: CallEnv<'a>, a: Integer<'a>) -> Integer<'a> { /* ... */ }
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
    #{name => "my_nifs", path => "native/my_nifs"}
]}.
```

```toml
# native/my_nifs/Cargo.toml
[lib]
crate-type = ["cdylib"]

[dependencies]
otter-nif = "0.2"
```

---

## What Otter Does Not Have

| Rustler feature | Why otter excludes it |
|---|---|
| `NifStruct` | Elixir structs (`__struct__` key) — no Erlang equivalent |
| `NifException` | Elixir exceptions — no Erlang equivalent |
| `NifUntaggedEnum` | Try-each dispatch — belongs in user code |
| Serde integration | Erlang terms don't map to serde's data model |
| `atoms!` macro | `atoms = [...]` in `init!` + `atom!` — pre-declared atoms with zero-cost retrieval |
| `ListIterator` | Lists are cons cells, not iterators |
| Automatic NIF registration | Explicit `init!` — visible, auditable |
| `Error` enum | `Result<T, Raised>` + `env.raise()` / `env.badarg()` — the actual NIF API |

Note: otter **does** accept native Rust args (`i64`, `String`, `Vec<T>`, `bool`, tuples, `HashMap`) via `Decoder`/`Encoder` — the difference from rustler is that they're opt-in per signature, and the term types stay available for when you want lazy / zero-copy access.

---

## Migration Checklist

1. Replace `rustler::init!` with `otter::init!("module_name", [nif1, nif2, ...])` — list all NIFs explicitly
2. Add `env: CallEnv` as the first argument to every NIF that needs it
3. Choose per argument: a BEAM term type (`Integer`, `Binary`, …) for lazy/zero-copy access, or a native type (`i64`, `String`, `Vec<T>`, …) for a direct codec — many rustler signatures port unchanged
4. Add explicit lifetime `<'a>` when multiple arguments carry lifetimes
5. Replace `rustler::Error` returns with `Result<T, Raised<'a>>`; raise via `env.raise(reason)` / `env.badarg()`
6. Replace `atoms! {}` blocks with `init!`'s `atoms = [...]` list + `atom!`. Reserve `Atom::intern(env, "name")` (now `-> Result<_, AtomError>`) for runtime strings — and never call it on untrusted input ([Atom-table safety](USAGE.md#atom-table-safety))
7. Replace `Vec<T>` list handling with `list.iter(env)`, or just take a native `Vec<T>` argument
8. Replace `resource!` macro with a `Resource` trait impl + listing the type in `init!`'s `resources = [...]`; switch construction to the free fn `otter::resource::make_resource(env, val)`
9. Replace `OwnedEnv::send_and_clear` (and `OwnedEnv::run`/`SavedTerm`) with `OwnedEnvArena` + `otter::types::send_move(&pid, &mut arena, oterm)` off-thread, or `send_copy_from(env, &pid, msg)` / `send_move_from(env, ...)` in a NIF
10. Update `Cargo.toml`: replace `rustler` dependency with `otter`
11. Update build config: replace Mix/rustler config with `rebar.config` + `rebar3_otter`
