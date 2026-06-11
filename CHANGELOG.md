# Changelog

## 0.1.0

Initial release.

### otter (core library)

- All 12 Erlang term types: Atom, Integer, Float, Binary, Bitstring, List, Tuple, Map, Pid, Port, Reference, Fun
- Three-level term resolution: RawTerm (zero work) → TypedTerm (one `enif_term_type` call) → concrete-type extraction
- Compile-time lifetime safety via invariant `Env<'a>` constructed from a stack borrow
- `Encoder`/`Decoder` traits for all types and `ResourceArc`; `Encoder::encode` returns `RawTerm<'a>` to avoid an extra `enif_term_type` call on every encoded return
- `impl Encoder for Result<T: Encoder, E: Encoder>`: `Ok` encodes normally, `Err` encodes and raises via `enif_raise_exception` — auto-raise dispatched by trait, not macro magic
- `BinaryBuilder` with `io::Write`, `Extend`, `Deref`/`DerefMut`, and `Drop`-on-leak release
- `List` as cons cell with `Node::{Nil, Cell}`, `ListIterator` (with `FusedIterator` and `IntoIterator`), `len`, `reverse`, `try_string`, `from_str`
- `MapIterator` with `enif_map_iterator_*` and `Drop`
- Resource types with destructors, process monitors, dynamic resource calls, and `catch_unwind` around both `destructor` and `down` callbacks so a user-code panic absorbs cleanly instead of unwinding across the FFI boundary
- `OwnedEnv::send` for building and sending terms from non-scheduler threads; closure-based, lifetime-bound
- Pre-declared atoms via `declare_atoms!` / `init_atoms!` / `atom!` macros — single atomic load per retrieval, no NIF call
- `time` (monotonic, offset, unit conversion), `system` (thread type), and `select` / `select_x` (I/O event multiplexing) modules
- `TermIn` sealed trait for polymorphic term arguments (`Atom`, `Integer`, `TypedTerm`, `RawTerm`, etc., and `&T` where `T: TermIn`)
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
