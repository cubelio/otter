# Changelog

## Unreleased

Hot code upgrade: every otter module is now a hot-upgradeable NIF library, and resource type registration moves into `init!`.

### otter (core library)

- **Breaking.** Resource registration is now declared in `init!`'s `resources = [...]` list rather than from a user `load` callback. `register`/`register_tagged` replace `register_resource_type`/`register_resource_type_named`; they require a `Load` or `Upgrade` env
- **Breaking.** Resource creation is `env.make_resource(val)`; `ResourceArc::from(val)` is removed. `make_resource` is shorthand for `env.resource_handle::<T>().make(val)` and panics if `T` was never registered
- **Breaking.** The `Resource` trait no longer carries a `resource_type_handle()` method; the registry lives in `priv_data`, keyed by type
- **Breaking.** `EnvKind::Init` is renamed `EnvKind::Load`; `EnvKind::Upgrade` and `EnvKind::Unload` are added. `Env::kind` distinguishes the upgrade-lifecycle contexts so registration can be gated to `Load`/`Upgrade`
- Per-build ABI tag in resource type names: `register` names the BEAM-side type `"{type_name}#abi={hash}"` (a per-build hash, so a different build never takes this build's resources across an upgrade); `register_tagged` names it `"{type_name}#tag={tag}"` (an explicit per-type cross-build promise)
- Tier-2 `_raw` lifecycle callbacks (`load_raw`/`upgrade_raw`/`unload_raw`) behind the `raw` feature, handing the user the library's `priv_data` `void*` directly
- Opt-in enif-backed global allocator via `otter::enif_global_allocator!()` (`EnifAlloc`, backed by `enif_alloc`/`enif_free`). Inert until installed — the macro is what pulls in the direct-linked symbols, so otter still links into ordinary non-BEAM binaries until you opt in

### otter_codegen (proc macros)

- `otter::init!` now emits non-NULL `load`/`upgrade`/`unload` NIF callbacks for every module, so every otter NIF library is hot-upgradeable. New `init!` keys: `resources = [...]`, `load`/`upgrade`/`unload` and their `_raw` variants

## 0.1.0

Initial release.

### otter (core library)

- All 12 Erlang term types: Atom, Integer, Float, Binary, Bitstring, List, Tuple, Map, Pid, Port, Reference, Fun
- Three-level term resolution: Term (zero work) → TypedTerm (one `enif_term_type` call) → concrete-type extraction. `enif_term_type` is read as a raw `c_int` and never transmuted into the term-type enum, so a term type added by a future OTP is handled safely — `Term::resolve` returns `Option<TypedTerm>` (`None` for an unrecognized type) and `TypedTerm: TryFrom<Term>` fails with `CodecError::UnknownTermType` — rather than undefined behavior. Building accessors (`Tuple::element`, `Map::get`/`iter`, `ListIterator::tail`, `binary_to_term`, `make_monitor_term`, `schedule_nif`) return raw `Term`, classifying lazily; the raw code is available via `Term::term_type_raw` under the `raw` feature. Constructors whose result type is statically known by the C contract return that type directly: `make_unique_integer → Integer`, `cpu_time → Tuple`
- Sound local/external split for pids and ports: `Pid<'a>`/`Port<'a>` are env-bound handles of unestablished locality (an external, remote-node pid/port is heap-boxed and must not be stored past its env); `LocalPid`/`LocalPort` are `Copy`, lifetime-free, storable handles validated via `enif_get_local_pid`/`_port` (or `enif_self`/`enif_whereis_*`). Refine with `Pid::to_local() -> Option<LocalPid>`. The operations that require an internal pid/port — `enif_send`, `enif_monitor_process`, `enif_select`, `enif_is_process_alive`, `enif_port_command`, `enif_is_port_alive` — take the validated `LocalPid`/`LocalPort` and never build one from an unvalidated term, closing the use-after-free / garbage-process-table-indexing gap (assessment `audit-03`)
- Compile-time lifetime safety via invariant `Env<'a>` constructed from a stack borrow
- `Encoder`/`Decoder` traits for all types and `ResourceArc`; `Encoder::encode` returns `Term<'a>` to avoid an extra `enif_term_type` call on every encoded return
- Env-as-receiver safe layer: every `enif_foo(env, …)` is exposed as `env.foo(…)` (e.g. `env.make_tuple`, `env.is_binary`, `env.get_map_value`), with `enif.rs` the sole `unsafe`/`funcs()` floor (`pub` under the `raw` feature). Per-type constructors (`Atom::intern`, `Binary::from_bytes`, …) delegate to these
- Exception model via `Raised<'a>`, an unforgeable witness that an exception is already pending: `env.raise_exception(reason)` / `env.make_badarg()` (generic over the success type) and fallible builders return `Result<_, Raised>`, which the `Encoder` returns raw at NIF exit — never re-raised, so double-raising is impossible. `make_double` / `Float::from_f64` / `cpu_time` / `schedule_nif` are fallible accordingly; `Env::check_raised` guards `raw`-surface calls that may raise
- `Env::send` / `Env::port_command` (in-NIF; NULL `msg_env`, copy from the call env), `OwnedEnv::send` / `OwnedEnv::port_command` (from a non-scheduler thread, via the owned env), and `Env::cpu_time`. `LocalPid::self_` panics rather than yielding an invalid pid if called off a process-bound env
- `BinaryBuf` — the RAII owner of a mutable `ErlNifBinary`: `Drop` releases it, `into_binary(env)` hands the allocation to the BEAM as a `Binary` term, and `as_bytes()`/`Deref` (`.to_vec()`) read it. Building API (`push`/`extend_from_slice`/`resize`/`reserve`/`io::Write`/`Extend`/`Deref`/`DerefMut`). `Bitstring::to_binary()` refines a byte-aligned bitstring to `Binary`
- ETF codec named for meaning, not the `x_to_y` BIF: `Term::serialize() -> Option<BinaryBuf>` (type-agnostic, on `Term` — no `resolve`; caller chooses `.into_binary(env)` for a term or `.as_bytes()`/`.to_vec()` for bytes, rather than otter imposing a term) and `Binary::deserialize`/`Env::deserialize(bytes, safe) -> Option<Term>`
- `List` as cons cell with `Node::{Nil, Cell}`, `ListIterator` (with `FusedIterator` and `IntoIterator`), `len`, `reverse`, `try_string`, `from_str`
- `MapIterator` with `enif_map_iterator_*` and `Drop`
- Resource types with destructors, process monitors, dynamic resource calls, and `catch_unwind` around both `destructor` and `down` callbacks so a user-code panic absorbs cleanly instead of unwinding across the FFI boundary
- `OwnedEnv::send` for building and sending terms from non-scheduler threads; closure-based, lifetime-bound
- Pre-declared atoms via `declare_atoms!` / `init_atoms!` / `atom!` macros — single atomic load per retrieval, no NIF call
- `time` (monotonic, offset, unit conversion), `system` (thread type), and `select` / `select_x` (I/O event multiplexing) modules
- `AsNifTerm<'a>` sealed trait for polymorphic term arguments (`Atom`, `Integer`, `TypedTerm`, `Term`, etc., and `&T` where `T: AsNifTerm<'a>`); lifetime parameter rejects cross-env terms at compile time. Every term *input* takes `impl AsNifTerm<'a>` uniformly — including `select`/`select_x` and `dynamic_resource_call`, which previously imposed `TypedTerm`
- Minimum NIF version 2.17 (OTP 26); optional `nif_2_18` feature for OTP 29
- Unix-only by construction (`compile_error!` on non-Unix targets)

### otter_codegen (proc macros)

- `#[otter::nif]` — generates the `extern "C"` wrapper, including argument unpacking via `Decoder`, panic catching, an `argc`-vs-arity guard that falls back to `badarg` on mismatch, and return encoding via a single `Encoder::encode` call
- Env-first argument rule: the first parameter is the call's `Env<'a>`; every subsequent parameter is decoded through `Decoder`. No name-based classification; an aliased env or a user type named `TypedTerm` works correctly
- `Encoder`-bound assertion on the return type surfaces missing `Encoder` impls as "the trait `Encoder` is not implemented for `T`" rather than a method-not-found error deep in the wrapper
- Dirty-scheduler flags emitted as `NIF_FUNC_DIRTY_CPU` / `NIF_FUNC_DIRTY_IO` constants, not bare literals
- `otter::init!` — generates the `nif_init` entry point, populates the `enif_*` function pointer table via `dlsym`, and wires the optional user `load` callback
- `#[otter::resource_impl]` — pass-through placeholder reserved for future resource boilerplate
- Attributes: `name = "..."`, `schedule = "DirtyCpu"`, `schedule = "DirtyIo"`
- `trybuild` UI tests lock in the macro's stable diagnostics (`fail_missing_env`, `fail_return_not_encoder`)

### rebar3_otter (rebar3 plugin)

- `otter_compile` pre-hook — invokes `cargo rustc --message-format=json-render-diagnostics`, parses the `compiler-artifact` line for the `cdylib` output, and copies the artifact to `priv/native/<name>.<ext>`
- `otter_clean` pre-hook — removes the installed shared object and runs `cargo clean`
- `otter new` provider — scaffolds a minimal NIF crate (Cargo.toml + lib.rs) pointing at the public otter repo
- `rebar3_otter__config` — schema validation for `otter_crates` (required `name`/`path`, normalized `mode`/`features`/`target`, unknown-key rejection); errors halt the build via `rebar_api:abort/2` rather than the pre-hook layer's misleading default
- Staleness detection delegated to cargo's own incremental check; the plugin invokes cargo unconditionally and no-ops cost ~50–200ms when nothing has changed

### Testing

- `test_apps/otter_demo/` — in-tree Erlang application that loads a real compiled `.so` and exercises the bridge end-to-end
- `rebar3 eunit` runs `otter_demo__nif_test:smoke_test_/0`, a generator returning 81 individual EUnit assertions across 26 NIFs covering all 12 term types, atoms, typed args/returns, binary build/sub, list iter/reverse, eq/ord/Debug, `TryFrom`, map/tuple ops, float roundtrip, pid/reference, `Result`-to-exception, dirty-CPU scheduling, `OwnedEnv` cross-thread send/receive, and a `PanickingResource` regression that asserts the VM survives a panic in a destructor
- `cargo test --test codegen_ui` runs the `trybuild` compile-fail suite

### Licensing

- Dual MIT / Apache-2.0 (Rust ecosystem standard)
