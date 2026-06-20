-module(rebar3_otter__new_test).
-moduledoc """
Integration test for the `otter new` scaffold (`rebar3_otter__new`).

Generates a crate with the real template, repoints its `otter` dependency at
the in-tree otter crate, and runs `cargo build`. This pins that the
scaffolded code actually compiles against the *current* otter API — the
template silently drifted out of date once already (issue audit-18), so a
shape check on the emitted text is not enough; it must compile.

Prerequisites: `cargo` on `PATH` and the in-tree otter crate. The otter
location is taken from the `OTTER_PATH` environment variable, falling back to
`../otter` relative to the plugin directory. The dependency is rewritten to a
path dep on purpose: the published git source lags the working tree, so
building against it would not catch API drift.
""".

-include_lib("eunit/include/eunit.hrl").

%%------------------------------------------------------------------------------

-spec scaffold_compiles_test_() -> {timeout, pos_integer(), fun(() -> _)}.
scaffold_compiles_test_() ->
  %% cargo's first build pulls the crates.io index and compiles otter plus
  %% its proc-macro deps from scratch; give it generous headroom.
  {timeout, 300, fun scaffold_compiles/0}.

-spec scaffold_compiles() -> _.
scaffold_compiles() ->
  Cargo = require_cargo(),
  OtterPath = require_otter_path(),
  TmpDir = tmp_dir(),
  CrateDir = filename:join(TmpDir, "my_nif"),
  try
    ok = rebar3_otter__new:scaffold(CrateDir, "my_nif"),
    %% Repoint the otter dep from the published git source to the in-tree
    %% crate so we validate against the current API.
    ok = write_path_cargo_toml(CrateDir, OtterPath),
    case cargo_build(Cargo, CrateDir) of
      {_Output, 0} ->
        ok;
      {Output, Code} ->
        erlang:error({cargo_build_failed, Code, binary_to_list(Output)})
    end
  after
    _ = file:del_dir_r(TmpDir)
  end.

%%------------------------------------------------------------------------------
%% Helpers

-spec require_cargo() -> string().
require_cargo() ->
  case os:find_executable("cargo") of
    false -> erlang:error("cargo not found on PATH; required to build the otter new scaffold");
    Path  -> Path
  end.

-spec require_otter_path() -> string().
require_otter_path() ->
  Path = case os:getenv("OTTER_PATH") of
           false -> filename:absname("../otter");
           Env   -> filename:absname(Env)
         end,
  case filelib:is_file(filename:join(Path, "Cargo.toml")) of
    true  -> Path;
    false -> erlang:error({otter_crate_not_found, Path,
                           "set OTTER_PATH to the in-tree otter crate directory"})
  end.

-spec write_path_cargo_toml(string(), string()) -> ok.
write_path_cargo_toml(CrateDir, OtterPath) ->
  Toml = io_lib:format(
    "[package]\n"
    "name = \"my_nif\"\n"
    "version = \"0.1.0\"\n"
    "edition = \"2024\"\n"
    "\n"
    "[workspace]\n"
    "\n"
    "[lib]\n"
    "crate-type = [\"cdylib\"]\n"
    "\n"
    "[dependencies]\n"
    "otter = { path = \"~s\" }\n",
    [OtterPath]),
  file:write_file(filename:join(CrateDir, "Cargo.toml"), Toml).

-spec cargo_build(string(), string()) -> {binary(), integer()}.
cargo_build(Cargo, CrateDir) ->
  Manifest = filename:join(CrateDir, "Cargo.toml"),
  Port = open_port({spawn_executable, Cargo},
                   [exit_status, stderr_to_stdout, binary,
                    {args, ["build", "--manifest-path", Manifest]}]),
  collect_port(Port, <<>>).

-spec collect_port(port(), binary()) -> {binary(), integer()}.
collect_port(Port, Acc) ->
  receive
    {Port, {data, Bin}}          -> collect_port(Port, <<Acc/binary, Bin/binary>>);
    {Port, {exit_status, Code}}  -> {Acc, Code}
  end.

-spec tmp_dir() -> string().
tmp_dir() ->
  Root = case os:getenv("TMPDIR") of false -> "/tmp"; T -> T end,
  Name = "otter_new_test_" ++ integer_to_list(erlang:unique_integer([positive])),
  Dir = filename:join(Root, Name),
  ok = filelib:ensure_dir(filename:join(Dir, ".")),
  Dir.
