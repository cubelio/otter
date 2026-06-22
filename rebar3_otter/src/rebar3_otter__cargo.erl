-module(rebar3_otter__cargo).
-moduledoc """
Cargo invocation and cdylib artifact resolution.

This module handles all interaction with the Rust toolchain.
It is independent of the rebar3 provider API.

The build pins cargo's `--target-dir` to `<crate>/target` and computes the
output cdylib path by convention rather than parsing cargo's JSON output.
cdylib final artifacts are not content-hashed, so the name is deterministic
(`lib<name>.so` / `.dylib`, `<name>.dll`), and pinning the target dir makes
its location a guarantee instead of a guess. This also keeps artifact
resolution free of the stdlib `json` module (no JSON to parse).
""".

-export([build/5, clean/1, nif_filename/1]).

-ifdef(TEST).
%% Exposed for unit tests of the cross-platform / cross-target path logic.
-export([artifact_path/4, artifact_filename/2]).
-endif.

%%------------------------------------------------------------------------------

%%%=============================================================================
%%% Public

-spec build(string(), string(), release | debug, [string()], string() | undefined) ->
  {ok, string()} | {error, term()}.
build(CratePath, Name, Mode, Features, Target) ->
  case find_cargo() of
    {error, _} = Err ->
      Err;
    {ok, Cargo} ->
      ManifestPath = filename:join(CratePath, "Cargo.toml"),
      TargetDir = target_dir(CratePath),
      Args = build_args(ManifestPath, Name, Mode, Features, Target, TargetDir),
      case run(Cargo, Args) of
        {0, _Output} ->
          %% We pinned --target-dir, so the cdylib path is fully determined
          %% by the inputs — no need to scrape cargo's output for it.
          Artifact = artifact_path(TargetDir, Name, Mode, Target),
          case filelib:is_file(Artifact) of
            true  -> {ok, Artifact};
            false -> {error, {no_cdylib, Name}}
          end;
        {Code, _Output} ->
          {error, {cargo_failed, Name, Code}}
      end
  end.

-doc """
Remove the crate's build output.

The build pins `--target-dir` to `<crate>/target`, so cleaning is just
removing that directory — exact in scope and independent of cargo (so it
works even without a toolchain installed).
""".
-spec clean(string()) -> ok.
clean(CratePath) ->
  _ = file:del_dir_r(target_dir(CratePath)),
  ok.

%% The directory cargo is told to write into (`--target-dir`) and that the
%% artifact path is computed against. Pinning it removes the workspace
%% target-dir ambiguity that would otherwise make the output location a guess.
-spec target_dir(string()) -> string().
target_dir(CratePath) ->
  filename:join(CratePath, "target").

%%%=============================================================================
%%% Private

%%------------------------------------------------------------------------------
%% Cargo invocation

-spec find_cargo() -> {ok, string()} | {error, cargo_not_found}.
find_cargo() ->
  case os:find_executable("cargo") of
    false -> {error, cargo_not_found};
    Path  -> {ok, Path}
  end.

-spec build_args(string(), string(), release | debug, [string()], string() | undefined, string()) ->
  [string()].
build_args(ManifestPath, Name, Mode, Features, Target, TargetDir) ->
  %% Plain `cargo build` (human message format): compiler diagnostics render
  %% to stderr, which `run/2` lets through to the terminal. `--target-dir`
  %% pins the output location so `artifact_path/4` can compute the cdylib path
  %% without parsing JSON (which would pull in the OTP-27-only `json` module).
  Base = ["build",
          "--manifest-path", ManifestPath,
          "--target-dir", TargetDir,
          "-p", Name],
  ModeArgs = case Mode of
    release -> ["--release"];
    debug   -> []
  end,
  FeatureArgs = case Features of
    [] -> [];
    _  -> ["--features", string:join(Features, ",")]
  end,
  TargetArgs = case Target of
    undefined -> [];
    T         -> ["--target", T]
  end,
  Base ++ ModeArgs ++ FeatureArgs ++ TargetArgs.

