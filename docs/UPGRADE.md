# NIF Memory and Hot Code Upgrade

Erlang's defining operational feature is upgrading a running system without
stopping it. A NIF library participates in that story, and doing so correctly
means understanding three things: where the memory a NIF allocates actually
lives, what the BEAM does to a NIF library when the module is reloaded, and the
**foreign-ABI boundary** the upgrade introduces between two builds of the library.

This document is the reference for that model. It is deliberately precise about
the failure modes, because the most dangerous ones are invisible in Rust source.

> **Core safety paradigm.** Outside the `raw` feature, otter must never assume that
> two builds of a library share an allocator, a std-datatype layout, or even a
> layout for identical source compiled by different Rust implementations. The
> upgrade boundary is a foreign-ABI boundary. This is a project-wide invariant — see
> `otter/DESIGN.md` "Core safety invariant" and the root `CLAUDE.md`. §7–8 below are
> the detailed justification and what the safe path therefore requires.

> **Status.** otter exposes upgrade support in **three tiers** (§8): tier 1 (the safe
> default — module and resource-*type* survival, no cross-build payload survival) is
> **shipped** (issue `audit-02`, closed): every module gets generated
> `load`/`upgrade`/`unload`, an otter-owned `PrivData` registry, and the per-build ABI
> name (`abi.rs`). Tier 2 is the `raw` escape hatch — its `load_raw`/`upgrade_raw`/
> `unload_raw` callbacks (which hand the user the `priv_data` `void*` directly) are
> **shipped behind otter's `raw` feature**; without it the `_raw` `init!` keys are
> rejected at compile time. Tier 3 (the fingerprint
> + `EnifAlloc` sandbox that recovers payload survival safely) is planned. The BEAM
> *semantics* in this document are stable and, where noted "verified", checked against
> `erts/emulator/beam/erl_nif.c`.

---

## 1. The module-instance model

A loaded NIF library is **tied to one Erlang module instance**, not to the module
name. When you load a new version of a module, the code server holds two
instances at once — "current" (new) and "old" — and each has its own library
load. They coexist until the old one is purged.

The BEAM drives a library through three C callbacks (declared via `ERL_NIF_INIT`;
the historical `reload` slot has been unsupported since OTP 20 and is always
`NULL`):

| Callback | When it runs | Signature shape |
|---|---|---|
| `load` | A library is loaded and **no** previously loaded library exists for this module | `(env, void** priv_data, load_info)` |
| `upgrade` | A library is loaded and **old code of this module still has a library loaded** | `(env, void** priv_data, void** old_priv_data, load_info)` |
| `unload` | The module instance the library belongs to is **purged** as old | `(env, void* priv_data)` |

Two facts from this table do most of the work in the rest of the document:

- **The discriminator between `load` and `upgrade` is whether old code already
  has a library loaded — *not* whether the `.so` file differs.** A reload calls
  `upgrade`.
- **`upgrade` must not be `NULL`.** Per erl_nif: *"The library fails to load if
  `upgrade` returns anything other than 0 or if `upgrade` is NULL."* A NIF module
  with a `NULL` upgrade callback **fails `-on_load` on every reload** and takes
  the module down. This is the core of `audit-02`.

### Same `.so` or different `.so`?

When the new module instance loads its NIF library, it may load **a different
`.so`** or **the exact same one** — your choice of path decides:

- **Same path** (e.g. `l(Mod)` after recompiling in place): the dynamic library
  is *reused*. There is one mapped image and **one set of file-scope statics,
  shared between the two module instances.**
- **Different path** (e.g. a release upgrade that ships a version-specific `.so`):
  a fresh image is mapped, with **its own, independent statics** (initialized
  empty).

Both paths invoke `upgrade`. The difference matters because otter's resource-type
handle lives in a static (see §6): in the different-`.so` case the new image's static
starts empty and is populated by the upgrade's registration; in the same-`.so` case the
static is *shared* (already populated by the first load), yet the upgrade's registration
must still re-run `enif_init_resource_type` to transfer type *ownership* to the new
instance — and re-store the (takeover-preserved) handle. One mechanism — an
unconditional enif call on every `load`/`upgrade`, writing a plain mutable handle cell —
covers both.

