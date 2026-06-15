# otter_demo

A minimal Erlang application that exercises the otter NIF library. It serves as both an end-to-end smoke test for otter and a usage reference for callers.

- Erlang module: `otter_demo__nif` ([src/otter_demo__nif.erl](src/otter_demo__nif.erl))
- Rust NIF crate: [native/otter_demo/](native/otter_demo/)
- Built via `rebar3 compile` (using the `rebar3_otter` plugin)

## Relationship to otter

This sits inside the otter repo at `test_apps/otter_demo/` and depends on the otter crate at `../../../otter` (see `native/otter_demo/Cargo.toml`). The `rebar3_otter` plugin is picked up via `_checkouts/rebar3_otter -> ../../../rebar3_otter`. The demo is excluded from otter's cargo workspace and declares its own with an empty `[workspace]` table.

## What the demo covers

The NIF crate demonstrates: pre-declared atoms (`declare_atoms!` / `init_atoms!` / `atom!`), typed arguments and returns, term passthrough, type inspection, binary construction, list iteration, equality/ordering, `Debug` formatting, `TryFrom`, resource types with destructors, and the `on_load` callback.

## Running

```
rebar3 compile          # builds Rust + Erlang
rebar3 eunit            # runs the smoke tests (test/otter_demo__nif_test.erl)
rebar3 shell            # interactive — call otter_demo__nif:hello() etc.
```

`rebar3 eunit` runs the tests in `otter_demo__nif_test`. The bulk live in `smoke_test_/0`, a test generator that returns one assertion per NIF; each assertion runs as its own EUnit test, so a single failure does not mask the others. Alongside it are standalone tests for the resource callbacks and upgrade path (`select_stop_test`, `select_x_test`, `monitor_down_test`, `port_command_test`, `upgrade_reload_test`).

If the eunit cache gets confused after editing the NIF, `rm -rf _build/test` followed by `rebar3 eunit` clears it.
