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
├── env.rs      Env<'a>, EnvKind, OwnedTermBuilder, OwnedTerm
├── term.rs     Term, TypedTerm, Raised, and the general-purpose Env methods
├── codec.rs    Encoder + Decoder traits, CodecError
├── types/      One file per concrete term type — its methods plus the Env methods
│               that build/inspect that type (env.make_tuple, env.is_binary, …)
├── resource.rs Resource trait, ResourceArc<T>, Monitor, dynamic_resource_call
├── time.rs     BEAM monotonic time, time offset, unit conversion
├── system.rs   Thread type introspection, system info
└── select.rs   I/O event multiplexing (enif_select)
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
`otter_codegen/DESIGN.md`). The crate is re-exported as `otter::enif_ffi`
(`#[doc(hidden)]`) so the codegen output in the user's crate can name the raw
types in the `extern "C"` signatures it emits; turning that into a deliberate
`raw`-feature escape hatch is planned (issue enhance-11). The one piece otter
keeps in-tree is `alloc.rs`, which **direct-links** `enif_alloc`/`enif_free`
rather than going through the resolved table — the global allocator may run before
`nif_init`, so it cannot depend on the resolution step.

---

## Layer 2: The safe layer (env-as-receiver)

Above the enif-ffi floor is the entire Erlang-facing surface, and it audits as safe — every `unsafe` FFI call goes through an `enif_ffi::*` shim.

