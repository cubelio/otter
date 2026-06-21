# otter: Core Rust NIF Library

## Purpose

`otter` is a Rust library for writing Erlang NIFs (Native Implemented Functions) in safe Rust. It provides a direct, honest mapping of the Erlang NIF C ABI into Rust types, with no abstractions that don't have a clear Erlang equivalent.

**Design principle:** Writing a NIF with otter should feel like working directly with Erlang. If an Erlang programmer would not recognize a concept, it does not belong in this library.

---

## Core safety invariant: no cross-build ABI assumptions

Erlang's defining feature is upgrading a running system in place. A NIF library must survive that: a *second* build of the library can be loaded beside the first and inherit its live state (resource objects, `priv_data`). The two builds may be produced by different compiler versions, with different allocators, and need not be byte-identical source.

Therefore, **outside the `raw` feature, otter must never assume any of the following across the hot-upgrade boundary:**

1. **Allocator / drop compatibility** — that memory allocated (or a value dropped) by one build can be freed (or have its `Drop` glue run) by the other. Custom global allocators, and even toolchain differences in how the default allocator handles alignment, make this unsound.
2. **Std datatype layout** — that `Vec`, `String`, `Mutex`, … have the same in-memory layout in both builds. `#[repr(Rust)]` layout is unspecified across compiler versions.
3. **Same-source layout** — that *identical source* compiled by two different Rust implementations (or the same one with layout randomization) produces layout-compatible structs.

**The upgrade boundary is a foreign-ABI boundary.** This governs every piece of Rust state that can outlive a single code version:

- `priv_data` passed through `load`/`upgrade`/`unload`,
- resource payloads inherited via resource-type takeover,
- the data behind any datatype handed to Erlang from a NIF (a resource handed back *is* such state).

Code that relies on ABI compatibility between builds is unsound in safe otter and belongs only behind the `raw` feature, where the user takes responsibility. The safe path must make the no-assumptions hold *by construction* — enif-backed allocation (one shared VM allocator) plus an ABI fingerprint checked at the cross-build read site. See `docs/UPGRADE.md` for the full treatment.

---

## Layer Structure

The raw C ABI floor (types, `enif_*` shims, the load-time loader) lives in the
external **`enif-ffi`** crate; everything under `otter/src/` is the safe layer
above it.

```
otter/src/
├── types/      The Env/Term spine: the Env + Term traits and env kinds, the
│   │           OwnedEnvArena tier, and Raised (mod.rs); term verbs not owned by
│   │           one type (ops.rs); TypedTerm + resolve (typed.rs); BinaryBuf
│   │           (binarybuf.rs); one file per concrete term type (terms/*.rs)
│   └── …
├── codec/      Encoder + Decoder + CodecError (mod.rs); native-Rust-type impls
│               per submodule (integer, float, bool, string, tuple, list, map, bigint)
├── resource.rs Resource trait, ResourceArc<T>, Monitor, register, dynamic_resource_call
├── priv_data.rs PrivData + ResourceRegistry (per-build type table)
├── abi.rs      build ABI fingerprint (the resource-takeover suffix)
├── alloc.rs    EnifAlloc global allocator (enif_global_allocator!)
├── time.rs     BEAM monotonic time, time offset, unit conversion
├── system.rs   Thread type introspection, system info
├── select.rs   I/O event multiplexing (enif_select / enif_select_x)
└── __codegen.rs  internals the macros emit against
```

---

## Layer 1: Raw C ABI floor — the `enif-ffi` crate

otter's raw floor is the external **`enif-ffi`** crate — a thin, 1:1, all-`unsafe`
binding to the `enif_*` C API. It supplies the three things otter once carried in
its own `sys.rs`/`enif.rs` and now consumes wholesale; otter calls `enif_ffi::*`
directly throughout the safe layer.

