# rebar3_otter

[![Hex.pm](https://img.shields.io/hexpm/v/rebar3_otter.svg)](https://hex.pm/packages/rebar3_otter)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)

A rebar3 plugin that builds Rust NIF crates and installs the resulting shared
library where `erlang:load_nif/2` expects it. It pairs with
[`otter-nif`](https://crates.io/crates/otter-nif) — write NIFs in Rust, then let
this plugin drive `cargo` from your normal rebar3 build.

The plugin has no Rust dependency of its own. It treats the Rust toolchain as an
external tool, the same way rebar3 treats the Erlang compiler, and will build any
crate that produces a `cdylib` — it is otter-agnostic.

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

Declare the crates to build, then wire the providers as build hooks:

```erlang
{otter_crates, [
    #{
        name     => my_nif,           % must match Cargo.toml [package].name
        path     => "native/my_nif",  % crate dir, relative to the app dir
        mode     => release,          % release | debug (default: release)
        features => [],               % Cargo features to enable
        target   => undefined         % cross-compile triple, or undefined
    }
]}.

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