---

## 2. Where NIF memory lives

There are three distinct places a NIF can put data, and they behave completely
differently across an upgrade:

1. **The Rust heap** — anything `Box`, `Vec`, `String`, `HashMap`, etc. allocates.
   This goes through Rust's **global allocator**, which by default is a thin
   passthrough to the system `malloc`/`free`. Memory here is process-heap memory;
   it is **not** part of any `.so` image.

2. **The VM allocator** — `enif_alloc` / `enif_free`. This is the *emulator's*
   allocator, a **single instance shared across the whole VM**, independent of
   any library. Memory here is the most portable across the upgrade boundary.

3. **BEAM-managed objects** — resource objects (`enif_alloc_resource`), binaries,
   and terms. These live in BEAM memory under GC / refcount control and are
   reached through handles, never owned by a particular `.so`.

The single most important consequence: **`.so` images do not own a private heap.**
Unloading a library (`dlclose`) unmaps its *code and static segments*; it does
**not** free the `malloc`'d or `enif_alloc`'d memory that library requested. Those
allocations persist in the shared process heap until something explicitly frees
them.

---

## 3. Crossing the library boundary

In one OS process there is one address space. A pointer made by library A is a
valid address in library B. The question is never "can B *see* the memory" — it
always can — but "can B *use* it safely." That splits into data and code.

### Data crosses freely — with an allocator caveat

- **`enif_alloc`'d memory: unconditionally shareable.** The VM allocator is one
  instance, so a block allocated by A can be read, written, and `enif_free`d by B.
  This is the robust channel.
- **Rust-heap memory: shareable for reads; freeable across libraries only if both
  use the same underlying allocator.** With the **default `System` allocator**,
  each `.so` statically links its own `__rust_alloc`/`__rust_dealloc` *symbols*,
  but both bottom out at the **shared libc** `malloc`/`free` — so B can free a
  block A allocated. With a **custom `#[global_allocator]`** (jemalloc, mimalloc,
  or even the same allocator statically linked twice with separate internal
  arenas), A's block lives in A's arena and B's free hits B's arena →
  **heap corruption.**

### Code pointers do **not** cross an unload

Anything that points *into a library's text segment* is valid only while that
library is mapped:

- function pointers,
- `dyn Trait` vtable pointers,
- `&'static str` / `&'static [u8]` into a library's read-only data,
- closures capturing the above.

When the originating library unloads, every such pointer dangles — even though the
surrounding heap bytes are still readable. **Reading the bytes is not the same as
the bytes still pointing at live code.** This single fact is why a resource type's
destructor — itself a code pointer into the library that created it — forces the BEAM
to either keep the old library mapped until its objects drain (clean separation) or
transfer the destructor to the new build (takeover). Both are developed in §5–6.

---

## 4. `priv_data`: the explicit handoff channel

`priv_data` is a single `void*` slot **per module instance** — the library's
private state, retrieved anywhere you hold an env via `enif_priv_data(env)`. It
exists precisely *because* static data is not a safe cross-instance channel
(either unshared across a different `.so`, or unintentionally shared across a
reused one).

Lifecycle:

- **`load`**: `*priv_data` starts `NULL`. Allocate state, fill it in, set
  `*priv_data`.