1. **Raw types and constants** — the `#[repr(C)]` transcription of `erl_nif.h`:
   `enif_ffi::{Term, Env, Func, Entry, Binary, Pid, Port, Monitor, ResourceType,
   ResourceTypeInit, MapIterator, TermType, SysInfo, Event, …}`; the flag newtypes
   (`SelectFlags`, `ResourceFlags`, `UniqueInteger`) with their scoped constants
   (`SelectFlags::READ`, …) and `BitOr`; and the standalone constants
   (`SELECT_*`, `THR_*`, `DIRTY_JOB_*`, `BIN2TERM_SAFE`, `TIME_ERROR`, …).

2. **Shim functions** — one `unsafe fn` per `enif_*`, calling through a load-time
   function-pointer table with the `enif_` prefix dropped (`enif_ffi::is_atom`,
   `enif_ffi::make_atom`, …). The macro-only C entry points (`make_tupleN`,
   `select_read/write/error`, `set_option_*`) are reimplemented over the real
   functions; the variadic `printf` family is intentionally left unwrapped, since
   its `va_list` form is unrepresentable on stable Rust.

3. **Symbol resolution + the entry point** — `enif_ffi::nif_init!` emits the
   platform-correct `nif_init` and resolves the `enif_*` table at load: `dlsym`
   on Unix, the BEAM-supplied callback table on Windows. **Both platforms are
   supported.** Minimum version: NIF 2.17 (OTP 26), with opt-in `nif_2_18`.

otter's generated `nif_init` invokes `enif_ffi::nif_init!` (see
`otter_codegen/DESIGN.md`). The crate is re-exported as `otter::enif_ffi` **only
under the `raw` feature** — the deliberate escape hatch (enhance-11, shipped).
Generated code no longer names that path: the raw types it puts in the `extern "C"`
signatures it emits come from the always-present `otter::__codegen::ffi::*` shim,
and the enif types that appear in otter's own *safe* public API are re-homed onto
their modules (`select::{Event, SelectFlags, SELECT_*}`, `types::{TermType, Hash,
UniqueInteger}`, `resource::ResourceFlags`). So nothing in the safe surface needs
`raw`. The one piece otter keeps in-tree is `alloc.rs`, which **direct-links**
`enif_alloc`/`enif_free` rather than going through the resolved table — the global
allocator may run before `nif_init`, so it cannot depend on the resolution step.

---

## Layer 2: The safe layer

Above the enif-ffi floor is the entire Erlang-facing surface, and it audits as safe — every `unsafe` FFI call goes through an `enif_ffi::*` shim.

The organising principle is **a branded term carries only its env's brand, never the env itself; operations take the env explicitly.** `Env<'id>` and `Term<'id>` are sealed *traits* (Layer 3, Layer 4), and the brand `'id` ties a term to the env that produced it. So:

- **Term constructors** are associated functions taking an env: `Integer::from_i64(env, n)`, `Binary::from_bytes(env, bytes)`, `Map::new(env)`, `Tuple::from_terms(env, …)`.
- **Accessors** take an env: `bin.as_bytes(env)`, `i.to_i64(env)`, `map.get(env, key)`, `term.resolve(env)`.
- **Generic verbs** that work on any term are default methods on the `Env` trait: `env.term_type(t)`, `env.hash(…)`, `env.make_unique_integer(…)`.
- **Context verbs** are inherent on the env *kind* that has the context: `CallEnv::{raise, badarg, cpu_time, schedule_nif}`, `InitEnv::set_option_*`.
- **Verbs owned by no single term** are free functions in `ops.rs`: `serialize`/`deserialize`, `send_from`, `port_command` (re-exported from `types`).
- **Env-less value ops** stay value-type methods: a term's `Ord`/`Eq` (via `enif_compare`/`enif_is_identical`), the `BinaryBuf` buffer ops.

Term inputs are taken as `impl Term<'id>` (Layer 4), so a term from another env's brand is rejected at compile time. Each verb lives next to its subject — per-type methods on that type's file in `types/terms/`, the shared verbs in `types/mod.rs` and `types/ops.rs`.

The optional sync/thread/IO-queue tier and the deliberately-unsafe set (`enif_alloc`/`dlsym`/`fprintf`/…) have **no** safe wrapper — they are reachable only through the raw `enif-ffi` crate (re-exported as `otter::enif_ffi`, under the `raw` feature).

---

