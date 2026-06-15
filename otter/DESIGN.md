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

```
otter/src/
├── sys.rs      Raw C ABI types mirroring erl_nif.h
├── enif.rs     Complete 1:1 enif_* shims + dlsym loading. The sole funcs()/unsafe
│               consumer; pub under the `raw` feature, else pub(crate).
├── env.rs      Env<'a>, EnvKind, OwnedEnv
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

## Layer 1: Raw C ABI (`sys/`)

Direct Rust transcription of `erl_nif.h`. No logic, no safety wrappers — only type definitions and constants.

Key types:

| Rust type | C type | Purpose |
|---|---|---|
| `NifTerm` | `ERL_NIF_TERM` | Opaque term handle — a tagged machine word |
| `NifEnv` | `ErlNifEnv` | Per-call or process-independent environment |
| `NifFunc` | `ErlNifFunc` | Describes one NIF: name, arity, function pointer, flags |
| `NifEntry` | `ErlNifEntry` | Library descriptor returned by `nif_init()` |
| `NifBinary` | `ErlNifBinary` | Inspected binary: size + data pointer |
| `NifResourceType` | `ErlNifResourceType` | Opaque resource type handle |
| `NifPid` | `ErlNifPid` | Local process identifier |
| `NifPort` | `ErlNifPort` | Port identifier |
| `NifMonitor` | `ErlNifMonitor` | Process monitor handle (32 bytes, opaque) |
| `NifMapIterator` | `ErlNifMapIterator` | Map iteration state |
| `NifTermType` | `ErlNifTermType` | Enum of the 11 term types |
| `NifTime` | `ErlNifTime` | Time value (i64) |
| `NifTimeUnit` | `ErlNifTimeUnit` | Second/Millisecond/Microsecond/Nanosecond |
| `NifHash` | `ErlNifHash` | InternalHash or Phash2 |
| `NifSysInfo` | `ErlNifSysInfo` | BEAM system information struct |
| `NifOption` | `ErlNifOption` | Option key for `enif_set_option` |
| `NifEvent` | `ErlNifEvent` | OS event handle (fd on Unix) |

Also defines flag newtypes with scoped constants: `NifResourceFlags::CREATE`, `NifUniqueInteger::POSITIVE`,
`NifSelectFlags::READ`, etc. All flag types implement `BitOr` for combination. Standalone constants:
`NIF_BIN2TERM_SAFE`, `NIF_DIRTY_JOB_*`, `NIF_SELECT_*`, `NIF_THR_*`, `NIF_TIME_ERROR`.

---

## Layer 1.5: NIF Function Shims (`enif.rs`)

Complete `enif_*` API surface in a single `pub(crate)` module. Three responsibilities:

1. **Function pointer table** — an `EnifFunctions` struct holding ~100+ `unsafe extern "C" fn` pointers, organized by NIF version (0.1 through 2.17, optional 2.18).

2. **Dynamic symbol loading** — `enif::init()` resolves all function pointers via `libc::dlsym(RTLD_DEFAULT, ...)` at NIF load time. Guarded by `OnceLock` against double-initialization. Returns `Err(symbol_name)` on first failure.

3. **Shim functions** — one `unsafe fn` per `enif_*`, calling through the pointer table with the `enif_` prefix dropped (e.g. `enif::is_atom()`, `enif::make_atom()`). Each doc comment notes the NIF version and OTP release where the C function was introduced.

`enif` is the **sole** consumer of `funcs()` and the only place FFI `unsafe` lives; everything above it audits as safe. The module is `pub` under the `raw` feature (the complete escape hatch) and `pub(crate)` otherwise — it is always compiled, the feature only controls visibility. Symbol loading is exposed as a single public `otter::init()` at the crate root, which delegates to `enif::init()`.

Minimum required version: NIF 2.17 (OTP 26). C macros that delegate to real enif functions (e.g. `enif_make_tuple3`, `enif_select_read`) are exposed as plain Rust functions. Variadic functions (`make_tuple`, `make_list`, `set_option`) are bound as variadic `fn` pointers and called directly; only the `printf` family stays type-erased (`*mut c_void`), since its `va_list` variants are unrepresentable on stable Rust.

---

## Layer 2: The safe layer (env-as-receiver)

Above `enif` is the entire Erlang-facing surface, and it audits as safe — `enif` is the only place `funcs()`/`unsafe` FFI is reached.

The organising principle is **env-as-receiver**: an operation takes its environment explicitly. When the env *is* the subject it is the receiver — `env.make_tuple(&[…])`, `env.is_binary(term)`, `env.get_map_value(map, key)` — under the audit rule *every `enif_foo(env, …)` becomes `env.foo(…)`*. Env-less operations on a clear subject are value-type methods instead (`Term`'s `Ord`/`Eq` via `enif_compare`/`enif_is_identical`, the `BinaryBuf` buffer ops). Term inputs are taken as `impl AsNifTerm<'a>` (see Layer 4), so a term from another env is rejected at compile time.