- **NIF calls**: `enif_priv_data(env)` returns it.
- **`upgrade`**: `*priv_data` starts `NULL` (the new instance's own slot);
  `*old_priv_data` holds whatever the old instance last stored. You may write both.
- **`unload`**: receives *that instance's* `priv_data`. **The BEAM does not free
  `priv_data`** — it is an opaque `void*`. You must free it here, or it leaks.

Because the two slots belong to two distinct instances, the *clean and default*
ownership pattern is: **each instance frees its own.**

### otter's model: otter owns the slot

The lifecycle above is the raw enif contract. **otter does not hand the bare
`void*` to the user; otter owns it and points it at a `PrivData` struct** (shipped,
`priv_data.rs`), of which the user's state is one field:

```rust
#[repr(C)]
pub struct PrivData {
    magic: u64,                  // PRIV_MAGIC — frozen header word, read cross-build
    user_priv_data: *mut c_void, // the user's void* (tier 2 `raw`); null in tier 1
    registry: ResourceRegistry,  // build-private: TypeId -> *mut NifResourceType (§6)
}
```

The first two fields are a frozen `#[repr(C)]` header — the only bytes ever read
across the upgrade boundary (at fixed offsets), and only to hand the user back their
old `user_priv_data` on upgrade. Everything after is build-private and reconstructed
fresh by each build. This is *why otter needs no `static`s* for cross-version state.
Two consequences:

- **The resource-type registry lives here, not in a `static`.** `priv_data` is
  genuinely per-instance even when the `.so` (and its file-scope statics) are shared
  on a same-`.so` reload — so each module instance carries its *own* type pointers,
  set at its own `load`/`upgrade`. That removes the one shared `static` otter would
  otherwise have. (See §6 for how the registry is populated.)
- **The faithful `void**` mirror survives as the `user_priv_data` *field*, not the
  whole slot.** In tier 1 it is always null. Tier 2 (`raw`, shipped behind the feature)
  exposes it as a bare `void*` the user's `load_raw`/`upgrade_raw`/`unload_raw`
  set/carry/free — `&mut *mut c_void` handles to the new (and, on upgrade, the old
  build's) field, the latter reached through the frozen header. A typed
  `Priv<P>` lens and tier 3's fingerprinted, `EnifAlloc`-backed whole-struct treatment
  remain planned. otter always allocates/frees the enclosing `PrivData`.

The cost is that reading `priv_data` needs a module-bound env (`enif_priv_data`),
which propagates to resource creation — see §5.

### The flows, enumerated

A complete taxonomy of what an upgrade can do with the old instance's state.
The "who frees A's original" column is *determined* by the action, not free to
choose:

| # | Flow | What B does | Who frees A's allocation |
|---|---|---|---|
| 1 | None | nobody uses `priv_data` | — |
| 2 | Fresh-cold | B allocates its own, ignores A's | A's `unload` |
| 3 | **Migrate** (`code_change`) | B allocates its own, **copies/transforms out of** A's during the upgrade window | A's `unload` |
| 4 | Whole takeover | B adopts A's pointer as its own; nulls `*old_priv_data` so A's `unload` skips it | B's eventual `unload` |
| 5 | Partial takeover | B keeps some fields (a live fd, a running thread's context), rebuilds the rest, nulls only the taken fields in `*old_priv_data` | split |
| 6 | B loads no library | the new module instance is not a NIF module | A's `unload` |

Two rules make the difference between correct and broken:

- **Migrate must copy out, not alias.** B may read A's memory *only during the
  `upgrade` call* (A is still mapped then). Retaining a pointer into A's memory
  past A's unload is a use-after-free.
- **Takeover of Rust-heap state is allocator-conditional** (§3); takeover of
  `enif_alloc`'d state is unconditionally safe.

### Which flow is "Erlang"?

Only **migrate** (flow 3) maps onto a concept an Erlang programmer already holds:
it is `code_change/3` — old state in, transformed state out, each side owning its
own. Erlang has no notion of "reuse the old version's heap allocation and suppress
its destructor"; whole/partial takeover are C-level optimizations.

There are nonetheless legitimate reasons to take over rather than copy — all of
them "this state is a long-lived singleton whose lifetime should transcend any one
code version":

1. **Pointer identity already escaped** — a background thread spawned in `load`
   holds the address and runs across the upgrade; copy-and-free pulls the rug out.
2. **Live OS/external resources** — a connection pool, fd, mmap, device handle, or
   loaded model that must not be torn down (zero downtime is the whole point).
3. **Copy is prohibitively expensive** — a multi-gigabyte cache; copying doubles
   peak memory for the duration of the upgrade.

A `code_change`-shaped API captures even these safely: **returning the old owned
value unchanged *is* takeover**, without the raw `void*`-nulling footgun.

---

## 5. Resource objects across an upgrade

A resource is the well-behaved citizen of hot upgrade, because the object itself
lives in **BEAM memory**, not in a library's heap.

When library A creates a `ResourceArc<T>`:

- `enif_alloc_resource` puts the object in **BEAM-managed memory** (GC + refcount).
  otter writes the `T` *into* that object, so the `T`'s top-level struct lives in
  BEAM memory.
- Any heap the `T` *owns* (e.g. a `Vec`'s backing buffer) is a **separate Rust-heap
  allocation** reached through a pointer inside the struct.

### Creation requires an env

Because the type registry lives in `priv_data` (§4) rather than a `static`, obtaining
the `*mut NifResourceType` to allocate or decode a resource goes through
`enif_priv_data(env)` — so it needs a module-bound env:

- **In a NIF or callback**: always have one. Construction becomes `env.make_resource(val)`
  (replacing the env-less `ResourceArc::from(val)`); decoding already holds an env.
- **Off-thread / `OwnedEnv`**: a spawned thread has no module-bound env, and
  `enif_priv_data` does not work on an `OwnedEnv`. So a worker captures the handle
  *before* spawning — `let h = env.resource_handle::<T>();` — and creates with `h` on
  the thread. The capability is preserved; it is just made explicit, consistent with
  otter's "capture what you need, no ambient magic" stance.

### Two upgrade paths: takeover vs. clean separation

The BEAM offers two sound ways to handle a resource type across an upgrade, and which
one otter uses is decided by the registered name (§6, §8):

**Takeover** — *a managed transfer of ownership, not a copy.*
`enif_init_resource_type(... CREATE | TAKEOVER ...)` under a name that *matches* the
old type inherits all its objects; the **new** library's destructor is thereafter
called for them. This is only sound when the new build can interpret and drop the old
build's payload — i.e. an ABI-compatible build (§7). Verified mechanics: takeover sets
`type->owner = new_lib` and releases the old library's `dynlib_refc`, so **the old
library can unmap even while its old objects are still alive** — which is exactly why a
taken-over payload must contain no pointers into the old image.

**Clean separation** — *registering under a name that does **not** match creates a
fresh type and leaves the old one alone.* This is the safe default (tier 1). The old
type, its objects, and its destructor stay with the old library, and the BEAM keeps
that library mapped until they drain. Verified mechanics: each live object holds a
reference on `type->refc` (incremented per object); at purge (`unload_nif`,
5311–5328) the type is *not* freed while objects remain, so its `dynlib_refc` on the
old library is not released and `close_dynlib` is deferred. When the last object is
GC'd, the dtor runs **with `type->owner` still the old library** — the old build drops
its own payload — and only then is the library unmapped. No cross-build drop ever
occurs; the cost is that old handles are inert to the new code (a fresh type identity).

### What survives, concretely

Take a resource wrapping `Mutex<Vec<u8>>` created by build A:

- **Under clean separation (tier-1 cross-build default):** the object is *not* adopted
  by B at all. `enif_get_resource` in B fails (B registered a different type identity),
  so B never interprets A's `Mutex`/`Vec`. A's own destructor frees it when the handle
  is GC'd. **Nothing crosses — and nothing can corrupt**, because no allocator or layout
  is shared. The handle must be re-acquired after the upgrade.
- **Under takeover (same byte-identical image, or `raw`/tier-3):**
  - The resource object and the `Mutex`/`Vec` *structs* — BEAM memory, refcount intact;
    `enif_get_resource` in B succeeds because takeover preserved the type identity.
  - The `Vec`'s backing buffer — Rust heap; readable by B, freed correctly by B's
    inherited destructor **iff A and B share the global allocator** (§3) — which holds
    unconditionally only for the byte-identical image, otherwise requires tier-3
    `EnifAlloc`.
  - The `Mutex` itself — see §7.

---

## 6. Type registration with a reloadable NIF

### What erl_nif actually does (verified against erts source)

`open_resource_type` (`erts/emulator/beam/erl_nif.c`) keys every type on the pair
**`(module_atom, name)`** — `module_am` comes from the loading module, `name` is the
string otter passes. The create-vs-takeover decision is then purely a function of
whether that pair is already registered:

- **Not found** + `CREATE` → a new type is allocated (`op = CREATE`).
- **Not found** + no `CREATE` → returns `NULL` (failure).
- **Found** + `TAKEOVER` → the existing type is taken over (`op = TAKEOVER`).
- **Found** + no `TAKEOVER` → returns **`NULL`** — *`CREATE`-only on an existing name
  fails.* This is what makes a naive reload (or a second registration of the same name)
  blow up.

Two consequences settle otter's design:

1. **The name is the entire ABI-compatibility lever.** Since the key is `(module,
   name)` and the module atom is fixed, *the name otter registers under decides whether
   an upgrade takes over the old type or creates a fresh one.* This is what tiers 1 and
   3 (§8) exploit: a per-build-unique name never matches across builds (→ fresh type,
   clean separation), a fingerprint name matches exactly when takeover is sound.
2. **Takeover transfers ownership, and that transfer is mandatory on a same-`.so`
   reload.** `prepare_opened_rt` sets `type->owner = new_lib` on takeover. The new
   instance *must* re-run registration in `upgrade` for two reasons at once: it
   populates the new instance's own registry (§4) with the type pointer, **and** it
   moves ownership forward so that when the old library is purged, `unload_nif`
   (5311–5328) *skips* the type (`owner` no longer matches it) instead of freeing it
   out from under the new instance and any still-outstanding objects. So registration's
   enif call **must run in `upgrade`**, unconditionally.

### otter's registration (implemented — `audit-02`)

- **Always-generated `load` / `upgrade` / `unload`.** otter emits all three (non-`NULL`)
  for every module, so any otter module is hot-upgradeable. `load` holds one-time side
  effects that must not repeat (priv_data, threads, fds); `upgrade` is the distinct
  callback the BEAM requires (and the only one with `old_priv_data`); `unload` frees the
  `PrivData`. Each dispatches to an *optional* user callback of the same name.
- **Registration is list-driven, run in *both* `load` and `upgrade`.** The user lists
  types in `init!`'s `resources = [...]`; the scaffolding registers them in load and
  re-registers (takeover) in upgrade — the C idiom, automated. A user callback may still
  call `register::<T>(env, flags)` by hand for dynamic cases (the `PrivData` is published
  before the callback runs, so it lands in the live registry).
- **Flag by callback:** `CREATE` in `load`, `CREATE | TAKEOVER` in `upgrade`, passed
  explicitly by the generated wrapper (not inferred from `EnvKind`). `load` runs only
  when no old code of this module exists, so `CREATE`-only there is the *strict* choice
  (a collision is a real error, not silently absorbed). `upgrade` is where takeover is
  both possible and needed.
- **`EnvKind`** gains `Upgrade` and `Unload` variants (`Init` → `Load`); `register`
  asserts the env is `Load` or `Upgrade`. It gates *call legality* only.
- **The handle is stored in the per-instance registry inside `priv_data` (§4), not a
  `static`.** Each `register` call writes the returned `*mut NifResourceType` into *this
  instance's* `PrivData` registry, keyed by `T`'s `TypeId`; `ResourceArc` reads it back
  via `env → enif_priv_data → registry`. Because `priv_data` is per-instance, and the
  registry is built entirely within `load`/`upgrade` (before any NIF call on that
  instance) and read-only thereafter, there is **no `static`, no shared cell, and no
  atomics** — the same-`.so` shared-static hazard disappears, since each instance owns
  its registry.

This superseded the earlier `OnceLock`/`AtomicPtr` static-cell design and removed the
`Resource` trait's `resource_type_handle()` static accessor: the type pointer is reached
through the env-bound registry, not a global. The enif call runs every time, for the
ownership transfer above.

---

## 7. The ABI contract (read this twice)

Everything above that "survives" survives **only if the new library is ABI-compatible
with the old one** for any state that crosses the boundary. Three seemingly separate
pitfalls are all the same contract — and they are **exactly the three assumptions
otter forbids** outside the `raw` feature (the core safety paradigm above):

| Pitfall | The assumption it would require otter to make |
|---|---|
| `#[repr(Rust)]` layout is unspecified across compilations | Same `rustc` version. `repr(Rust)` layout is deterministic per version but undefined across versions; `-Z randomize-layout` breaks even same-version. |
| Allocator is not guaranteed identical | Same global allocator, so Rust-heap payloads remain freeable by the new build. |
| `Resource: Send + Sync` forces `Mutex`/`RwLock`/`Atomic` state | Same `rustc` again — so the lock's internal *protocol and state machine* (the futex word, the poison flag), not just `T`'s field order, are interpreted identically. |

So the one-sentence reality is:

> **A NIF resource survives hot upgrade only if the two builds are ABI-compatible:
> same `rustc`, same global allocator, no layout randomization.** Safe otter is **not
> permitted to assume this** — relying on it is a `raw`-feature posture. The safe path
> (§8) must make survival hold *without* the assumption.

### Do synchronization primitives survive?

Because `Resource` is `Send + Sync`, mutable resources are built from atomics or
locks. On modern std they survive, and — importantly — **without dragging in an
allocator dependency**:

- **Atomics** (`AtomicU64`, …): the cleanest case. Inline, well-defined layout, no
  pointers, no handle. They survive; B operates on the same memory location with
  the same instructions.
- **`Mutex` / `RwLock`** on modern std (Linux, `rustc ≥ 1.62`): **futex-based and
  fully inline** — an `AtomicU32` plus a poison `AtomicBool` plus the inline
  `UnsafeCell<T>`. **No heap `Box`, no `pthread_mutex_t`, no OS handle.** They
  survive under the same-toolchain assumption, because B interprets the in-place
  futex word and poison flag with its own code, which must match A's protocol.
  *(Pre-1.62 std boxed a `pthread_mutex_t`, which would have reintroduced both an
  allocator dependency and an OS-handle concern.)*
- **The data inside the lock** (`Vec` inside `Mutex<Vec>`) keeps its own Rust-heap
  allocator pitfall — the lock wrapper neither adds nor removes it.

But "survive under the same-toolchain assumption" is *still* an assumption — exactly
the one the paradigm forbids outside `raw`. So even atomics and futex locks are only
*relied upon* across an upgrade on the `raw` path or behind the fingerprint check
(§8); safe otter treats their cross-build survival as unproven, not as a guarantee.

### The Rust-specific trap

This ABI contract is the same one *every* native hot-upgrade system has — C NIFs
need matching struct layout, a shared allocator, and a compatible libpthread to
interpret a mid-flight mutex. The difference is **visibility**. In C you write
layouts explicitly and know you are committing to an ABI. In Rust, `#[repr(Rust)]`
makes the dependency **invisible in the source**: nothing in
`struct State { data: Mutex<Vec<u8>> }` signals "this only survives upgrade against
an ABI-identical build." A team that bumps its pinned `rustc` between two releases
gets no warning that it just broke hot upgrade. **That silence is the hazard, not
the mechanism.**

---

## 8. Tiers of upgrade support

otter exposes hot upgrade at **three tiers**. They differ only in *how much state is
allowed to cross the upgrade boundary* and *what guarantees that crossing is sound*.
All three ride the same BEAM mechanics (§5–6); the single knob that distinguishes
them is the **name a resource type is registered under** (and, for `priv_data`,
whether a layout fingerprint is checked).

### Tier 1 — module and resource-*type* survival (the safe default)

**Survives:** the module reloads, and resource *types* survive — so new resources
work and the same-image reload keeps existing handles valid. **Does not survive:**
Rust-typed resource *payloads* and `priv_data` are not carried across a *cross-build*
upgrade.

The mechanism is the BEAM's own `(module, name)` resource lookup (§6, verified against
erts source). otter registers each type under a **per-build-unique name** — in effect
a *maximally conservative fingerprint that compares equal only for the byte-identical
library image*. Concretely (`abi.rs`), the default name is
`"{type_name}#abi={hash}"`, where `{hash}` is a `DefaultHasher` digest of this
library's own binary, located at load time via `dladdr` over an otter function address
and read from disk (cached; a per-load fallback on failure, which degrades to "never
take over"). A type may instead be registered `"{type_name}#tag={tag}"` via
`register_tagged` / `resources = [T: "tag"]`, a stable name with no hash that opts the
type *into* cross-build takeover (the per-type analog of `raw` — a promise its layout is
stable). Registration uses `CREATE` in `load`, `CREATE | TAKEOVER` in `upgrade`. The
lookup then routes each case automatically:

- **Same `.so` reloaded in place** (`l(Mod)`): same build ⟹ same name ⟹ the lookup
  matches ⟹ **takeover**. Sound, because the two instances are the *identical image* —
  same layout, same allocator, same code. Ownership transfers to the new instance and
  existing objects keep working. (The enif call must run on this path to perform that
  ownership transfer — see §6.)
- **Different `.so`** (a recompiled release, even of identical source): different build
  ⟹ different name ⟹ no match ⟹ **a fresh type is created**. The old type, still owned
  by the old library, is untouched; its objects drain through the *old* library's own
  destructor, which the BEAM keeps mapped until the last one dies (§5, verified). Old
  handles become inert to the new code — recreate them. This is `code_change`
  semantics: pre-upgrade state is not carried in place.

No ABI assumption is ever made: the *only* case that takes over — and therefore drops
the old build's payloads with the new build's code — is the one where the builds are
byte-identical. Every other case degrades to clean separation. This is the non-`raw`
default and what `audit-02` ships.

**Why payloads can't cross here.** Because a cross-build upgrade creates a *fresh*
type, a `ResourceArc<Mutex<Vec<u8>>>` created by the old build simply fails to decode
under the new build's type (`enif_get_resource` mismatch, §5). The data is cleanly
destroyed by the old build; the new build never interprets it. There is no allocator
or layout hazard *precisely because nothing is shared.*

**Where the type pointers live.** otter owns `priv_data` (§4) and keeps each instance's
resource-type registry *there*, not in a `static` — so even the same-`.so` reload, which
shares file-scope statics, still gives each instance its own type pointers, set at its
own `load`/`upgrade`. The trade is that resource creation needs a module-bound env
(§5).

### Tier 2 — raw `void**` callbacks (`raw` feature)

Full enif fidelity, fully unsafe. The user writes `load` / `upgrade` / `unload` and
gets their state as a raw `void*` — the `user` field of otter's per-instance struct
(§4), with full `void**`-style semantics (set in `load`, carry or null in `upgrade`,
free in `unload`). They may register types under a *stable* name with `TAKEOVER` to
keep payload and `priv_data` continuity, accepting the ABI contract (§7) as their
responsibility. otter makes no safety guarantee on this path; it is the documented
escape hatch for users who pin their toolchain and own the consequences.

> **Open:** whether a pure-`raw` NIF that registers *no* otter resource types gets the
> literal enif `priv_data` slot (otter stepping back entirely), or always the nested
> `user` field. Leaning toward otter owning the slot uniformly, for one model.

### Tier 3 — the safe sandbox (planned)

Recovers continuity *without* any ABI assumption, by replacing tier 1's crude
byte-identical fingerprint with a **real ABI fingerprint** over **enif-allocated,
code-pointer-free** payloads. Two pieces working together:

- **Fingerprint *as* the resource type name.** Register each type under a name derived
  from a fingerprint of its layout. Then `CREATE | TAKEOVER` self-dispatches through
  the *same* `(module, name)` lookup with **no manual check**: matching fingerprint ⟹
  provably ABI-compatible ⟹ sound takeover (continuity preserved); differing
  fingerprint ⟹ clean separation. The VM performs the ABI-compatibility test for free.
  **Tier 1 is literally this mechanism with the fingerprint pinned to "byte-identical";
  tier 3 widens the equal-set to "provably ABI-compatible."** Same code path, the
  conservatism knob turned from maximal to precise.
- **enif-backed allocation.** Every allocation a crossable payload owns, *transitively*,
  comes from the VM allocator (an `EnifAlloc` ZST over `allocator-api2`, since
  `allocator_api` is nightly), so the new build can free the old build's bytes through
  the one shared free path — neutralizing the allocator assumption even after the old
  library unloads. The `#[global_allocator]` half of this — `EnifAlloc: GlobalAlloc`,
  opt-in via `otter::enif_global_allocator!()` — is **shipped** (`alloc.rs`, issue
  `upgrade-04`); it routes *all* of a NIF's Rust allocations through `enif_alloc`. The
  per-type *scoped* `allocator-api2` `Allocator` impl and the fingerprint remain planned.

The fingerprint and `EnifAlloc` apply to the whole `PrivData` struct (§4) — the
registry plus the user payload — as one enif-allocated, fingerprinted unit. The
resource-type registry rides the `(module, name)` name-matching automatically; the
**user payload** has no name-matching to ride on, so tier 3 checks the fingerprint
**manually** at the single cross-build read site (`old_priv_data` in `upgrade`): match
→ interpret and adopt; mismatch → free the raw (enif-allocated) bytes and rebuild from
Erlang-side state.

**Fingerprint soundness conditions** (each verified in §5–7):

- **Conservative and asymmetric.** Over-declaring *difference* is safe (it falls back
  to clean separation); over-declaring *sameness* is catastrophic (unsound takeover).
  When in doubt the fingerprint must *differ*. It therefore folds in `rustc` version,
  codegen flags, and target — not just structural layout, since `#[repr(Rust)]` layout
  is undefined across compilations.
- **No image-relative pointers.** A payload containing `fn`, `dyn`, or `&'static` has
  identical *layout* across builds but pointer *values* into the originating image;
  takeover unmaps that image while the objects are still live (§5, verified), so the
  pointers dangle. A layout fingerprint cannot see this — so participating types must
  be restricted to self-contained data. `abi_stable`'s `StableAbi` derive both computes
  the layout fingerprint *and* enforces this restriction; use its **derive, not its
  containers** (its containers use the global allocator and embed dealloc vtables that
  dangle on unload).

Until tier 3 ships, **cross-build survival of Rust-typed state lives only behind `raw`**
(tier 2). And one rule holds at every tier: **state that crosses an upgrade must not
embed code pointers** — they dangle the moment the originating library unloads,
independent of allocator or layout.

---

## See also

- `docs/RESOURCES.md` — the resource lifecycle in normal (non-upgrade) operation.
- `otter/DESIGN.md` — layering and the safe-layer architecture.
- erl_nif reference: `_oss/otp-doc-29.0.2/erts-17.0.2/doc/html/erl_nif.md`
  (`load` / `upgrade` / `unload`, `enif_init_resource_type`, `enif_priv_data`).
- erl_nif source (the "verified" mechanics): `~/dev/erlang/otp/erts/emulator/beam/erl_nif.c`
  — `open_resource_type` (≈2659), `prepare_opened_rt`/`steal_resource_type` (≈2582,
  2769), the resource dtor path (≈2978), and `unload_nif` (≈5300).