## Layer 3: Environment (`types/mod.rs`)

### `Env<'id>` — a trait, with a generative brand

The central lifetime-safety mechanism. `Env<'id>` is a **sealed trait** (`Copy`), and `'id` is a *generative brand* — invariance via `Invariant<'id> = PhantomData<*mut &'id ()>`. Unlike rustler's per-call lifetime synthesized from a stack borrow, otter mints the brand through a `for<'id>` closure: each entry point introduces a fresh, non-escaping `'id`, so a term branded by one env cannot be used with another (the GhostCell construction).

```rust
pub trait Env<'id>: Copy + sealed::Sealed {
    fn raw_env(&self) -> *mut enif_ffi::Env;
    fn as_any_env(self) -> AnyEnv<'id> { … }      // kind-erased handle terms build against
    // default verbs valid on any env:
    fn term_type(self, term: impl Term<'id>) -> Option<TermType> { … }
    fn hash(self, algorithm: Hash, term: impl Term<'id>, salt: u64) -> u64 { … }
    fn consume_timeslice(self, percent: i32) -> bool { … }
    fn is_current_process_alive(self) -> bool { … }
    fn make_unique_integer(self, properties: UniqueInteger) -> Integer<'id> { … }
}
```

The VM hands a call or callback a raw env pointer; codegen wraps it in the matching **concrete kind**, so context-specific verbs are typed and the wrong kind is a compile error (this is the lifted, type-level form of the old `EnvKind` enum). Each kind is structurally an `AnyEnv` (one pointer + the ZST brand) and differs only as a type:

- `AnyEnv<'id>` — kind-erased storage (list/map iterators hold it).
- `CallEnv<'id>` — the process-bound env handed to a NIF call.
- `InitEnv<'id>` — the `load` / `upgrade` callbacks.
- `CallbackEnv<'id>` — resource callbacks (`destructor`/`down`/`stop`).
- `DeinitEnv<'id>` — the `unload` callback.
- `OwnedEnv<'a, 'id>` — the process-independent arena env (below).

Each is entered through an `unsafe` `with_*_env` function that mints the brand:
`with_call_env`, `with_init_env`, `with_callback_env`, `with_deinit_env` — each takes `raw` + a `for<'id> FnOnce(KindEnv<'id>) -> R` closure. The grouping trait **`CallingEnv<'id>: Env<'id>`** (implemented by `CallEnv` and `CallbackEnv`) marks the envs that carry a live process/scheduler context — the ones you can `send_from` / `port_command` *from*, with caller attribution. Context verbs sit inherent on the kind that has the context: `CallEnv::{raise, badarg, check_raised, cpu_time, schedule_nif}`, `InitEnv::set_option_*`.

### The owned-env arena: `OwnedEnvArena` / `OwnedEnv` / `OwnedEnvTerm`

Building and sending a message from outside a NIF call (e.g. a spawned OS thread) uses a *reusable* process-independent env arena:

```rust
pub struct OwnedEnvArena { /* owns an enif_alloc_env env + a generation stamp */ }

impl OwnedEnvArena {
    pub fn new() -> Self;                                   // also Default
    pub fn run<R>(&mut self, f: impl for<'id> FnOnce(OwnedEnv<'_,'id>) -> R) -> R;
    pub fn clear(&mut self);                                // wipe the heap, reuse
    pub fn copy_in (&mut self, term: impl Term<'_>) -> OwnedEnvTerm;
    pub fn copy_out(&self, oterm: OwnedEnvTerm, env: impl Env<'_>) -> AnyTerm<'_>;
}

// OwnedEnv<'a,'id> — the branded env handed to `run`'s closure; it is an Env<'id>
//   like any other (generic verbs work) plus: export(term) -> OwnedEnvTerm,
//   import(oterm) -> AnyTerm<'id>.
// OwnedEnvTerm — Copy, unbranded, portable handle to a term stored in the arena.

// the message is sent with a free verb that steals the arena's heap (O(1)):
pub fn send(pid: &LocalPid, env: &mut OwnedEnvArena, term: OwnedEnvTerm) -> bool;
```