%% Run cargo and capture stdout. Stderr is inherited by the child
%% process, so compiler diagnostics appear directly in the terminal.
-spec run(string(), [string()]) -> {non_neg_integer(), binary()}.
run(Cargo, Args) ->
  Env = [{"ERTS_INCLUDE_DIR", erts_include_dir()}],
  Port = open_port({spawn_executable, Cargo}, [
    {args, Args},
    {env, Env},
    binary,
    exit_status
  ]),
  collect(Port, []).

-spec collect(port(), [binary()]) -> {non_neg_integer(), binary()}.
collect(Port, Acc) ->
  receive
    {Port, {data, Data}} ->
      collect(Port, [Data | Acc]);
    {Port, {exit_status, Status}} ->
      {Status, iolist_to_binary(lists:reverse(Acc))}
  end.

-spec erts_include_dir() -> string().
erts_include_dir() ->
  filename:join([code:root_dir(),
                 "erts-" ++ erlang:system_info(version),
                 "include"]).

%%------------------------------------------------------------------------------
%% Artifact path (by convention)

%% Absolute path of the cdylib cargo writes for crate Name, given the pinned
%% target dir, the build mode, and the optional --target triple. cargo lays
%% artifacts out as `<target_dir>/[<triple>/]<release|debug>/<file>` and the
%% cdylib final name is not content-hashed, so this is exact.
-spec artifact_path(string(), string(), release | debug, string() | undefined) ->
  string().
artifact_path(TargetDir, Name, Mode, Target) ->
  ProfileDir = case Mode of release -> "release"; debug -> "debug" end,
  File = artifact_filename(normalize_crate_name(Name), Target),
  Parts = case Target of
            undefined -> [TargetDir, ProfileDir, File];
            _         -> [TargetDir, Target, ProfileDir, File]
          end,
  filename:join(Parts).

%% The cdylib filename for the already-normalized crate name. The library
%% prefix/extension follow the *target* platform: derived from the --target
%% triple when one is set (so cross-compiles resolve correctly), otherwise
%% from the build host. Note this is the cargo *source* name (`.dylib` on
%% macOS); `nif_filename/1` gives the `.so` *destination* Erlang expects.
-spec artifact_filename(string(), string() | undefined) -> string().
artifact_filename(Norm, undefined) ->
  case os:type() of
    {win32, _}     -> Norm ++ ".dll";
    {unix, darwin} -> "lib" ++ Norm ++ ".dylib";
    {unix, _}      -> "lib" ++ Norm ++ ".so"
  end;
artifact_filename(Norm, Target) ->
  case classify_triple(Target) of
    windows -> Norm ++ ".dll";
    darwin  -> "lib" ++ Norm ++ ".dylib";
    other   -> "lib" ++ Norm ++ ".so"
  end.

-spec classify_triple(string()) -> windows | darwin | other.
classify_triple(Triple) ->
  IsWindows = string:find(Triple, "windows") =/= nomatch,
  IsDarwin  = (string:find(Triple, "darwin") =/= nomatch)
              orelse (string:find(Triple, "apple") =/= nomatch),
  if
    IsWindows -> windows;
    IsDarwin  -> darwin;
    true      -> other
  end.

%% cargo replaces '-' with '_' in lib target names, so a package named
%% `my-nif` produces `libmy_nif.so`. Normalize the configured name to match.
-spec normalize_crate_name(string()) -> string().
normalize_crate_name(Name) ->
  lists:flatten(string:replace(Name, "-", "_", all)).

%%------------------------------------------------------------------------------
%% Helpers

-doc """
Platform-appropriate filename for a NIF shared library.

`.dll` on Windows, `.so` everywhere else (including macOS, where Erlang
expects `.so` rather than `.dylib`).
""".
-spec nif_filename(string()) -> string().
nif_filename(Name) ->
  case os:type() of
    {win32, _} -> Name ++ ".dll";
    _          -> Name ++ ".so"
  end.
