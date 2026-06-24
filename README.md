# `otter`

[![CI](https://github.com/cubelio/otter/actions/workflows/ci.yml/badge.svg?branch=master)](https://github.com/cubelio/otter/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)
[![MSRV](https://img.shields.io/badge/MSRV-1.82-blue.svg)](#requirements)

**otter — write fast and efficient Erlang NIFs in Rust.**

This is the monorepo for **`otter`** (published as `otter-nif` on crates.io).
For the introduction, quick start, and feature tour:
- [README](otter/README.md)
- [crates.io](https://crates.io/crates/otter-nif)
- [docs.rs](https://docs.rs/otter-nif)

## Repository layout

| Path | Crate / App | Package | Docs | What it is |
|---|---|---|---|---|
| [`otter/`](otter) | `otter-nif` | [![otter-nif](https://img.shields.io/crates/v/otter-nif?logo=rust&label=crates.io&color=E37222)](https://crates.io/crates/otter-nif) | [![docs.rs](https://img.shields.io/docsrs/otter-nif?logo=docsdotrs&label=docs.rs&color=E37222)](https://docs.rs/otter-nif) | Core Rust library — types, codecs, environment, resources (imported as `otter`) |
| [`otter_codegen/`](otter_codegen) | `otter-nif-macros` | [![otter-nif-macros](https://img.shields.io/crates/v/otter-nif-macros?logo=rust&label=crates.io&color=E37222)](https://crates.io/crates/otter-nif-macros) | — | Proc macros (`#[otter::nif]`, `otter::init!`), re-exported through `otter-nif` |
| [`rebar3_otter/`](rebar3_otter) | `rebar3_otter` | [![rebar3_otter](https://img.shields.io/hexpm/v/rebar3_otter?logo=erlang&label=hex.pm&color=6E4A7E)](https://hex.pm/packages/rebar3_otter) | [![hexdocs](https://img.shields.io/badge/hexdocs-online-6E4A7E?logo=hex)](https://rebar3-otter.hexdocs.pm) | rebar3 plugin — drives `cargo build`/`cargo clean` and installs the NIF into `priv/` |
| [`otter_test/`](otter_test) | `otter_test` | — | — | In-tree Erlang app — end-to-end test suite exercising the bridge |

You only ever depend on `otter-nif` (imported as `otter`); the codegen macros are
re-exported through it.

## Documentation

| Document | Contents |
|---|---|
| [otter/README.md](otter/README.md) | Start here — introduction, quick start, feature tour |
| [docs/USAGE.md](docs/USAGE.md) | User-facing guide — setup, all types, atoms, resources, message passing, scheduling, select |
| [docs/RESOURCES.md](docs/RESOURCES.md) | Deep dive on the resource lifecycle |
| [docs/UPGRADE.md](docs/UPGRADE.md) | Hot-upgrade safety model and the no-cross-build-ABI invariant |
| [docs/RUSTLER.md](docs/RUSTLER.md) | Design comparison with rustler |
| [docs/MIGRATION.md](docs/MIGRATION.md) | Side-by-side rustler-to-otter migration guide |
| [otter/DESIGN.md](otter/DESIGN.md) | Core library architecture and internals |
| [otter_codegen/DESIGN.md](otter_codegen/DESIGN.md) | What the macros generate, argument/return type rules |
| [rebar3_otter/DESIGN.md](rebar3_otter/DESIGN.md) | Plugin providers, cargo integration, NIF loading path |

## Requirements

**`otter-nif`** — the Rust library

- **OTP 26** (NIF 2.17) runtime floor; the optional `nif_2_18` feature requires **OTP 29** (NIF 2.18)
- **Rust** edition 2021, MSRV 1.82 (`otter-nif-macros` 1.56)
- `cargo` on `PATH`

**`rebar3_otter`** — the rebar3 plugin

- **OTP 27** (it uses OTP-27 doc syntax)

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in this project by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
