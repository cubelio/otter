# Otter

Otter is a Rust library for writing Erlang NIFs. It maps the NIF C ABI directly into Rust types with compile-time lifetime safety and zero hidden magic.

**Status:** 0.1, pre-release. The full surface is implemented and exercised end-to-end by [test_apps/otter_demo](test_apps/otter_demo/), but otter has not yet been used in production. Feedback on the API shape, the Erlang-first philosophy, and the safety model is welcome — open an issue.

## Why

Rustler targets Elixir and Mix. It does not support Erlang. Otter is built for Erlang from the ground up — if an Erlang programmer wouldn't recognise a concept, it doesn't belong.

Concrete differences from rustler: no serde, no Elixir types, no linker-based registration magic, no `NifUntaggedEnum`, no `Error` enum. Lists are cons cells. Registration is explicit. See [RUSTLER.md](docs/RUSTLER.md) for a detailed comparison.

## Quick start

### Rust (`native/my_nifs/src/lib.rs`)

```rust
use otter::env::Env;
use otter::types::{Atom, Integer};

otter::declare_atoms![world];

#[otter::nif]
fn hello(_env: Env) -> Atom {
    otter::atom![world]
}

#[otter::nif]
fn add<'a>(env: Env<'a>, a: Integer<'a>, b: Integer<'a>) -> Integer<'a> {
    let sum = i64::try_from(a).unwrap() + i64::try_from(b).unwrap();
    Integer::from_i64(env, sum)
}

fn on_load(env: Env, _load_info: otter::term::Term) -> bool {
    otter::init_atoms!(env);
    true
}

otter::init!("my_nifs", [hello, add], load = on_load);
```

### Erlang (`src/my_nifs.erl`)

```erlang
-module(my_nifs).
-on_load(init/0).
-export([hello/0, add/2]).

init() ->
    erlang:load_nif(filename:join(code:priv_dir(my_app), "native/my_nifs"), 0).

hello()     -> exit(nif_not_loaded).
add(_A, _B) -> exit(nif_not_loaded).
```

### Cargo.toml (`native/my_nifs/Cargo.toml`)

```toml
[package]
name = "my_nifs"
version = "0.1.0"
edition = "2024"

[lib]
crate-type = ["cdylib"]

[dependencies]
otter = { git = "https://github.com/cubelio/otter.git" }
```

### rebar.config

```erlang
{plugins, [
    {rebar3_otter, {git, "https://github.com/cubelio/otter.git", {branch, "master"}}}
]}.
{provider_hooks, [
    {pre, [{compile, otter_compile}, {clean, otter_clean}]}
]}.
{otter_crates, [
    #{name => my_nifs, path => "native/my_nifs"}
]}.
```

Build with `rebar3 compile`. The plugin calls `cargo`, finds the `.so`, and puts it in `priv/native/`.

## Components

| Crate / App | What it does |
|---|---|
| `otter` | Core Rust library — types, codecs, environment, resources |
| `otter_codegen` | Proc macros (`#[otter::nif]`, `otter::init!`) — re-exported through `otter` |
| `rebar3_otter` | rebar3 plugin — drives `cargo build` and `cargo clean` |

You only depend on `otter`. The codegen macros are re-exported through it.

## Features

- **All 12 Erlang term types** — Atom, Integer, Float, Binary, Bitstring, List, Tuple, Map, Pid, Port, Reference, Fun
- **Two-level term resolution** — `RawTerm` (zero cost) → `Term` (one NIF call) → data extraction. Pay only for what you use.
- **Compile-time lifetime safety** — `Env<'a>` ties every term to its NIF call. Terms cannot escape. No runtime checks.
- **Pre-declared atoms** — `declare_atoms!` / `init_atoms!` / `atom!` for zero-cost atom retrieval (single atomic load)
- **Resource types** — BEAM-managed Rust objects with destructors and process monitors
- **OwnedEnv** — build and send terms from background threads
- **Dirty schedulers** — `#[otter::nif(schedule = "DirtyCpu")]` / `"DirtyIo"`
- **Result returns** — `Result<T, E>` where Ok encodes normally and Err raises an exception
- **BinaryBuilder** — growable binary buffer with `io::Write` support
- **I/O select** — `enif_select` / `enif_select_x` for async I/O integration
- **Panic safety** — panics in NIF bodies are caught and converted to exceptions

## Requirements

- **OTP 26+** (NIF version 2.17). Optional `nif_2_18` feature for OTP 29.
- **Rust** edition 2024.
- `cargo` on `PATH`.

## Documentation

| Document | Contents |
|---|---|
| [docs/USAGE.md](docs/USAGE.md) | User-facing guide — setup, all types, atoms, resources, OwnedEnv, scheduling, select |
| [docs/RESOURCES.md](docs/RESOURCES.md) | Deep dive on the resource lifecycle |
| [docs/RUSTLER.md](docs/RUSTLER.md) | Design comparison with rustler |
| [docs/MIGRATION.md](docs/MIGRATION.md) | Side-by-side rustler-to-otter migration guide |
| [otter/DESIGN.md](otter/DESIGN.md) | Core library architecture and internals |
| [otter_codegen/DESIGN.md](otter_codegen/DESIGN.md) | What the macros generate, argument/return type rules |
| [rebar3_otter/DESIGN.md](rebar3_otter/DESIGN.md) | Plugin providers, cargo integration, NIF loading path |

## License

Copyright © 2026 Lynn Gabbay.

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in this project by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