These methods are not gathered in one module — each lives next to its subject. The predicate and builder Env methods for a type sit on that type's file in `types/` (`env.make_binary` in `types/binary.rs`, `env.make_tuple` in `types/tuple.rs`); the general ones (`raise_exception`, `make_copy`, `term_type`, `schedule_nif`, `cpu_time`, …) sit on `term.rs`. The per-type constructors (`Atom::intern`, `Binary::from_bytes`, `Map::new`, …) remain and delegate to the matching Env method.

The optional sync/thread/IO-queue tier and the deliberately-unsafe set (`enif_alloc`/`dlsym`/`fprintf`/…) have **no** safe wrapper — they are reachable only through the `raw`-feature `enif` surface.

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

### `OwnedEnv`

A process-independent environment for building and sending terms from outside a NIF call (e.g. from a spawned OS thread). Simple struct with one field:

```rust
pub struct OwnedEnv {
    env: *mut NifEnv,
}

impl OwnedEnv {
    pub fn new() -> OwnedEnv;
    pub fn send<F>(&mut self, pid: &LocalPid, f: F) -> bool
    where F: FnOnce(Env<'_>) -> TypedTerm<'_>;
    pub fn port_command<F>(&mut self, port: &LocalPort, f: F) -> bool   // same closure shape
    where F: FnOnce(Env<'_>) -> TypedTerm<'_>;
    pub fn clear(&mut self);
}
```

`send` is closure-based: the closure builds a term in a temporary env, sends it to `pid`, and clears automatically. Terms cannot escape the closure — the lifetime is tied to the closure's scope. `OwnedEnv` implements `Drop` (calls `enif_free_env`), `Default`, and is `Send`.

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
// (in-NIF send: env.send(to, msg) -> bool)

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

## Layer 5: Codec (`codec.rs`)

```rust
pub enum CodecError { WrongType, IntegerOverflow }

pub trait Encoder {
    fn encode<'a>(&self, env: Env<'a>) -> Term<'a>;
}

pub trait Decoder<'a>: Sized {
    fn decode(term: Term<'a>) -> Result<Self, CodecError>;
}
```

Implemented for all otter term types. Not implemented for native Rust types.

Note: a blanket `TryFrom<TypedTerm<'a>> for T: Decoder<'a>` cannot be provided — it violates Rust's orphan rules (E0210). Use `T::decode(term)` directly.

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
- **Thread spawning** — not a core NIF concept. Use `OwnedEnv::send` for messaging from OS threads spawned via standard Rust threading.
- **Raw memory allocation** (`enif_alloc`/`enif_free`) — use Rust's allocator for ordinary per-call work. Opting *all* allocations onto the BEAM allocator is available via `otter::enif_global_allocator!()` (the `EnifAlloc` `#[global_allocator]`, `src/alloc.rs`), so cross-build state is freeable through the one shared path; a per-state *scoped* allocator and the ABI fingerprint that complete the safe sandbox remain planned — see the core safety invariant and `docs/UPGRADE.md`.
- **NIF threading primitives** (`enif_mutex_*`, `enif_cond_*`, etc.) — use `std::sync`.