You build terms inside `arena.run(|oenv| …)` — the closure's branded `OwnedEnv` keeps them from escaping — and `export` the one you want to an unbranded `OwnedEnvTerm`. `send(&pid, &mut arena, oterm)` transplants the arena's heap into the message (`enif_send` with a non-NULL `msg_env`) and marks the arena dirty; `clear` resets it for reuse.

`OwnedEnvTerm` is `Copy` and unbranded so it can cross the closure boundary / be carried to a worker thread, so it cannot use the brand to prove validity. Instead it records a **process-global monotonic `AtomicU64` generation stamp** taken at `new`/`clear`; a use against the wrong arena-generation fails a single `u64` compare (the UAF fix — the prior env-pointer-plus-version guard could alias a freed-then-reused env). For an in-NIF live send with caller attribution, use the free verb `send_from(env, &pid, msg)` instead (no arena needed).

---

## Layer 4: Terms (`types/`)

### Three levels of resolution

**Level 1 — `AnyTerm<'id>`:** The bare machine word, carrying only the brand `'id` (the env is *not* stored). `#[repr(transparent)]` over the raw word, so a `&[RawTerm]` can be viewed in place as `&[AnyTerm<'id>]` (used by `TupleView`). Zero work done — the fastest representation. `Term<'id>` is the *trait* every term implements (`raw_term`, `copy_to`).

**Level 2 — `TypedTerm<'id>` enum:** One `enif_term_type` call has been made (`term.resolve(env)`). The correct variant is known. Data is still on the BEAM heap.

```rust
pub enum TypedTerm<'id> {
    Atom(Atom), Bitstring(Bitstring<'id>), Float(Float<'id>),
    Fun(Fun<'id>), Integer(Integer<'id>), List(List<'id>),
    Map(Map<'id>), Pid(Pid<'id>), Port(Port<'id>),
    Reference(Reference<'id>), Tuple(Tuple<'id>),
}
```

11 variants for 11 type tags — `Bitstring` covers both byte-aligned binaries and sub-byte bitstrings; refine to a `Binary` with `Bitstring::to_binary` (or `is_binary`).

`TypedTerm` implements `PartialEq`/`Eq` (via `enif_is_identical`) and `PartialOrd`/`Ord` (via `enif_compare`); the concrete types carry their own such impls. All concrete types implement `From<T> for TypedTerm<'id>`, so `let t: TypedTerm = atom.into()` works. Resolution is `AnyTerm::resolve(self, env) -> Option<TypedTerm<'id>>` (`None` for a type code this otter build does not recognize); `TypedTerm` is also a `Decoder` (decode = `resolve().ok_or(UnknownTermType)`). There is no `TryFrom<Term>`.

**Level 3 — concrete types:** Type is known. Data is still on the BEAM heap. Accessor methods (each taking an env) pull data out on demand.

### Lazy by default

Construction is always free. Extraction is on demand. Every concrete type is one machine word plus the ZST brand marker — `{ raw_term: RawTerm, _id: Invariant<'id> }`. No data is read from the BEAM heap until explicitly requested with an env-passing accessor.

### Brand rules

- `Atom`, `LocalPid`, `LocalPort` — **no brand**. Tagged immediates (an atom; an internal pid/port validated via `enif_get_local_pid`/`_port`), valid in any env. They implement `FreeTerm: for<'id> Term<'id>`, so they satisfy an `impl Term<'id>` slot for *every* brand.
- `Integer<'id>`, `Float<'id>`, `Binary<'id>`, `Bitstring<'id>`, `Fun<'id>`, `List<'id>`, `Map<'id>`, `Reference<'id>`, `Tuple<'id>`, `Pid<'id>`, `Port<'id>` — carry the brand `'id` because values may live on the BEAM heap and must not escape their env. `Pid<'id>`/`Port<'id>` are pids/ports of *unestablished* locality: an external (remote-node) one is heap-boxed. Refine to a storable `LocalPid`/`LocalPort` with `to_local(env)`.

### `impl Term<'id>` — universal term input

