# otter: Core Rust NIF Library

## Purpose

`otter` is a Rust library for writing Erlang NIFs (Native Implemented Functions) in safe Rust. It provides a direct, honest mapping of the Erlang NIF C ABI into Rust types, with no abstractions that don't have a clear Erlang equivalent.

**Design principle:** Writing a NIF with otter should feel like working directly with Erlang. If an Erlang programmer would not recognize a concept, it does not belong in this library.

---

## Layer Structure

```
otter/src/
├── sys/        Raw C ABI types mirroring erl_nif.h
├── enif.rs     Complete enif_* function pointer table, dlsym loading, ~200 pub(crate) shims
├── wrapper/    Rust-idiomatic wrappers over enif shims (pub for codegen)
├── env.rs      Env<'a>, EnvKind, OwnedEnv
├── term.rs     RawTerm, Term enum, Env methods (raise, hash, schedule, etc.)
├── codec.rs    Encoder + Decoder traits, CodecError
├── types/      One file per concrete term type
├── resource/   Resource trait, ResourceArc<T>, Monitor, dynamic_resource_call
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

3. **Wrapper functions** — ~200 `pub(crate)` functions that call through the function pointer table. The `enif_` prefix is dropped (e.g. `enif::is_atom()`, `enif::make_atom()`). Each function's doc comment notes the NIF version and OTP release where the C function was introduced.

Minimum required version: NIF 2.17 (OTP 26). C macros that delegate to real enif functions (e.g. `enif_make_tuple3`, `enif_select_read`) are exposed as plain Rust functions. Variadic functions like `set_option` store raw pointers and are transmuted per use.

---

## Layer 2: Wrappers (`wrapper/`)

Rust-idiomatic wrappers over the `enif` shims. Each submodule covers one category of `enif_*` functions, adding `Option` returns, buffer management, and type conversions. All submodules and their functions are `pub(crate)` — not part of the public otter API.

Both `enif` and `wrapper` are `pub(crate)` — entirely internal. Wrapper submodules access the function pointer table via `crate::enif::funcs()`. Symbol loading is exposed as a single public function `otter::init()` at the crate root, which delegates to `enif::init()`.

| Module | Covers |
|---|---|
| `atom.rs` | `enif_make_new_atom_len`, `enif_make_existing_atom_len`, `enif_get_atom`, `enif_get_atom_length` |
| `binary.rs` | `enif_alloc_binary`, `enif_release_binary`, `enif_make_binary`, `enif_inspect_binary`, `enif_make_new_binary`, `enif_make_sub_binary`, `enif_term_to_binary`, `enif_binary_to_term` |
| `check.rs` | `enif_is_binary` |
| `env.rs` | `enif_alloc_env`, `enif_free_env`, `enif_clear_env`, `enif_send` |
| `exception.rs` | `enif_raise_exception`, `enif_make_badarg` |
| `list.rs` | `enif_get_list_cell`, `enif_get_list_length`, `enif_make_list_from_array`, `enif_make_list_cell` |
| `map.rs` | `enif_make_new_map`, `enif_get_map_size`, `enif_get_map_value`, `enif_make_map_put/update/remove`, `enif_map_iterator_*` |
| `monitor.rs` | `enif_monitor_process`, `enif_demonitor_process`, `enif_compare_monitors`, `enif_make_monitor_term` |
| `number.rs` | `enif_get_long/ulong`, `enif_make_long/ulong`, `enif_get_double`, `enif_make_double` |
| `pid.rs` | `enif_self`, `enif_get_local_pid`, `enif_is_process_alive`, `enif_is_current_process_alive`, `enif_whereis_pid` |
| `port.rs` | `enif_port_command`, `enif_whereis_port` |
| `resource.rs` | `enif_init_resource_type`, `enif_alloc/release/keep/make/get_resource`, `enif_dynamic_resource_call` |
| `schedule.rs` | `enif_schedule_nif` |
| `select.rs` | `enif_select`, `enif_select_x` |
| `system.rs` | `enif_system_info`, `enif_thread_type`, `enif_set_option` (typed wrappers per option variant) |
| `term.rs` | `enif_term_type`, `enif_compare`, `enif_is_identical`, `enif_make_copy`, `enif_consume_timeslice`, `enif_make_ref`, `enif_make_unique_integer`, `enif_hash` |
| `time.rs` | `enif_monotonic_time`, `enif_time_offset`, `enif_convert_time_unit` |
| `tuple.rs` | `enif_make_tuple_from_array`, `enif_get_tuple` |

---

## Layer 3: Environment (`env.rs`)

### `Env<'a>`

