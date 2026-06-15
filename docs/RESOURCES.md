# Resources: Rust Heap Data in Erlang

A resource lets you allocate a data structure on the Rust heap and hand an opaque reference to Erlang. Erlang cannot inspect or modify it — it just holds it, passes it around, and sends it to other processes. When no Erlang term references it anymore, the BEAM tells Rust to drop it.

This is how you bridge long-lived Rust state (a connection pool, a compiled regex, a hash map, a file handle) into the BEAM world.

---

## What happens at each step

### 1. Define the Rust struct

```rust
use std::collections::HashMap;
use std::sync::Mutex;

struct MyMap {
    data: Mutex<HashMap<Vec<u8>, Vec<u8>>>,
}
```

This is a plain Rust struct. It lives on the Rust heap and follows normal Rust rules. The `Mutex` is required because the BEAM may call NIFs on this resource from multiple scheduler threads concurrently — `ResourceArc` only gives `&T`, never `&mut T`.

### 2. Register it as a resource type

Before the BEAM can manage instances of your struct, you register the type. You list it in `init!`, and otter registers it in the generated load (and upgrade) callbacks.

```rust
use otter::resource::{Resource, ResourceArc};

impl Resource for MyMap {}

otter::init!("my_module", [new, put, get],
    resources = [MyMap]);
```

`Resource` requires `Send + Sync + 'static`. This is enforced at compile time — the compiler will reject a struct that isn't safe to share across threads. The trait has no required methods; `destructor`, `down`, and `stop` are optional.

The registered type pointer lives in a per-instance registry inside otter-owned `priv_data`, keyed by the type's `TypeId`. Each module instance carries its own registry (sound across a hot upgrade — no shared static), and `ResourceArc` looks the pointer up via `env → enif_priv_data → registry`. This is why creation takes an env (step 3).

The BEAM-side resource type identifier is derived from `std::any::type_name::<T>()` — the fully-qualified Rust type path (e.g. `"my_crate::MyMap"`) — plus a per-build ABI suffix. The type path guarantees uniqueness within the NIF library (BEAM's resource type table is per-library and rustc's `type_name` for distinct types produces distinct strings); the ABI suffix keeps a different build from taking the type over on upgrade. To opt a type into cross-build takeover under a stable name, tag it: `resources = [MyMap: "v1"]`. For dynamic registration outside the list, call `otter::resource::register::<T>(env, ResourceFlags::CREATE)` (or `register_tagged`) inside `load`/`upgrade`.

### 3. Create an instance

```rust
#[otter::nif]
fn new(env: Env) -> ResourceArc<MyMap> {
    env.make_resource(MyMap {
        data: Mutex::new(HashMap::new()),
    })
}
```

What happens here:

1. `env.make_resource(val)` looks `MyMap` up in the registry and calls `enif_alloc_resource` — the BEAM allocates a block of memory and sets its reference count to 1.
2. Rust writes `val` into that block via `ptr::write`.
3. The NIF returns a `ResourceArc`, which the `#[otter::nif]` macro encodes by calling `enif_make_resource` — this creates an Erlang term (an opaque reference) that holds a second reference to the same block.
4. The `ResourceArc` is then dropped at the end of the NIF call, decrementing the count back to 1. Now only the Erlang term keeps the allocation alive.

On the Erlang side, you receive an opaque reference:

```erlang
1> M = my_module:new().
#Ref<0.1234.5.6>
```

This reference is the BEAM's handle to your Rust struct. You cannot inspect it from Erlang. You can only pass it back to NIFs that know how to decode it.

### 4. Use the instance from other NIFs

```rust
// `ok` and `error` are pre-declared via `declare_atoms![ok, error]` at module scope.
#[otter::nif]
fn put<'a>(_env: Env<'a>, key: Binary<'a>, val: Binary<'a>, map: ResourceArc<MyMap>) -> Atom {
    map.data.lock().unwrap().insert(
        key.as_bytes().to_vec(),
        val.as_bytes().to_vec(),
    );
    otter::atom![ok]
}

#[otter::nif]
fn get<'a>(env: Env<'a>, key: Binary<'a>, map: ResourceArc<MyMap>) -> TypedTerm<'a> {
    match map.data.lock().unwrap().get(key.as_bytes()) {
        Some(val) => {
            let ok: TypedTerm = otter::atom![ok].into();
            let bin: TypedTerm = Binary::from_bytes(env, val).into();
            TypedTerm::Tuple(Tuple::from_terms(env, [ok, bin]))
        }
        None => TypedTerm::Atom(otter::atom![error]),
    }
}
```

