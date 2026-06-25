# otter_test

An Erlang application that exercises the otter NIF library end-to-end. It is the
project's integration test suite: a Rust NIF crate covering the full otter
surface, driven from EUnit under a live BEAM.

- Erlang module: `otter_test__nif` ([src/otter_test__nif.erl](src/otter_test__nif.erl))
- Rust NIF crate: [native/otter_test/](native/otter_test/)
- Built via `rebar3 compile` (using the `rebar3_otter` plugin)

## Relationship to otter

This sits at the otter repo top level (`otter_test/`) and depends on the otter
crate at `../../../otter` (see `native/otter_test/Cargo.toml`). The
`rebar3_otter` plugin is picked up via `_checkouts/rebar3_otter -> ../../rebar3_otter`.
The suite is excluded from otter's cargo workspace and declares its own with an
empty `[workspace]` table. It builds with `features = ["bigint"]`.

## What the suite covers

The NIF crate exercises: pre-declared atoms (`init!`'s `atoms = [...]` + `atom!`), typed arguments and returns, term passthrough, type inspection, binary construction, list iteration, equality/ordering, `Debug` formatting, integer/float/string extraction, native-type codecs (ints, `String`, `Vec`, `HashMap`), bignums (`bigint`), resource types with destructors / monitors / `select` stop callbacks, message passing across the full send matrix (`OwnedEnvArena`, off-thread `send_move`, in-NIF `send_copy_from` / `send_move_from`), the encoder-panic regression, and the `on_load` / upgrade callbacks.

## Running

```
rebar3 compile          # builds Rust + Erlang
rebar3 eunit            # runs the suite (test/otter_test__nif_test.erl)
rebar3 shell            # interactive — call otter_test__nif:hello() etc.
```

`rebar3 eunit` runs the tests in `otter_test__nif_test`. The bulk live in `smoke_test_/0`, a test generator that returns one assertion per NIF; each assertion runs as its own EUnit test, so a single failure does not mask the others. Alongside it are standalone tests for the resource callbacks, upgrade path, and panic handling (`select_stop_test`, `select_x_test`, `monitor_down_test`, `port_command_test`, `upgrade_reload_test`, `panic_in_encoder_test`).

If the eunit cache gets confused after editing the NIF, `rm -rf _build/test` followed by `rebar3 eunit` clears it.