The central lifetime safety mechanism. Each NIF call gets an `Env<'a>` with a unique per-call lifetime synthesized from a stack borrow. `PhantomData<*mut &'a u8>` makes `Env` invariant over `'a`, preventing any `Term<'a>` from being stored past the call's lifetime. There is no runtime check — this is enforced entirely by the type system.

```rust
pub struct Env<'a> {
    pub kind: EnvKind,
    env: *mut NifEnv,
    _id: PhantomData<*mut &'a u8>,
}

pub enum EnvKind {
    ProcessBound,       // standard NIF call env (constructed by codegen)
    Callback,           // resource destructor/monitor callback env
    Init,               // load callback env — only valid for resource registration
    ProcessIndependent, // allocated with enif_alloc_env
}
```

`EnvKind` and `Env.kind` are `pub` because generated code constructs `ProcessBound` and `Init` envs. `register_resource_type` asserts `env.kind == EnvKind::Init` at runtime.

### `OwnedEnv`

A process-independent environment for building and sending terms from outside a NIF call (e.g. from a spawned OS thread). Simple struct with one field:

```rust
pub struct OwnedEnv {
    env: *mut NifEnv,
}

impl OwnedEnv {
    pub fn new() -> OwnedEnv;
    pub fn send<F>(&mut self, pid: &Pid, f: F) -> bool
    where F: FnOnce(Env<'_>) -> Term<'_>;
    pub fn clear(&mut self);
}
```

`send` is closure-based: the closure builds a term in a temporary env, sends it to `pid`, and clears automatically. Terms cannot escape the closure — the lifetime is tied to the closure's scope. `OwnedEnv` implements `Drop` (calls `enif_free_env`), `Default`, and is `Send`.

---

## Layer 4: Terms (`term.rs` and `types/`)

### Three levels of resolution

**Level 1 — `RawTerm<'a>`:** The bare machine word plus its `Env`. Zero work done. The fastest possible representation. A received type — you cannot construct one from scratch.

**Level 2 — `Term<'a>` enum:** One `enif_term_type` call has been made. The correct variant is known. Data is still on the BEAM heap.

```rust
pub enum Term<'a> {
    Atom(Atom), Binary(Binary<'a>), Bitstring(Bitstring<'a>),
    Float(Float<'a>), Fun(Fun<'a>), Integer(Integer<'a>),
    List(List<'a>), Map(Map<'a>), Pid(Pid), Port(Port),
    Reference(Reference<'a>), Tuple(Tuple<'a>),
}
```

12 variants for 11 type tags — `Bitstring` maps to both `Binary` and `Bitstring` depending on `enif_is_binary`.

`Term` and `RawTerm` implement `PartialEq`/`Eq` (via `enif_is_identical`) and `PartialOrd`/`Ord` (via `enif_compare`).

All concrete types implement `From<T> for Term<'a>`, so `let t: Term = atom.into()` works. `RawTerm` converts via `From` as well (calls `resolve()`).

**Level 3 — concrete types:** Type is known. Data is still on the BEAM heap. Accessor methods pull data out on demand.

### Lazy by default

Construction is always free. Extraction is on demand. Every concrete type is `NifTerm` + `Env<'a>`. No data is read from the BEAM heap until explicitly requested.

### Lifetime rules

- `Atom`, `Pid`, `Port` — no lifetime. Tagged immediates, valid anywhere.
- `Integer<'a>`, `Float<'a>`, `Binary<'a>`, `Bitstring<'a>`, `Fun<'a>`, `List<'a>`, `Map<'a>`, `Reference<'a>`, `Tuple<'a>` — carry `'a` because values may live on the BEAM heap.
- `Bitstring` and `Fun` carry `env` for lifetime only — no NIF inspection functions exist for them. These fields have `#[allow(dead_code)]`.

### `TermIn` — universal term input

Functions that accept a term as input use `impl TermIn` instead of `Term<'a>`. This sealed trait is implemented for all otter term types (`Atom`, `Binary`, `Integer`, `List`, `Term`, `RawTerm`, etc.) and for `&T` where `T: TermIn`. It extracts the underlying `NifTerm` without allocating or copying.

This means you can pass concrete types directly — no `.encode(env)` needed:

```rust
map.put(atom_key, integer_val)
List::from_terms(env, [int1, int2, int3])
env.raise(some_atom)
```

`TermIn` is sealed — it cannot be implemented outside the crate.

### Per-type methods