Functions that accept a term as input take `impl Term<'id>` (the `Term` trait). This is implemented by every otter term type. The brand binds the term to a specific env: an `impl Term<'id>` argument only accepts terms of brand `'id`. Env-portable types (`Atom`, `LocalPid`, `LocalPort`) implement `FreeTerm` and so satisfy any call site; env-bound types only carry their own brand, so cross-env terms are rejected at compile time. BEAM treats cross-env terms as undefined behavior; this constraint is load-bearing for soundness.

This means you can pass concrete types directly — no `.encode(env)` needed:

```rust
map.put(env, atom_key, integer_val)
List::from_terms(env, [int1, int2, int3])
env.raise(some_atom)
```

`Term` (and `Env`) are sealed — they cannot be implemented outside the crate. The `from_raw`/`wrap` constructors that build a branded term from a raw word are `#[crate::raw]`-gated, exposed under the `raw` feature.

### Per-type methods

Every accessor takes `env: impl Env<'id>` of the term's brand (the term carries only the brand). Constructors are associated functions taking an env.

```rust
// Atom (no brand)
fn intern(env, name: &str) -> Result<Atom, AtomError>   // NameTooLong if >255 chars
fn try_existing(env, name: &str) -> Option<Atom>        // look up without creating
fn is_atom(env, term) -> bool
fn name(self, env) -> String
// AtomError — single variant NameTooLong (a plain Rust error, not a pending raise).
// StaticAtom — pre-declared atom, OnceLock<Atom> storage:
const fn new(name: &'static str) -> StaticAtom
fn init(&self, env) -> Result<(), AtomError>   // intern at load; from init!'s atoms=[] it's infallible
fn get(&self) -> Atom                          // acquire load of a OnceLock<Atom>

// Integer
fn from_i64(env, i64) / from_u64(env, u64) -> Integer<'id>
fn to_i64(self, env) -> Option<i64>            // None on overflow
fn to_u64(self, env) -> Option<u64>            // None if negative / overflow
fn to_bigint(self, env) -> BigInt              // `bigint` feature; total, via ETF
fn from_bigint(env, &BigInt) -> Integer<'id>   // `bigint` feature; i64/u64 fast-path else ETF

// Float
fn from_f64(env, f64) -> Option<Float<'id>>    // None if not finite (no raise)
fn to_f64(self, env) -> f64

// Binary / Bitstring
fn as_bytes(self, env) -> &'id [u8]            // zero-copy into BEAM heap
fn len(self, env) -> usize / is_empty(self, env) -> bool
fn try_str(self, env) -> Result<&'id str, Utf8Error>
fn sub(self, env, pos, len) -> Binary<'id>     // zero-copy slice (enif_make_sub_binary)
fn from_bytes(env, &[u8]) -> Binary<'id>       // enif_make_new_binary
fn is_binary(env, term) -> bool
fn deserialize(self, env, safe) -> Option<AnyTerm<'id>>   // deserialize this binary's ETF
// Bitstring: is_binary(self, env) -> bool, to_binary(self, env) -> Option<Binary<'id>>

// BinaryBuf — owned, growable buffer (Vec<u8> model; no env, no brand)
fn new() / with_capacity(cap) -> BinaryBuf
fn push / extend_from_slice / resize / reserve
fn as_bytes(&self) -> &[u8] / as_bytes_mut(&mut self) -> &mut [u8]
fn len / is_empty / capacity
fn into_binary(self, env) -> Binary<'id>       // consume, hand allocation to the BEAM
impl Deref/DerefMut/AsRef/AsMut<[u8]>, Extend<u8>, Write, Default, Debug, Drop

// List (cons cell)
fn node(self, env) -> Node<'id>                // decompose: Nil or Cell(head, tail)
fn iter(self, env) -> ListIterator<'id>        // yields AnyTerm heads; .tail() for terminal
fn len(self, env) -> Option<usize>             // O(n), None for improper lists
fn is_empty(self, env) -> bool
fn reverse(self, env) -> Option<List<'id>>
fn try_string(self, env) -> Option<String>
fn from_terms(env, IntoIterator<Item: Term<'id>>) -> List<'id>
fn from_str(env, &str) -> List<'id>            // UTF-8 string → list of codepoints
fn cons(env, head, tail) -> List<'id>
fn is_list(env, term) -> bool

// Tuple — lean handle; with_elements does the single enif_get_tuple
fn with_elements(self, env) -> TupleView<'id>
fn from_terms(env, IntoIterator<Item: Term<'id>>) -> Tuple<'id>
fn is_tuple(env, term) -> bool
// TupleView<'id>: len()/is_empty() (no env), Index<usize> -> AnyTerm<'id> (zero-copy),
//   IntoIterator (Item = AnyTerm<'id>); a Term itself, but deliberately no Eq/Ord.

// Map (immutable; mutations return a new map)
fn new(env) -> Map<'id>
fn size(self, env) -> usize
fn get(self, env, key) -> Option<AnyTerm<'id>>
fn put(self, env, key, value) -> Map<'id>
fn update(self, env, key, value) -> Option<Map<'id>>   // None if key absent
fn remove(self, env, key) -> Map<'id>                  // NOT Option: absent key → map unchanged
fn iter(self, env) -> MapIterator<'id>
fn is_map(env, term) -> bool

// Pid / LocalPid
fn Pid::to_local(self, env) -> Option<LocalPid> / Pid::is_pid(env, term) -> bool
fn LocalPid::self_(env: CallEnv) -> LocalPid           // CallEnv only
fn LocalPid::whereis(env, name: impl Term) -> Option<LocalPid>
fn LocalPid::is_alive(self, env) -> bool
// sends are FREE verbs (see ops.rs), not pid methods:
//   send_from(env: impl CallingEnv, &LocalPid, msg)   (copy, caller-attributed, in-NIF)
//   send(&LocalPid, &mut OwnedEnvArena, OwnedEnvTerm)  (heap steal, off-thread)

// Port / LocalPort
fn Port::to_local(self, env) -> Option<LocalPort> / Port::is_port(env, term) -> bool
fn LocalPort::whereis(env, name: impl Term) -> Option<LocalPort>
fn LocalPort::is_alive(self, env) -> bool
//   port_command(env: impl CallingEnv, &LocalPort, msg)  (free verb)

// Reference / Fun
fn Reference::new(env) -> Reference<'id> / Reference::is_ref(env, term) -> bool
fn Fun::is_fun(env, term) -> bool

// serialization — type-agnostic free verbs (types::)
fn serialize(env, term) -> Option<BinaryBuf>           // term → ETF
fn deserialize(env, &[u8], safe: bool) -> Option<AnyTerm<'id>>
```

