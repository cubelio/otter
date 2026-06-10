# rebar3_otter: rebar3 Plugin

## Purpose

`rebar3_otter` is a rebar3 plugin that integrates Rust NIF compilation into the Erlang build pipeline. It invokes `cargo`, locates the built shared library, and places it where `erlang:load_nif/2` expects to find it.

This is a pure Erlang OTP application. It has no Rust dependency — it treats the Rust toolchain as an external tool, the same way rebar3 treats the Erlang compiler.

---

## What it does not do

- It does not know or care about otter (the Rust library). It will build any Rust crate that produces a `cdylib`.
- It does not generate Erlang boilerplate. NIF loading is standard Erlang and belongs in the user's module.
- It does not manage Rust toolchain installation. `cargo` must already be on `PATH`.

---

## rebar3 Integration

The plugin registers two providers in the `default` namespace; users wire them up via `provider_hooks`:

| Provider name | Module | Hooked as |
|---|---|---|
| `otter_compile` | `rebar3_otter__compile` | `{pre, [{compile, otter_compile}]}` |
| `otter_clean` | `rebar3_otter__clean` | `{pre, [{clean, otter_clean}]}` |

A third provider lives in the `otter` namespace and is invoked directly:

| Provider name | Module | Invoked as |
|---|---|---|
| `new` | `rebar3_otter__new` | `rebar3 otter new --name my_nif` |

---

## Configuration

In `rebar.config`:

```erlang
{plugins, [
    {rebar3_otter, {git, "https://github.com/cubelio/otter.git", {branch, "master"}}}
]}.

{otter_crates, [
    #{
        name    => my_crate,          % must match Cargo.toml [package].name
        path    => "native/my_crate", % path to crate relative to project root
        mode    => release,           % release | debug (default: release)
        features => [],               % list of Cargo features to enable
        target  => undefined          % cross-compile target or undefined
    }
]}.
```

Multiple crates are supported — each entry in `otter_crates` is compiled independently.

---

## Compile Provider (`otter_compile`, module `rebar3_otter__compile`)

Runs as a `pre_compile` hook so the `.so` is in place before the Erlang compiler runs (which may check for NIF existence).

### Steps

1. **Read and validate config** — parse `otter_crates` from `rebar.config` through `rebar3_otter__config:validate/1`, which checks required fields (`name`, `path`), normalizes optional fields (`mode`, `features`, `target`), rejects unknown keys, and produces a list of normalized crate maps. Validation errors halt the build with a formatted message via `rebar_api:abort/2` (the rebar3 pre-hook layer mangles `{error, _}` return values, so config errors take the abort path instead).

2. **Invoke cargo:**
   ```
   cargo rustc \
     --message-format=json-render-diagnostics \
     --manifest-path <path>/Cargo.toml \
     [--release] \
     [--features feat1,feat2] \
     [--target <triple>] \
     -p <name>
   ```
   `--message-format=json-render-diagnostics` causes cargo to emit one JSON object per line on stdout while rendering human-readable diagnostics to stderr. Cargo is invoked unconditionally — its own incremental check decides whether real work needs to happen, and no-ops cost ~50–200ms.

3. **Parse artifact location** — scan cargo's JSON output for a line with `"reason": "compiler-artifact"` where the target `kind` list contains `"cdylib"`. Extract the path from `"filenames"`. This handles workspace layouts, custom `target-dir` settings, and cross-compilation output directories.

4. **Determine output filename** — platform-appropriate extension:
   - Linux: `<name>.so`
   - macOS: `<name>.so` (not `.dylib` — Erlang expects `.so` regardless)
   - Windows: `<name>.dll`

5. **Copy artifact** to `priv/native/<name>.so`. Create `priv/native/` if it does not exist.

6. **Surface diagnostics** — cargo emits compiler errors and warnings on stderr (inherited from the child process), so they appear in the rebar3 build output directly without us needing to parse them.

### Error handling

- `cargo` not on PATH → clear error message, build fails
- Cargo compilation failure → surface the compiler errors, build fails
- No `cdylib` artifact found in cargo output → error indicating the crate may not have `crate-type = ["cdylib"]` in its `Cargo.toml`

---

## Clean Provider (`otter_clean`, module `rebar3_otter__clean`)

Runs as a `pre_clean` hook.

1. For each configured crate, remove `priv/native/<name>.so` if it exists.
2. Run `cargo clean --manifest-path <path>/Cargo.toml` to remove the Rust build artifacts.

---

## New Provider (`otter new`, module `rebar3_otter__new`)

`rebar3 otter new --name my_nif`

Scaffolds a minimal NIF crate:

**`native/my_nif/Cargo.toml`:**
```toml
[package]
name = "my_nif"
version = "0.1.0"
edition = "2024"

[lib]
crate-type = ["cdylib"]

[dependencies]
otter = { git = "https://github.com/cubelio/otter.git" }
```

**`native/my_nif/src/lib.rs`:**
```rust
use otter::env::Env;
use otter::types::Atom;

#[otter::nif]
fn hello(env: Env) -> Atom {
    Atom::new(env, "world").unwrap()
}

otter::init!("my_nif", [hello]);
```

**Note:** The scaffolded Erlang module and `-on_load` declaration are intentionally not generated. NIF loading is two lines of standard Erlang that the programmer should write and understand:

```erlang
-on_load(init/0).
init() -> erlang:load_nif(filename:join(code:priv_dir(my_app), "native/my_nif"), 0).
```

---

## NIF Loading (user's responsibility)

The plugin does not generate or modify Erlang source files. The user writes their own NIF loading boilerplate:

```erlang
-module(my_module).
-on_load(init/0).

init() ->
    erlang:load_nif(filename:join(code:priv_dir(my_app), "native/my_nif"), 0).

%% Stub replaced at load time by the NIF implementation
my_function(_Arg) -> exit(nif_not_loaded).
```

This is standard Erlang. Every Erlang programmer who has written a NIF before will recognise it immediately.

---

## Module Structure

```
rebar3_otter/src/
├── rebar3_otter.erl           % plugin entry point, registers providers
├── rebar3_otter__compile.erl  % pre_compile provider (otter_compile)
├── rebar3_otter__clean.erl    % pre_clean provider (otter_clean)
├── rebar3_otter__new.erl      % scaffold provider (otter new)
├── rebar3_otter__cargo.erl    % cargo invocation and JSON output parsing
└── rebar3_otter__config.erl   % otter_crates schema validation
```

The double-underscore convention is a local stylistic choice so the underscore-separated namespace inside `rebar3_otter` is unambiguous against the rebar3 plugin name itself.

---

## Dependency tracking

The compile provider invokes `cargo` on every run; cargo's own incremental check decides whether real work needs to happen, and no-ops cost ~50–200ms. This is intentionally simple — cargo already tracks every input that affects a build (sources, features, lockfile, target, environment) and the plugin would only re-implement it badly. The plugin's responsibility is "invoke cargo, then if cargo succeeded, install the artifact." Nothing more.