```rust
// Atom
fn new(env, name: &str) -> Option<Atom>       // create/intern
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
fn from_f64(env, val) -> Float<'a>

// Binary
fn as_bytes(self) -> &'a [u8]     // zero-copy into BEAM heap
fn len(self) -> usize
fn try_str(self) -> Result<&'a str, Utf8Error>
fn sub(self, pos, len) -> Binary<'a>   // zero-copy slice
fn from_bytes(env, data) -> Binary<'a>
fn to_term(self, env, safe) -> Option<Term<'a>>  // deserialize from external format
impl Deref<Target=[u8]>            // auto-coerce to &[u8]
impl AsRef<[u8]>                   // trait-based byte access
impl Debug                         // Binary(N bytes)

// BinaryBuilder — growable buffer (Vec<u8> model)
fn new() -> BinaryBuilder
fn with_capacity(cap) -> BinaryBuilder
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
impl Debug                         // BinaryBuilder { len, capacity }

// List (cons cell)
fn node(self) -> Node<'a>           // decompose: Nil or Cell(head, tail)
fn iter(self) -> ListIterator<'a>   // yields RawTerm heads; .tail() for terminal
fn len(self) -> Option<usize>       // O(n), None for improper lists
fn reverse(self) -> Option<List<'a>>
fn try_string(self) -> Result<String, CodecError>
fn from_terms(env, terms) -> List<'a>
fn from_str(env, &str) -> List<'a>  // UTF-8 string → list of codepoints
fn cons(env, head, tail) -> List<'a>

// Tuple
fn len(self) -> usize
fn element(self, i) -> Term<'a>
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

// Port
fn whereis(env, name: Atom) -> Option<Port>
fn command(self, env, msg) -> bool

// Reference
fn new(env) -> Reference<'a>

// Term
fn to_binary(self, env) -> Option<Binary<'a>>  // serialize to external format
```

### Env methods (defined in `term.rs`)

```rust
impl<'a> Env<'a> {
    fn consume_timeslice(self, percent: i32) -> bool
    fn make_unique_integer(self, properties) -> Term<'a>
    fn hash(self, algorithm, term, salt) -> u64
    fn is_current_process_alive(self) -> bool
    fn raise(self, reason: impl TermIn) -> Term<'a>
    fn raise_badarg(self) -> Term<'a>
    unsafe fn schedule_nif(self, name, flags, fp, argc, argv) -> Term<'a>
    fn set_option_delay_halt(self, ms) -> bool
    unsafe fn set_option_on_halt(self, callback) -> bool
    unsafe fn set_option_on_unload_thread(self, callback) -> bool
}
```

---

## Layer 5: Codec (`codec.rs`)

```rust
pub enum CodecError { WrongType, IntegerOverflow, InvalidCodepoint }

pub trait Encoder {
    fn encode<'a>(&self, env: Env<'a>) -> RawTerm<'a>;
}

pub trait Decoder<'a>: Sized {
    fn decode(term: Term<'a>) -> Result<Self, CodecError>;
}
```

Implemented for all otter term types. Not implemented for native Rust types.

Note: a blanket `TryFrom<Term<'a>> for T: Decoder<'a>` cannot be provided — it violates Rust's orphan rules (E0210). Use `T::decode(term)` directly.

---

## Layer 6: Resources (`resource/`)

### `Resource` trait

```rust
pub trait Resource: Sized + Send + Sync + 'static {
    fn resource_type_handle() -> &'static OnceLock<ResourceTypeHandle>;
    fn destructor(self, _env: Env<'_>) {}
    fn down<'a>(&'a self, _env: Env<'a>, _pid: Pid, _monitor: Monitor) {}
}
```

### `ResourceArc<T>`

Two-pointer layout: `raw` (allocation start for keep/release) and `inner` (aligned write position for Deref/destructor). Implements `Encoder`, `Decoder`, `Deref<Target=T>`, `Clone`, `Drop`, `From<T>`.

### `Monitor`

Wraps `NifMonitor`. Implements `PartialEq`/`Eq` via `enif_compare_monitors`. Has `to_term(env)` via `enif_make_monitor_term`.

### Registration

Explicit. `register_resource_type::<T>(env, name)` must be called from the load callback (`EnvKind::Init`). Panics if called from wrong context or called twice.

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
- **Type predicates** (`enif_is_atom`, `enif_is_list`, etc.) — `Term` enum + pattern matching is strictly better.
- **Raw memory allocation** (`enif_alloc`/`enif_free`) — use Rust's allocator.
- **NIF threading primitives** (`enif_mutex_*`, `enif_cond_*`, etc.) — use `std::sync`.