The organising principle is **env-as-receiver**: an operation takes its environment explicitly. When the env *is* the subject it is the receiver — `env.make_tuple(&[…])`, `env.is_binary(term)`, `env.get_map_value(map, key)` — under the audit rule *every `enif_foo(env, …)` becomes `env.foo(…)`*. Env-less operations on a clear subject are value-type methods instead (`Term`'s `Ord`/`Eq` via `enif_compare`/`enif_is_identical`, the `BinaryBuf` buffer ops). Term inputs are taken as `impl AsNifTerm<'a>` (see Layer 4), so a term from another env is rejected at compile time.

These methods are not gathered in one module — each lives next to its subject. The predicate and builder Env methods for a type sit on that type's file in `types/` (`env.make_binary` in `types/binary.rs`, `env.make_tuple` in `types/tuple.rs`); the general ones (`raise_exception`, `make_copy`, `term_type`, `schedule_nif`, `cpu_time`, …) sit on `term.rs`. The per-type constructors (`Atom::intern`, `Binary::from_bytes`, `Map::new`, …) remain and delegate to the matching Env method.

The optional sync/thread/IO-queue tier and the deliberately-unsafe set (`enif_alloc`/`dlsym`/`fprintf`/…) have **no** safe wrapper — they are reachable only through the raw `enif-ffi` crate (re-exported as `otter::enif_ffi`).

---

## Layer 3: Environment (`env.rs`)

### `Env<'a>`

The central lifetime safety mechanism. Each NIF call gets an `Env<'a>` with a unique per-call lifetime synthesized from a stack borrow. `PhantomData<*mut &'a u8>` makes `Env` invariant over `'a`, preventing any `TypedTerm<'a>` from being stored past the call's lifetime. There is no runtime check — this is enforced entirely by the type system.

```rust
pub struct Env<'a> {
    pub kind: EnvKind,
    env: *mut NifEnv,
    _id: PhantomData<*mut &'a u8>,
}

pub enum EnvKind {
    ProcessBound,       // standard NIF call env (constructed by codegen)
    Callback,           // resource destructor/monitor callback env
    Load,               // load callback env — valid for resource registration
    Upgrade,            // upgrade callback env — valid for resource registration
    Unload,             // unload callback env
    ProcessIndependent, // allocated with enif_alloc_env
}
```

`EnvKind` and `Env.kind` are `pub` because generated code constructs `ProcessBound`, `Load`, `Upgrade`, and `Unload` envs. `register` asserts `env.kind` is `Load` or `Upgrade` at runtime.

### `OwnedTermBuilder` / `OwnedTerm`

Building and sending a message from outside a NIF call (e.g. from a spawned OS thread), via the `enif_send` heap steal — O(1), single-use:

```rust
pub struct OwnedTermBuilder { /* owns a process-independent env */ }
pub struct OwnedTerm        { /* owns its env + the chosen message word */ }

impl OwnedTermBuilder {
    pub fn new() -> OwnedTermBuilder;
    pub fn env(&self) -> Env<'_>;       // build terms on it; they borrow the builder
    pub fn set(&self, t: Term<'_>);     // choose the message (must be built in this env)
    pub fn build(self) -> OwnedTerm;    // consume the builder, taking ownership of its env
}

// the message is delivered via a method on the recipient pid:
//   pid.send_owned(owned)            // off-thread (NULL caller env)
//   pid.send_owned_from(env, owned)  // in-NIF (caller env attributes the sender)
```

Terms are built directly on the builder (`value.encode(b.env())`) and held as ordinary `Term`s — they borrow the builder, so they cannot outlive it. `set` records which term is the message (provenance-checked against the builder's env); `build` consumes the builder into an `OwnedTerm` whose heap is transplanted into the message when sent with [`LocalPid::send_owned`]. Both types implement `Drop` (`enif_free_env`) and are `Send`; the builder also implements `Default`.

A successful send *steals* the env's heap (`enif_send` with a non-NULL `msg_env`), so the env is single-use: there is no reuse-and-clear, and no off-thread `port_command` (`enif_port_command` aborts the VM on a NULL caller env).

---

## Layer 4: Terms (`term.rs` and `types/`)

### Three levels of resolution

**Level 1 — `Term<'a>`:** The bare machine word plus its `Env`. Zero work done. The fastest possible representation. A received type — you cannot construct one from scratch.

**Level 2 — `TypedTerm<'a>` enum:** One `enif_term_type` call has been made. The correct variant is known. Data is still on the BEAM heap.

```rust
pub enum TypedTerm<'a> {
    Atom(Atom), Bitstring(Bitstring<'a>), Float(Float<'a>),
    Fun(Fun<'a>), Integer(Integer<'a>), List(List<'a>),
    Map(Map<'a>), Pid(Pid<'a>), Port(Port<'a>),
    Reference(Reference<'a>), Tuple(Tuple<'a>),
}
```

11 variants for 11 type tags — `Bitstring` covers both byte-aligned binaries and sub-byte bitstrings; refine to a `Binary` with `Bitstring::to_binary` (or `is_binary`).

`TypedTerm` and `Term` implement `PartialEq`/`Eq` (via `enif_is_identical`) and `PartialOrd`/`Ord` (via `enif_compare`).

All concrete types implement `From<T> for TypedTerm<'a>`, so `let t: TypedTerm = atom.into()` works. `Term` converts via `TryFrom` (calls `resolve()`), failing with `CodecError::UnknownTermType` for a term type this otter build does not recognize.

**Level 3 — concrete types:** Type is known. Data is still on the BEAM heap. Accessor methods pull data out on demand.

### Lazy by default

Construction is always free. Extraction is on demand. Every concrete type is `NifTerm` + `Env<'a>`. No data is read from the BEAM heap until explicitly requested.

### Lifetime rules

- `Atom`, `LocalPid`, `LocalPort` — no lifetime. Tagged immediates (an atom; an internal pid/port validated via `enif_get_local_pid`/`_port`), valid anywhere.
- `Integer<'a>`, `Float<'a>`, `Binary<'a>`, `Bitstring<'a>`, `Fun<'a>`, `List<'a>`, `Map<'a>`, `Reference<'a>`, `Tuple<'a>`, `Pid<'a>`, `Port<'a>` — carry `'a` because values may live on the BEAM heap. `Pid<'a>`/`Port<'a>` are pids/ports of *unestablished* locality: an external (remote-node) one is heap-boxed, so they must not outlive `'a`. Refine to a storable `LocalPid`/`LocalPort` with `to_local()`.
- `Bitstring` and `Fun` carry `env` for lifetime only — no NIF inspection functions exist for them. These fields have `#[allow(dead_code)]`.

### `AsNifTerm<'a>` — universal term input

Functions that accept a term as input use `impl AsNifTerm<'a>` instead of `TypedTerm<'a>`. This sealed trait is implemented for all otter term types (`Atom`, `Binary`, `Integer`, `List`, `TypedTerm`, `Term`, etc.) and for `&T` where `T: AsNifTerm<'a>`. It extracts the underlying `NifTerm` without allocating or copying.

The lifetime parameter binds the term to a specific env: an `impl AsNifTerm<'a>` argument only accepts terms whose env is `'a`. Env-portable types (`Atom`, `LocalPid`, `LocalPort`) implement `AsNifTerm<'a>` for every `'a` and so satisfy any call site. Env-bound types (`Term<'a>`, `Binary<'a>`, `Pid<'a>`, `Port<'a>`, etc.) only implement it for their own lifetime, so cross-env terms are rejected at compile time. BEAM treats cross-env terms as undefined behavior; this constraint is load-bearing for soundness.

This means you can pass concrete types directly — no `.encode(env)` needed:

```rust
map.put(atom_key, integer_val)
List::from_terms(env, [int1, int2, int3])
env.raise_exception(some_atom)
```

`AsNifTerm` is sealed — it cannot be implemented outside the crate.

### Per-type methods

```rust
// Atom
fn intern(env, name: &str) -> Option<Atom>    // create/intern
fn try_existing(env, name: &str) -> Option<Atom>  // look up without creating
fn name(self, env) -> String

// StaticAtom — pre-declared atom with eager initialization
const fn new(name: &'static str) -> StaticAtom
fn init(&self, env: Env)           // intern in atom table (call from on_load)
fn get(&self) -> Atom              // single atomic load

// Integer
impl TryFrom<Integer> for i64     // extract signed 64-bit
impl TryFrom<Integer> for u64     // extract unsigned 64-bit
impl TryFrom<Integer> for i128    // combined i64/u64 range
fn from_i64(env, val) -> Integer<'a>
fn from_u64(env, val) -> Integer<'a>

// Float
impl From<Float> for f64           // infallible extraction
fn from_f64(env, val) -> Result<Float<'a>, Raised<'a>>  // Err(Raised) if not finite

// Binary
fn as_bytes(self) -> &'a [u8]     // zero-copy into BEAM heap
fn len(self) -> usize
fn try_str(self) -> Result<&'a str, Utf8Error>
fn sub(self, pos, len) -> Binary<'a>   // zero-copy slice
fn from_bytes(env, data) -> Binary<'a>
fn deserialize(&self, env, safe) -> Option<Term<'a>>  // deserialize ETF bytes
impl Deref<Target=[u8]>            // auto-coerce to &[u8]
impl AsRef<[u8]>                   // trait-based byte access
impl Debug                         // Binary(N bytes)

// BinaryBuf — growable buffer (Vec<u8> model)
fn new() -> BinaryBuf
fn with_capacity(cap) -> BinaryBuf
fn push(&mut self, byte)
fn extend_from_slice(&mut self, &[u8])
fn resize(&mut self, new_len, value)
fn as_slice(&self) -> &[u8]
fn as_mut_slice(&mut self) -> &mut [u8]
fn finish(self, env) -> Binary<'a>
impl Deref<Target=[u8]> / DerefMut // auto-coerce to &[u8] / &mut [u8]
impl AsRef<[u8]> / AsMut<[u8]>    // trait-based byte access
impl Extend<u8>                    // iterator-based appending
impl Write                         // write! and write_all support
impl Debug                         // BinaryBuf { len, capacity }

// List (cons cell)
fn node(self) -> Node<'a>           // decompose: Nil or Cell(head, tail)
fn iter(self) -> ListIterator<'a>   // yields Term heads; .tail() for terminal
fn len(self) -> Option<usize>       // O(n), None for improper lists
fn reverse(self) -> Option<List<'a>>
fn try_string(self) -> Result<String, CodecError>
fn from_terms(env, terms) -> List<'a>
fn from_str(env, &str) -> List<'a>  // UTF-8 string → list of codepoints
fn cons(env, head, tail) -> List<'a>

// Tuple
fn len(self) -> usize
fn element(self, i) -> TypedTerm<'a>
fn from_terms(env, terms) -> Tuple<'a>

// Map
fn new(env) -> Map<'a>
fn size(self) -> usize
fn get(self, key) -> Option<Term<'a>>
fn put(self, key, value) -> Map<'a>
fn update(self, key, value) -> Option<Map<'a>>
fn remove(self, key) -> Option<Map<'a>>
fn iter(self) -> MapIterator<'a>

// Pid
fn self_(env) -> Pid
fn is_alive(self, env) -> bool
fn whereis(env, name: Atom) -> Option<Pid>
// sends are methods on the recipient LocalPid:
//   to.send(msg) / to.send_from(env, msg)            (copy)
//   to.send_owned(owned) / to.send_owned_from(env, owned)  (steal)

// Port
fn whereis(env, name: Atom) -> Option<Port>
fn command(self, caller_env, msg_env, msg) -> bool

// Reference
fn new(env) -> Reference<'a>

// Term (serialization is type-agnostic — no resolve needed)
fn serialize(self) -> Option<BinaryBuf>   // serialize to ETF; .into_binary(env) or .as_bytes()
```

### Env methods

The env-as-receiver methods are spread across the type files (the per-type predicates and builders — `env.is_binary`, `env.make_tuple`, `env.get_map_value`, …) and `term.rs` (the general ones below):

```rust
impl<'a> Env<'a> {
    fn consume_timeslice(self, percent: i32) -> bool
    fn make_unique_integer(self, properties) -> TypedTerm<'a>
    fn hash(self, algorithm, term, salt) -> u64
    fn is_current_process_alive(self) -> bool
    fn cpu_time(self) -> Result<TypedTerm<'a>, Raised<'a>>   // Err(Raised) if OS can't
    fn raise_exception<T>(self, reason: impl AsNifTerm<'a>) -> Result<T, Raised<'a>>
    fn make_badarg<T>(self) -> Result<T, Raised<'a>>
    fn check_raised(self, term: NifTerm) -> Result<Term<'a>, Raised<'a>>
    unsafe fn schedule_nif(self, name, flags, fp, argc, argv) -> Result<TypedTerm<'a>, Raised<'a>>
    fn set_option_delay_halt(self) -> bool
    unsafe fn set_option_on_halt(self, callback) -> bool
    unsafe fn set_option_on_unload_thread(self, callback) -> bool
}
```

### Exceptions: the `Raised` witness

`enif_make_badarg` / `enif_raise_exception` — and builders like `enif_make_double` on bad input — raise *on the spot*: they set a pending exception on the env that the BEAM raises when the NIF returns, and until then any further env operation is UB.

`Raised<'a>` is an opaque witness that this has happened. It has a private field and is only produced by an operation that actually raised (`raise_exception`, `make_badarg`, or `check_raised` after a raising call), so holding one proves the env is already pending. A NIF returns `Result<T, Raised<'a>>`; the `Encoder` for that returns the marker word directly on `Err` — it never *re-*raises — so exit is sound by construction and double-raising is impossible. `raise_exception`/`make_badarg` are generic over the success type, so the idiom `return env.make_badarg()` fits `return`, `let`-`else`, and `.or_else(|_| env.make_badarg())?` positions alike.

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
`str`/`String`, tuples (arity 1–12), `Vec<T>`, and `HashMap<K, V>` — and these
are the only impls that can actually fail (a non-finite float on encode; an
out-of-range integer, bad UTF-8, or wrong-arity tuple on decode).

Note: a blanket `TryFrom<TypedTerm<'a>> for T: Decoder<'a>` cannot be provided — it violates Rust's orphan rules (E0210). Use `T::decode(term, env)` directly.

---

## Layer 6: Resources (`resource/`)

### `Resource` trait

```rust
pub trait Resource: Sized + Send + Sync + 'static {
    fn destructor(self, _env: Env<'_>) {}
    fn down<'a>(&'a self, _env: Env<'a>, _pid: LocalPid, _monitor: Monitor) {}
    fn stop(&self, _env: Env<'_>, _event: NifEvent, _is_direct_call: bool) {}
}
```

No required methods: a bare `impl Resource for T {}` suffices. The type pointer is not stored on the trait — it lives in the per-instance registry inside `priv_data` (see Registration).

### `ResourceArc<T>`

Two-pointer layout: `raw` (allocation start for keep/release) and `inner` (aligned write position for Deref/destructor). Implements `Encoder`, `Decoder`, `Deref<Target=T>`, `Clone`, `Drop`. Instances are created with `env.make_resource(val)` (or `env.resource_handle::<T>().make(val)` for a `Send` handle usable off-thread), both of which look the type pointer up in the registry.

### `Monitor`

Wraps `NifMonitor`. Implements `PartialEq`/`Eq` via `enif_compare_monitors`. Has `to_term(env)` via `enif_make_monitor_term`.

### Registration

Resource types are listed in `init!`'s `resources = [...]`; the generated load/upgrade scaffolding registers each one (`CREATE` in load, `CREATE | TAKEOVER` in upgrade) and records the returned `*mut NifResourceType` in the per-instance registry inside `priv_data`, keyed by `TypeId`. The free functions `register::<T>(env, flags)` / `register_tagged::<T>(env, flags, tag)` are the manual primitives (load/upgrade env only; panic on wrong context or double registration).

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
pub fn select<T: Resource>(env, event, flags, obj, pid, ref_term) -> i32;
pub fn select_x<T: Resource>(env, event, flags, obj, pid, msg, msg_env) -> i32;
```

Requires a `ResourceArc<T>` — the BEAM ties I/O event lifecycle to resource objects.

---

## What is deliberately excluded

- **Serde integration** — implement `Encoder`/`Decoder` directly.
- **Elixir types** — no `NifStruct`, no `NifException`, no `__struct__` key handling.
- **Automatic NIF registration** — registration is explicit via `init!`.
- **`NifUntaggedEnum`** — structural dispatch belongs in user code.
- **Convenience wrappers** — no built-in `IoData`, no pre-assembled type hierarchies.
- **Thread spawning** — not a core NIF concept. Use `OwnedTermBuilder` for messaging from OS threads spawned via standard Rust threading.
- **Raw memory allocation** (`enif_alloc`/`enif_free`) — use Rust's allocator for ordinary per-call work. Opting *all* allocations onto the BEAM allocator is available via `otter::enif_global_allocator!()` (the `EnifAlloc` `#[global_allocator]`, `src/alloc.rs`), so cross-build state is freeable through the one shared path; a per-state *scoped* allocator and the ABI fingerprint that complete the safe sandbox remain planned — see the core safety invariant and `docs/UPGRADE.md`.
- **NIF threading primitives** (`enif_mutex_*`, `enif_cond_*`, etc.) — use `std::sync`.
