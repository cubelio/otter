# `rebar3_otter`

[![CI](https://github.com/cubelio/otter/actions/workflows/ci.yml/badge.svg?branch=master)](https://github.com/cubelio/otter/actions/workflows/ci.yml)
[![rebar3_otter](https://img.shields.io/hexpm/v/rebar3_otter?logo=erlang&label=rebar3_otter&color=6E4A7E)](https://hex.pm/packages/rebar3_otter)
[![hexdocs](https://img.shields.io/badge/hexdocs-rebar3__otter-6E4A7E?logo=hex)](https://rebar3-otter.hexdocs.pm)
[![OTP](https://img.shields.io/badge/OTP-%E2%89%A527-blue?logo=erlang)](#requirements)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)

A `rebar3` plugin that builds Rust NIF crates written with
[`otter-nif`](https://crates.io/crates/otter-nif) and installs the resulting
shared library where `erlang:load_nif/2` expects it. Write NIFs in Rust, then
let this plugin drive `cargo` from your normal `rebar3` build.

The plugin has no Rust dependency of its own — it treats the Rust toolchain as
an external tool, the same way `rebar3` treats the Erlang compiler.

## Requirements

- **OTP 27** — the plugin uses OTP-27 doc syntax (`-moduledoc`/`-doc`).
- **`cargo` on `PATH`** — toolchain installation is the user's responsibility.

(The *NIF runtime* floor is lower — NIF 2.17 / OTP 26 — but that governs loading
the compiled library into a BEAM, not this build-time plugin.)

## Install

Add the plugin to your `rebar.config`:

```erlang
{plugins, [rebar3_otter]}.
```

Declare the crates to build:

```erlang
{otter_crates, [
    #{name => "my_nif", path => "native/my_nif"}
]}.
```

Each entry in `otter_crates` is a map describing one crate:

| Key | Required? | Type | Description |
|---|---|---|---|
| `name` | Required | `string() \| binary()` | Cargo crate name — must match `[package].name` in the crate's `Cargo.toml`. |
| `path` | Required | `string() \| binary()` | Crate directory, relative to the declaring app's directory. |
| `mode` | Optional | `release \| debug` | Cargo build profile. Defaults to `release`. |
| `features` | Optional | `[string() \| binary()]` | Cargo features to enable. Defaults to `[]`. |
| `target` | Optional | `string() \| binary() \| undefined` | Cross-compilation target triple; `undefined` builds for the host. Defaults to `undefined`. |

String-typed values (`name`, `path`, `target`, and each `features` element)
accept a string or a binary, normalized to a string. They mirror Cargo's own
strings and carry hyphens (e.g. `"x86_64-unknown-linux-gnu"`) naturally.

Then wire the providers as build hooks:

```erlang
{provider_hooks, [
    {pre, [
        {compile, otter_compile},
        {clean,   otter_clean}
    ]}
]}.
```

`otter_crates` is read **per application**: `path` resolves relative to that
app's directory and the built `.so` is installed into that app's
`priv/native/`. In an umbrella project each app declares the crates it owns.

## Scaffold a crate

```
rebar3 otter new --name my_nif
```

Generates `native/my_nif/` with a minimal `Cargo.toml` (depending on
`otter-nif`) and a `src/lib.rs` containing a `hello` NIF. The Erlang loader is
intentionally **not** generated — NIF loading is two lines of standard Erlang you
should write yourself:

```erlang
-module(my_nif).
-on_load(init/0).

init() ->
    erlang:load_nif(filename:join(code:priv_dir(my_app), "native/my_nif"), 0).

%% Stub replaced at load time by the NIF implementation
hello() -> exit(nif_not_loaded).
```

## Providers

| Provider | Namespace | Role |
|---|---|---|
| `otter_compile` | `default` | `pre_compile` hook — runs `cargo build`, copies the `cdylib` into `priv/native/`. |
| `otter_clean`   | `default` | `pre_clean` hook — removes the installed `.so` and the crate's `target/`. |
| `new`           | `otter`   | `rebar3 otter new --name <n>` — scaffolds a NIF crate. |

`cargo` runs with `ERTS_INCLUDE_DIR` set to the running ERTS's include dir, so
the native build can locate `erl_nif.h` without extra configuration.

See [DESIGN.md](DESIGN.md) for the full provider design, artifact-path
resolution, and error handling.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.
