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
{plugins, [rebar3_otter]}.

{otter_crates, [
    #{
        name    => my_crate,          % must match Cargo.toml [package].name
        path    => "native/my_crate", % path to crate relative to the app dir
        mode    => release,           % release | debug (default: release)
        features => [],               % list of Cargo features to enable
        target  => undefined          % cross-compile target or undefined
    }
]}.
```

Multiple crates are supported — each entry in `otter_crates` is compiled independently.

`otter_crates` is read **per application**. Declare it in each app's own
`rebar.config`; `path` is resolved relative to that app's directory and the
built artifact is installed into the same app's `priv/native/`. In a single-app
project the app dir is the project root, so nothing special is needed. In an
umbrella project each app declares the crates it owns, and the `.so` lands where
`code:priv_dir(App)` for that app resolves — a top-level `otter_crates` in an
umbrella with no root app is not attached to any application and is ignored.

---

## Compile Provider (`otter_compile`, module `rebar3_otter__compile`)

Runs as a `pre_compile` hook so the `.so` is in place before the Erlang compiler runs (which may check for NIF existence).

### Steps

1. **Read and validate config** — iterate `rebar_state:project_apps/1`; for each app read its own `otter_crates` (`rebar_app_info:get/3`) and pass it through `rebar3_otter__config:validate/1`, which checks required fields (`name`, `path`), normalizes optional fields (`mode`, `features`, `target`), rejects unknown keys, and produces a list of normalized crate maps. The app's directory (`rebar_app_info:dir/1`) is the base for both the crate `path` and the install location. Validation errors halt the build with a formatted message via `rebar_api:abort/2` (the rebar3 pre-hook layer mangles `{error, _}` return values, so config errors take the abort path instead).

2. **Invoke cargo:**
   ```
   cargo build \
     --manifest-path <path>/Cargo.toml \
     --target-dir <path>/target \
     -p <name> \
     [--release] \
     [--features feat1,feat2] \
     [--target <triple>]
   ```
   Cargo runs with `ERTS_INCLUDE_DIR` set to the running ERTS's include dir (`<root>/erts-<vsn>/include`), so the native build (e.g. a `bindgen`/`cc` step, or `enif-ffi`) can locate `erl_nif.h` without the user configuring a path. Plain `cargo build` (the default *human* message format) renders compiler diagnostics to stderr; `run/2` lets the child's stderr through to the terminal, so errors and warnings appear in the rebar3 output directly. `--target-dir` is pinned to `<crate>/target` so the output location is dictated rather than discovered (see step 3). Cargo is invoked unconditionally — its own incremental check decides whether real work needs to happen, and no-ops cost ~50–200ms.

3. **Compute artifact path (by convention)** — because the target dir is pinned and cdylib final artifacts are *not* content-hashed, the output path is fully determined by the inputs: `<target_dir>/[<triple>/]<release|debug>/<file>`, where `<file>` is `lib<name>.so` (Linux), `lib<name>.dylib` (macOS), or `<name>.dll` (Windows), with `<name>` normalized `-`→`_` as cargo does for lib targets. The `lib` prefix / extension follow the *target* platform — derived from the `--target` triple when set (so cross-compiles resolve), otherwise the build host (`os:type/0`). This deliberately avoids parsing cargo's JSON output, which would pull in the OTP-27-only stdlib `json` module; pinning `--target-dir` is what makes the path a guarantee instead of a guess (it removes the workspace / custom-`target-dir` ambiguity the JSON scrape previously absorbed). The computed path is confirmed to exist (`filelib:is_file/1`); a miss yields the `{no_cdylib, _}` error below.

4. **Determine output filename** — the *destination* uses the platform-appropriate extension Erlang expects:
   - Linux: `<name>.so`
   - macOS: `<name>.so` (not `.dylib` — Erlang expects `.so` regardless)
   - Windows: `<name>.dll`

5. **Copy artifact** to the owning app's `priv/native/<name>.so`. Create `priv/native/` if it does not exist.

6. **Surface diagnostics** — cargo emits compiler errors and warnings on stderr (inherited from the child process), so they appear in the rebar3 build output directly without us needing to parse them.

### Error handling

- `cargo` not on PATH → clear error message, build fails
- Cargo compilation failure → surface the compiler errors, build fails
- No `cdylib` artifact found at the computed path → error indicating the crate may not have `crate-type = ["cdylib"]` in its `Cargo.toml`
- Artifact copy into `priv/native/` failed → error with the underlying file reason (`copy_failed`)

---

## Clean Provider (`otter_clean`, module `rebar3_otter__clean`)

Runs as a `pre_clean` hook.

1. For each project app's configured crates, remove the app's `priv/native/<name>.so` if it exists.
2. Remove the crate's pinned target directory (`<crate>/target`) directly. Since the build dictates that directory via `--target-dir`, cleaning is an exact `file:del_dir_r/1` — no cargo invocation, so it works even without a toolchain installed and cannot over-clean a shared workspace target dir.

---

## New Provider (`otter new`, module `rebar3_otter__new`)

`rebar3 otter new --name my_nif`

Scaffolds a minimal NIF crate:

**`native/my_nif/Cargo.toml`:**
```toml
[package]
name = "my_nif"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]
otter-nif = "0.3"
```

**`native/my_nif/src/lib.rs`:**
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

otter::init!("my_nif", [hello], atoms = [world], load = on_load);
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

This is standard Erlang. Every Erlang programmer who has written a NIF before will recognize it immediately.

---

## Module Structure

```
rebar3_otter/src/
├── rebar3_otter.erl           % plugin entry point, registers providers
├── rebar3_otter__compile.erl  % pre_compile provider (otter_compile)
├── rebar3_otter__clean.erl    % pre_clean provider (otter_clean)
├── rebar3_otter__new.erl      % scaffold provider (otter new)
├── rebar3_otter__cargo.erl    % cargo invocation and cdylib artifact resolution
└── rebar3_otter__config.erl   % otter_crates schema validation
```

The double-underscore convention is a local stylistic choice so the underscore-separated namespace inside `rebar3_otter` is unambiguous against the rebar3 plugin name itself.

---

## Dependency tracking

The compile provider invokes `cargo` on every run; cargo's own incremental check decides whether real work needs to happen, and no-ops cost ~50–200ms. This is intentionally simple — cargo already tracks every input that affects a build (sources, features, lockfile, target, environment) and the plugin would only re-implement it badly. The plugin's responsibility is "invoke cargo, then if cargo succeeded, install the artifact." Nothing more.
