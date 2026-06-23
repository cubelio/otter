# Otter

[![CI](https://github.com/cubelio/otter/actions/workflows/ci.yml/badge.svg?branch=master)](https://github.com/cubelio/otter/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)
[![MSRV](https://img.shields.io/badge/MSRV-1.82-blue.svg)](#requirements)

**`otter` — write fast and efficient Erlang NIFs in Rust**

`otter` is built from the ground up to give Erlang and Elixir programmers an easy, efficient way to write NIFs in Rust. Its first priority is an API surface that **faithfully captures the capabilities of the underlying `erl_nif` C API** — and where the safe surface isn't enough, the **raw API** can be exposed by enabling the `raw` feature. The datatypes are designed so you can target speed and efficiency through **controlled layering of `enif_*` calls**, with extensive work to enforce constraints at **compile time rather than runtime**.

- **Full NIF lifecycle** — `load`, `upgrade`, and `unload`, with upgrade-safe `priv_data`, so your module stays hot-code-upgradeable.
- **Faithful lists** — `List` is a real cons cell (`Node::Nil | Cell(head, tail)`); improper lists are first-class, and codecs reject a bad tail with a clean error.
- **The full send matrix** — copy or move (O(1) heap-steal) × attributed or off-thread, including stealing a message heap from inside a NIF.
- **Decoders that don't lie** — `300` into a `u8` is an `IntegerOverflow` error, never a silently-truncated `44`; floats won't quietly swallow integers.
- **Compile-time env identity** — a generative brand makes cross-env misuse a compile error and keeps terms one machine word, with no per-operation runtime env check.

*`otter` is inspired by rustler; see [docs/RUSTLER.md](../docs/RUSTLER.md) for a detailed comparison.*

**Status:** 0.3.0. The full surface is implemented and exercised end-to-end by [test_apps/otter_demo](../test_apps/otter_demo), but otter has not yet been used in production. Feedback on the API shape, the Erlang-first philosophy, and the safety model is welcome — open an issue.

*Note on Elixir.* For now, `otter` ships no Elixir-specific tooling. Getting the Erlang-facing library right is the current priority; once the surface stabilizes, we will revisit building Elixir tooling on top of the `otter` framework or as an opt-in feature.

## Quick start

This walks through a working NIF from an empty directory. It assumes `rebar3`,
`cargo`, and an OTP 27+ install are on your `PATH` (the rebar3 plugin uses OTP-27
doc syntax; the NIF runtime floor itself is OTP 26). The example uses an
application called `my_app` with a NIF crate called `my_nifs`.

**1. Create the Erlang application.**

```console
$ rebar3 new app name=my_app
$ cd my_app
```

**2. Add the plugin to `rebar.config`.** The plugin lives in a subdirectory of
the `otter` repo, so it must be referenced with `git_subdir`:

```erlang
{plugins, [
    {rebar3_otter, {git_subdir, "https://github.com/cubelio/otter.git", {branch, "master"}, "rebar3_otter"}}
]}.
```

**3. Scaffold the NIF crate.**

```console
$ rebar3 otter new --name my_nifs
```

This creates `native/my_nifs/Cargo.toml` (already depending on `otter-nif` from crates.io)
and `native/my_nifs/src/lib.rs` with a minimal NIF:

```rust
use otter::types::{AnyTerm, Atom, CallEnv, InitEnv};

// Optional load hook. Atoms listed in `init!` are interned by the
// scaffolding before this runs, so a fresh crate has nothing to do here.
fn on_load(_env: InitEnv, _load_info: AnyTerm) -> bool {
    true
}

#[otter::nif]
fn hello(_env: CallEnv) -> Atom {
    otter::atom![world]
}

otter::init!("my_nifs", [hello], atoms = [world], load = on_load);
```

**4. Register the crate and build hooks in `rebar.config`** (the scaffolder
prints this for you):

```erlang
{otter_crates, [
    #{name => "my_nifs", path => "native/my_nifs"}
]}.
{provider_hooks, [
    {pre, [{compile, otter_compile}, {clean, otter_clean}]}
]}.
```

**5. Write the Erlang loader module `src/my_nifs.erl`.** This is standard
Erlang and is yours to write — the plugin never generates Erlang source. The
module name must match the name passed to `otter::init!`:

```erlang
-module(my_nifs).
-export([hello/0]).
-on_load(init/0).

init() ->
    erlang:load_nif(filename:join(code:priv_dir(my_app), "native/my_nifs"), 0).

%% Stub replaced at load time by the NIF implementation.
hello() -> exit(nif_not_loaded).
```

**6. Build.** The `pre_compile` hook invokes `cargo` (pulling `otter-nif` from
crates.io on the first build), locates the `.so`, and installs it into `priv/native/`.

```console
$ rebar3 compile
===> Compiling Rust crate my_nifs
===> Installed .../my_app/priv/native/my_nifs.so
```

**7. Verify.**

```console
$ rebar3 shell --eval 'io:format("~p~n", [my_nifs:hello()]), halt().'
world
```

To grow from here — more types, pre-declared atoms, an `on_load` callback,
resources, scheduling — see [docs/USAGE.md](../docs/USAGE.md).

## Components

| Crate / App | What it does |
|---|---|
| `otter-nif` | Core Rust library — types, codecs, environment, resources (imported as `otter`) |
| `otter-nif-macros` | Proc macros (`#[otter::nif]`, `otter::init!`) — re-exported through `otter-nif` |
| `rebar3_otter` | rebar3 plugin — drives `cargo build` and `cargo clean` |

You only depend on `otter-nif` (imported as `otter`). The codegen macros are re-exported through it.

## Features

- **All 11 Erlang term types** — Atom, Integer, Float, Binary/Bitstring, List, Tuple, Map, Pid, Port, Reference, Fun
- **Four-level term resolution** — `AnyTerm` (zero cost) → `TypedTerm` (one NIF call) → concrete term type → native Rust value (`i64`, `String`, `Vec<T>`, … decoded directly). Pay only for what you use.
- **Compile-time lifetime safety** — `Env`/`Term` are traits with an invariant brand `'id` that ties every term to its NIF call. Terms cannot escape. No runtime checks.
- **Native codecs** — `Encoder`/`Decoder` for Rust primitives, `String`, tuples, `Vec<T>`, `HashMap<K,V>` (take and return them directly), plus optional arbitrary-precision integers via the `bigint` feature
- **Pre-declared atoms** — `init!`'s `atoms = [...]` + `atom!` for zero-cost atom retrieval, interned at load and re-interned on upgrade; `Atom::intern` returns `Result<_, AtomError>`
- **Resource types** — BEAM-managed Rust objects with destructors, monitors, and `select` stop callbacks, registered via `init!`'s `resources = [...]`
- **Hot code upgrade** — every `otter` module is hot-upgradeable; a per-build ABI tag on resource type names keeps a different build from unsafely taking over, with an opt-in stable tag (and `raw` callbacks) for state you carry across by hand
- **Message passing** — four free verbs in a 2×2: `send_copy`/`send_move` (copy a live term vs. steal an `OwnedEnvArena` heap) × plain (NULL caller, off-thread) and `_from` (caller-attributed, in-NIF)
- **Dirty schedulers** — `#[otter::nif(schedule = "DirtyCpu")]` / `"DirtyIo"`
- **Result returns** — `Result<T, Raised>` where `Ok` encodes normally and `Err(Raised)` carries an already-pending exception out (raise via `env.raise` / `env.badarg`); an encode failure raises `badret`
- **BinaryBuf** — growable binary buffer with `io::Write` support
- **I/O select** — `enif_select` / `enif_select_x` for async I/O integration
- **enif-backed global allocator** — opt-in `enif_global_allocator!()` routes Rust allocations through the BEAM allocator (`enif_alloc`/`enif_free`)
- **Panic safety** — panics in NIF bodies, encoders, and callbacks are caught and converted to exceptions (with a `panic = "abort"` build guard)
- **Feature flags** — `bigint` (arbitrary-precision integers), `raw` (the raw `enif_*` FFI escape hatch + `_raw` lifecycle callbacks), `nif_2_18` (OTP 29 additions); all off by default

## Requirements

- **OTP 27+** to build through the rebar3 plugin (it uses OTP-27 doc syntax); the NIF runtime floor is 2.17 / OTP 26. Optional `nif_2_18` feature for OTP 29.
- **Rust** edition 2021, MSRV 1.82 (`otter-nif-macros` 1.56).
- `cargo` on `PATH`.

## Documentation

| Document | Contents |
|---|---|
| [docs/USAGE.md](../docs/USAGE.md) | User-facing guide — setup, all types, atoms, resources, message passing, scheduling, select |
| [docs/RESOURCES.md](../docs/RESOURCES.md) | Deep dive on the resource lifecycle |
| [docs/UPGRADE.md](../docs/UPGRADE.md) | Hot-upgrade safety model and the no-cross-build-ABI invariant |
| [docs/RUSTLER.md](../docs/RUSTLER.md) | Design comparison with rustler |
| [docs/MIGRATION.md](../docs/MIGRATION.md) | Side-by-side rustler-to-`otter` migration guide |
| [otter/DESIGN.md](../otter/DESIGN.md) | Core library architecture and internals |
| [otter_codegen/DESIGN.md](../otter_codegen/DESIGN.md) | What the macros generate, argument/return type rules |
| [rebar3_otter/DESIGN.md](../rebar3_otter/DESIGN.md) | Plugin providers, cargo integration, NIF loading path |

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](../LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](../LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in this project by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
