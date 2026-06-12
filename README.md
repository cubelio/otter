# Otter

Otter is a Rust library for writing Erlang NIFs. It maps the NIF C ABI directly into Rust types with compile-time lifetime safety and zero hidden magic.

**Status:** 0.1, pre-release. The full surface is implemented and exercised end-to-end by [test_apps/otter_demo](test_apps/otter_demo/), but otter has not yet been used in production. Feedback on the API shape, the Erlang-first philosophy, and the safety model is welcome — open an issue.

## Why

There is already an established library that builds Erlang NIFs from Rust, `rustler`. As a regular user of `rustler`, I ran up against many points of friction. The design and documentation lean toward Elixir over Erlang. The API surface made several opinionated decisions, like how to convert terms and when to raise an exception. It prefers syntactic sugar to explicitness.

I built `otter` to be on the opposite end of the spectrum. Everything is explicit and as close to the original NIF C API as possible. The design philosophy was to expose the full capabilities of the NIF API in the most idiomatic Rust way without any opinionated decisions hidden in the scaffolding. If a NIF programmer wouldn't recognize a concept, it doesn't belong.

See [docs/RUSTLER.md](docs/RUSTLER.md) for a detailed comparison.

*Note on Elixir.* For now, `otter` ships no Elixir-specific tooling. Getting the Erlang-facing library right is the current priority; once the surface stabilizes, we will revisit building Elixir tooling on top of the `otter` framework or as an opt-in feature.

## Quick start

This walks through a working NIF from an empty directory. It assumes `rebar3`,
`cargo`, and an OTP 26+ install are on your `PATH`. The example uses an
application called `my_app` with a NIF crate called `my_nifs`.

**1. Create the Erlang application.**

```console
$ rebar3 new app name=my_app
$ cd my_app
```

**2. Add the plugin to `rebar.config`.** The plugin lives in a subdirectory of
the otter repo, so it must be referenced with `git_subdir`:

```erlang
{plugins, [
    {rebar3_otter, {git_subdir, "https://github.com/cubelio/otter.git", {branch, "master"}, "rebar3_otter"}}
]}.
```

**3. Scaffold the NIF crate.**

```console
$ rebar3 otter new --name my_nifs
```

This creates `native/my_nifs/Cargo.toml` (already depending on otter from git)
and `native/my_nifs/src/lib.rs` with a minimal NIF:

```rust
use otter::env::Env;
use otter::term::TypedTerm;
use otter::types::Atom;

otter::declare_atoms![world];

fn on_load(env: Env, _info: Term) -> bool {
    otter::init_atoms!(env);
    true
}

#[otter::nif]
fn hello(_env: Env) -> Atom {
    otter::atom![world]
}

otter::init!("my_nifs", [hello], load = on_load);
```

**4. Register the crate and build hooks in `rebar.config`** (the scaffolder
prints this for you):

```erlang
{otter_crates, [
    #{name => my_nifs, path => "native/my_nifs"}
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

**6. Build.** The `pre_compile` hook invokes `cargo` (pulling otter from git on
the first build), locates the `.so`, and installs it into `priv/native/`.

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
resources, scheduling — see [docs/USAGE.md](docs/USAGE.md).

## Components

| Crate / App | What it does |
|---|---|
| `otter` | Core Rust library — types, codecs, environment, resources |
| `otter_codegen` | Proc macros (`#[otter::nif]`, `otter::init!`) — re-exported through `otter` |
| `rebar3_otter` | rebar3 plugin — drives `cargo build` and `cargo clean` |

You only depend on `otter`. The codegen macros are re-exported through it.

## Features

- **All 12 Erlang term types** — Atom, Integer, Float, Binary, Bitstring, List, Tuple, Map, Pid, Port, Reference, Fun
- **Two-level term resolution** — `Term` (zero cost) → `TypedTerm` (one NIF call) → data extraction. Pay only for what you use.
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
