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

> **Status.** The mechanism described here (a generated `upgrade` callback and
> takeover-aware resource registration) is the subject of issue `audit-02`. The
> *semantics* in this document are the BEAM's, and are stable; the otter API
> surface that exposes them is noted as **planned** where it is not yet shipped.

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
handle lives in a static (see §6): in the different-`.so` case the new image's
static starts empty and must be *populated* by takeover; in the same-`.so` case
it is already populated and is *overwritten*. One mechanism must cover both.

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
the bytes still pointing at live code.** This single fact is the reason resource
types need *takeover* (§6): a resource's destructor is a code pointer into the
library that created it.

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

### Takeover and postponed unload

The BEAM's upgrade mechanism for resources is a **managed transfer of ownership**,
not a copy:

- `enif_init_resource_type(... CREATE | TAKEOVER ...)` in the new library **takes
  over** the existing type and inherits all its objects. The **new** library's
  destructor is thereafter called for those objects.
- **Unloading the old library is postponed** as long as resource objects with a
  destructor in it exist. So there is never a window where a live object's
  destructor points into an unmapped `.so`.

This is the "drain" that production hot-upgrade relies on: objects stay put in host
(BEAM) memory, and only *responsibility* (the destructor — a code pointer) moves
from A to B. No per-object copy occurs.

### What survives, concretely

Take a resource wrapping `Mutex<Vec<u8>>` created by A, after B takes over:

- **The resource object** (and the `Mutex`/`Vec` *structs* inside it) — BEAM memory,
  refcount intact, the Erlang-side term still references it. **Survives
  unconditionally.** `enif_get_resource` in B succeeds because takeover preserved
  the type identity.
- **The `Vec`'s backing buffer** — Rust heap. Readable by B (survives even A's
  unload, being libc-heap memory). Freed correctly by B's inherited destructor
  **iff A and B share the global allocator** (§3).
- **The `Mutex` itself** — see §7.

---

## 6. Type registration with a reloadable NIF

For a resource type to be takeable-over, registration must:

1. Run inside `load` **and** `upgrade` (the only callbacks where
   `enif_init_resource_type` is legal), and
2. Pass `ERL_NIF_RT_CREATE | ERL_NIF_RT_TAKEOVER` — create on first load, take over
   on every subsequent one — which is also what fixes registration being called
   twice when two Erlang modules load the same `.so`.

The handle the call returns (`*mut ErlNifResourceType`) must be stored in a slot
that is **writable**, not write-once: the different-`.so` upgrade case populates a
fresh-empty slot, the same-`.so` case overwrites an existing one. A write-once cell
(`OnceLock`) panics on the second registration — the second half of `audit-02`.

> **Planned (audit-02).** otter's resource-type handle moves from
> `OnceLock<ResourceTypeHandle>` to a writable atomic cell, registration switches
> to `CREATE | TAKEOVER` with store-not-set semantics, and the `init!` macro
> generates a non-`NULL` `upgrade` callback (its extra `old_priv_data` argument is
> threaded but, by default, ignored). The user's `load` function is re-run on
> upgrade, which re-registers each type — now an idempotent takeover rather than a
> panic.

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

## 8. Guidance

It is tempting to note that the common operational case — recompile the same crate
with the same pinned toolchain, no custom allocator, `l(Mod)` — *appears* to work end
to end: the module survives, types take over, inline state persists, Rust-heap
payloads are freed correctly. **otter does not treat this as a supported safe path.**
It works only by making the three assumptions the paradigm forbids; nothing in the
source signals the dependency, and a toolchain or allocator change between releases
breaks it silently. Relying on it is therefore a **`raw`-feature posture** — available
to users who opt out of the safety guarantee and take responsibility — not the default.

The safe path makes cross-build survival hold *without* any ABI assumption, by two
requirements working together (the tier-3 sandbox; **planned**, to be designed
alongside `audit-02`):

- **enif-backed allocation.** Any state that crosses the upgrade boundary is allocated
  from the BEAM allocator — a single VM-wide instance reached through the same shared
  free function in every build — not the Rust global heap. This neutralizes the
  allocator assumption: the bytes are always safely freeable by the other build, even
  after the originating library unloads.
- **ABI fingerprinting.** The data carries a fingerprint of its layout, checked at the
  one cross-build read site (`old_priv_data` in `upgrade`, and takeover of a resource
  payload). Match → safe to interpret and drop with the new build's code. Mismatch →
  the new build must not interpret it; the only safe action is to free the raw bytes
  (which enif-backing always permits) and rebuild from Erlang-side state.

Together these make the no-assumptions paradigm hold by construction: the allocator
requirement covers assumption (a); the fingerprint covers (b) and (c).

Until that mechanism ships, **cross-build survival of Rust-typed state is supported
only behind `raw`**, where the user accepts the ABI contract. The non-`raw` default is
the safe, conservative one: the module and resource *types* survive reload (the
`audit-02` machinery), but Rust-typed resource payloads and `priv_data` are not
assumed to survive a non-ABI-identical upgrade.

And one rule that holds regardless: **state that crosses an upgrade must not embed
code pointers** (`Box<dyn Trait>`, function pointers, `&'static` references into the
library image) — those dangle the moment the originating library unloads, independent
of allocator or layout.

---

## See also

- `docs/RESOURCES.md` — the resource lifecycle in normal (non-upgrade) operation.
- `otter/DESIGN.md` — layering and the safe-layer architecture.
- erl_nif reference: `_oss/otp-doc-29.0.2/erts-17.0.2/doc/html/erl_nif.md`
  (`load` / `upgrade` / `unload`, `enif_init_resource_type`, `enif_priv_data`).