### Env verbs

The default `Env`-trait verbs (valid on any env kind) are `term_type`, `hash`, `consume_timeslice`, `is_current_process_alive`, `make_unique_integer` (Layer 3). Context verbs sit on the env kind that has the context:

```rust
impl<'id> CallEnv<'id> {
    fn raise<T>(self, reason: impl Term<'id>) -> Result<T, Raised<'id>>
    fn badarg<T>(self) -> Result<T, Raised<'id>>
    fn check_raised(self, term: AnyTerm<'id>) -> Result<AnyTerm<'id>, Raised<'id>>
    fn cpu_time(self) -> Result<Tuple<'id>, Raised<'id>>     // Err(Raised) if OS can't
    unsafe fn schedule_nif(self, name, flags, fp, argc, argv) -> Result<AnyTerm<'id>, Raised<'id>>
}
impl<'id> InitEnv<'id> {
    fn set_option_delay_halt(self) -> bool
    unsafe fn set_option_on_halt(self, callback) -> bool
    unsafe fn set_option_on_unload_thread(self, callback) -> bool
}
```

### Exceptions: the `Raised` witness

`enif_make_badarg` / `enif_raise_exception` raise *on the spot*: they set a pending exception on the env that the BEAM raises when the NIF returns, and until then any further env operation is UB.