When Erlang calls `my_module:put(<<"key">>, <<"val">>, M)`, the generated wrapper:

1. Sees that the third argument type is `ResourceArc<MyMap>`
2. Calls `enif_get_resource` to extract the raw pointer from the opaque reference term
3. Calls `enif_keep_resource` to increment the reference count (the `ResourceArc` now holds a reference for the duration of the NIF call)
4. Hands you a `ResourceArc<MyMap>` that `Deref`s to `&MyMap`
5. When the NIF returns, the `ResourceArc` is dropped, decrementing the count

The data never moves. Every NIF call gets a pointer to the same Rust heap allocation.

### 5. Destruction

When the last Erlang reference to the resource is garbage collected, the BEAM calls your destructor:

```rust
impl Resource for MyMap {
    fn destructor(self, _env: Env<'_>) {
        // self is moved here — Rust drops it when this function returns
    }
}
```

The destructor callback is always registered at the C level — it calls `ptr::read` to move the value out and drop it. If you override `destructor`, your code runs before the drop. If you don't, the default no-op runs and the value drops normally. Either way, Rust `Drop` semantics are preserved.

---

## Thread safety

The BEAM runs NIFs on scheduler threads. Multiple NIFs can execute concurrently on different schedulers, and they may all hold references to the same resource. This is why `Resource` requires `Sync`.

`ResourceArc<T>` gives you `&T` — a shared reference. You cannot get `&mut T`. For mutation, use interior mutability:

| Pattern | When to use |
|---|---|
| `RwLock<T>` | Default choice — concurrent reads, exclusive writes |
| `Mutex<T>` | Every access mutates and the critical section is trivially short |
| `AtomicU64` etc. | Single scalar values |

`RwLock` is the right default for NIF resources. Most access patterns are read-heavy, and `RwLock` lets multiple scheduler threads read concurrently. `Mutex` serializes all access — use it only when writes dominate and the lock is held briefly enough that contention doesn't matter.

Pick the narrowest lock scope either way. A NIF that holds a lock across a long computation blocks all other NIFs trying to access the same resource.

---

## Lifetime model

Resources live outside the `Env<'a>` lifetime system. A resource outlives any single NIF call — that's the point. The `ResourceArc<T>` does not carry a lifetime parameter.

This means you cannot store `TypedTerm<'a>` or `Binary<'a>` inside a resource — those are borrowed from the NIF call's environment and become invalid when the NIF returns. To store term data in a resource, copy it into an owned Rust type first (e.g. `Vec<u8>`, `String`, `i64`).

**Across a hot code upgrade**, that owned payload is *not* assumed to survive: a second build taking over the resource type must not assume it can interpret or free data the previous build allocated (different compiler, allocator, or layout). Outside the `raw` feature this is a core safety invariant — see `docs/UPGRADE.md`. The module and the resource *type* survive reload; the Rust-typed *payload* is the part under the ABI constraint.

---

## Erlang-side usage

```erlang
-module(my_module).
-on_load(init/0).
-export([new/0, put/3, get/2]).

init() ->
    erlang:load_nif(filename:join(code:priv_dir(my_app), "native/my_nifs"), 0).

new()          -> exit(nif_not_loaded).
put(_K, _V, _M) -> exit(nif_not_loaded).
get(_K, _M)     -> exit(nif_not_loaded).
```

The opaque reference behaves like any Erlang term:

```erlang
M = my_module:new(),
my_module:put(<<"key">>, <<"value">>, M),
{ok, <<"value">>} = my_module:get(<<"key">>, M),

%% Send to another process — the reference is copied, the Rust data is not
Pid ! {map, M},

%% When M goes out of scope and is GC'd, the Rust destructor fires
```