`Raised<'id>` is a term-less typestate token proving this has happened. It has a private field and is only produced by an operation that actually raised — `CallEnv::raise`, `CallEnv::badarg`, or `CallEnv::check_raised` after a raising `raw` call — so holding one proves the env is already pending. A NIF returns `Result<T, Raised<'id>>`; the `Encoder` for that returns the non-value marker directly on `Err` — it never *re-*raises — so exit is sound by construction and double-raising is impossible. `raise`/`badarg` are generic over the success type, so `return env.badarg()` fits `return`, `let`-`else`, and `.or_else(|_| env.badarg())?` alike. (Builders that *don't* raise — e.g. `Float::from_f64` on a non-finite value — return `None` instead; the caller decides whether to turn that into a raise.)

---

## Layer 5: Codec (`codec/`)

```rust
pub enum CodecError {
    WrongType, IntegerOverflow, NotFinite, FloatRange, NotUtf8, WrongArity, UnknownTermType,
}

pub trait Encoder<'id> {
    fn encode(&self, env: impl Env<'id>) -> Result<AnyTerm<'id>, CodecError>;
}

pub trait Decoder<'id>: Sized {
    fn decode(term: AnyTerm<'id>, env: impl Env<'id>) -> Result<Self, CodecError>;
}
```

Both directions are fallible and symmetric: a failed `Decoder` on a NIF argument
becomes `badarg`, a failed `Encoder` on a NIF return becomes `badret`. otter term
types implement both and never fail (encoding wraps the word). The `codec/`
submodules add conversions for native Rust types — integers, floats, `bool`,
`str`/`String`, tuples (arity 1–12), `Vec<T>`, `HashMap<K, V>`, and (under the
`bigint` feature) `num_bigint::BigInt` — and these are the only impls that can
actually fail (a non-finite float on encode; an out-of-range integer, bad UTF-8,
or wrong-arity tuple on decode).

Note: a blanket `TryFrom<TypedTerm<'a>> for T: Decoder<'a>` cannot be provided — it violates Rust's orphan rules (E0210). Use `T::decode(term, env)` directly.

---

## Layer 6: Resources (`resource/`)

### `Resource` trait

```rust
pub trait Resource: Sized + Send + Sync + 'static {
    fn destructor(self, _env: CallbackEnv<'_>) {}
    fn down<'a>(&'a self, _env: CallbackEnv<'a>, _pid: LocalPid, _monitor: Monitor) {}
    fn stop(&self, _env: CallbackEnv<'_>, _event: select::Event, _is_direct_call: bool) {}
}
```

No required methods: a bare `impl Resource for T {}` suffices. Callbacks run with a `CallbackEnv` (a scheduler thread, no process context). The type pointer is not stored on the trait — it lives in the per-instance registry inside `priv_data` (see Registration).

### `ResourceArc<T>`

Two-pointer layout: `raw` (allocation start for keep/release) and `inner` (aligned write position for Deref/destructor). Implements `Encoder`, `Decoder`, `Deref<Target=T>`, `Clone`, `Drop`. Instances are created with the free function `make_resource(env, val)` (or `resource_handle::<T>(env).make(val)` for a `Send` handle usable off-thread), both of which look the type pointer up in the registry. It also carries `monitor(env: Option<E>, &pid) -> Option<Monitor>` / `demonitor`.

### `Monitor`

Wraps `enif_ffi::Monitor`. `Copy`; implements `PartialEq`/`Eq` via `enif_compare_monitors`. Has `to_term(env)` via `enif_make_monitor_term`.

### Registration

Resource types are listed in `init!`'s `resources = [...]`; the generated load/upgrade scaffolding registers each one (`CREATE` in load, `CREATE | TAKEOVER` in upgrade) and records the returned `*mut enif_ffi::ResourceType` in the per-instance registry inside `priv_data`, keyed by `TypeId`. The free functions `register::<T>(env: InitEnv, flags)` / `register_tagged::<T>(env: InitEnv, flags, tag)` are the manual primitives — they take an `InitEnv`, so they are callable only from `load`/`upgrade` (enforced by type), and panic on double registration.

The BEAM-side identifier is `std::any::type_name::<T>()` (the fully-qualified Rust type path, unique within the per-library type table) plus an ABI suffix: `#abi=<hash>` by default — a content hash of this build's binary, so a different build does not take the type over — or `#tag=<tag>` for `register_tagged`, a stable name that opts into cross-build takeover. See `abi.rs` and `docs/UPGRADE.md`.

Resource payloads inherited across a hot upgrade fall under the **core safety invariant** above: a second build taking over a resource type must not assume it can interpret or drop a `T` allocated by the previous build. See `docs/UPGRADE.md`.

### `dynamic_resource_call`

Module-level function wrapping `enif_dynamic_resource_call`.

---

## Layer 7: Time (`time.rs`)

```rust
pub fn monotonic_time(unit: TimeUnit) -> Time;
pub fn time_offset(unit: TimeUnit) -> Time;
pub fn convert_time_unit(val: Time, from: TimeUnit, to: TimeUnit) -> Time;
```

---

## Layer 8: System (`system.rs`)

```rust
pub enum ThreadType { Scheduler, DirtyCpu, DirtyIo, NonScheduler, Unknown(c_int) }
pub fn thread_type() -> ThreadType;
pub fn system_info(info: &mut SysInfo);
```

---

## Layer 9: I/O Select (`select.rs`)

```rust
pub fn select<'id, T: Resource>(
    env: impl Env<'id>, event: select::Event, flags: select::SelectFlags,
    obj: &ResourceArc<T>, pid: &LocalPid, ref_term: impl Term<'id>) -> i32;
pub fn select_x<'id, 'm, T: Resource, M: Env<'m>>(
    env: impl Env<'id>, event: select::Event, flags: select::SelectFlags,
    obj: &ResourceArc<T>, pid: &LocalPid, msg: impl Term<'m>, msg_env: Option<M>) -> i32;
```

Requires a `ResourceArc<T>` — the BEAM ties I/O event lifecycle to resource objects. Both return a raw `i32` bitmask decoded against `select::SELECT_*` (a typed result wrapper is tracked as enhance-12). `Event`, `SelectFlags`, and the `SELECT_*` constants are re-exported from `enif_ffi` through `otter::select`, so the API is fully usable without the `raw` feature.

---

## Cargo features

otter always binds NIF 2.17 (OTP 26) through `enif-ffi`'s `nif_2_17`. Three opt-in features:

| Feature | Effect |
|---|---|
| `nif_2_18` | Enable the NIF 2.18 (OTP 29) additions (forwards to `enif-ffi/nif_2_18`). |
| `bigint` | Pull in `num-bigint` (re-exported as `otter::num_bigint`) and add `Integer::{to_bigint, from_bigint}` plus `Encoder`/`Decoder for BigInt` — arbitrary-precision integers beyond `i64`/`u64`. Off by default. |
| `raw` | Expose the raw, 1:1, all-`unsafe` `enif_ffi` crate as `otter::enif_ffi`, widen the `#[crate::raw]`-gated bridge constructors to `pub`, and accept the tier-2 `_raw` lifecycle callbacks in `init!`. The escape hatch — see `docs/UPGRADE.md`. |

---

## What is deliberately excluded

- **Serde integration** — implement `Encoder`/`Decoder` directly.
- **Elixir types** — no `NifStruct`, no `NifException`, no `__struct__` key handling.
- **Automatic NIF registration** — registration is explicit via `init!`.
- **`NifUntaggedEnum`** — structural dispatch belongs in user code.
- **Convenience wrappers** — no built-in `IoData`, no pre-assembled type hierarchies.
- **Thread spawning** — not a core NIF concept. Use `OwnedEnvArena` + `send` for messaging from OS threads spawned via standard Rust threading.
- **Raw memory allocation** (`enif_alloc`/`enif_free`) — use Rust's allocator for ordinary per-call work. Opting *all* allocations onto the BEAM allocator is available via `otter::enif_global_allocator!()` (the `EnifAlloc` `#[global_allocator]`, `src/alloc.rs`), so cross-build state is freeable through the one shared path; a per-state *scoped* allocator and the ABI fingerprint that complete the safe sandbox remain planned — see the core safety invariant and `docs/UPGRADE.md`.
- **NIF threading primitives** (`enif_mutex_*`, `enif_cond_*`, etc.) — use `std::sync`.
