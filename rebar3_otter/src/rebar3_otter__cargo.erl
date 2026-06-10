-module(rebar3_otter__cargo).
-moduledoc """
Cargo invocation, JSON output parsing, and change detection.

This module handles all interaction with the Rust toolchain.
It is independent of the rebar3 provider API.
""".

-export([build/5, clean/1, nif_filename/1]).

%%------------------------------------------------------------------------------

%%%=============================================================================
%%% Public

-spec build(string(), string(), release | debug, [atom() | string()], atom() | string() | undefined) ->
  {ok, string()} | {error, term()}.
build(CratePath, Name, Mode, Features, Target) ->
  case find_cargo() of
    {error, _} = Err ->
      Err;
    {ok, Cargo} ->
      ManifestPath = filename:join(CratePath, "Cargo.toml"),
      Args = build_args(ManifestPath, Name, Mode, Features, Target),
      case run(Cargo, Args) of
        {0, Output} ->
          case find_cdylib(Output) of
            {ok, _} = Ok -> Ok;
            error -> {error, {no_cdylib, Name}}
          end;
        {Code, _Output} ->
          {error, {cargo_failed, Name, Code}}
      end
  end.

-doc """
Run `cargo clean` for the given crate.
""".
-spec clean(string()) -> ok.
clean(CratePath) ->
  case find_cargo() of
    {error, _} ->
      ok;
    {ok, Cargo} ->
      ManifestPath = filename:join(CratePath, "Cargo.toml"),
      _ = run(Cargo, ["clean", "--manifest-path", ManifestPath]),
      ok
  end.

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

-spec build_args(string(), string(), release | debug, [atom() | string()], atom() | string() | undefined) ->
  [string()].
build_args(ManifestPath, Name, Mode, Features, Target) ->
  Base = ["rustc",
          "--message-format=json-render-diagnostics",
          "--manifest-path", ManifestPath,
          "-p", Name],
  ModeArgs = case Mode of
    release -> ["--release"];
    debug   -> []
  end,
  FeatureArgs = case Features of
    [] -> [];
    _  -> ["--features", string:join([to_str(F) || F <- Features], ",")]
  end,
  TargetArgs = case Target of
    undefined -> [];
    T         -> ["--target", to_str(T)]
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
%% Artifact detection

%% Scan cargo's JSON stdout for a compiler-artifact message whose
%% target kind list contains "cdylib". Return the first filename.
-spec find_cdylib(binary()) -> {ok, string()} | error.
find_cdylib(Output) ->
  Lines = binary:split(Output, <<"\n">>, [global, trim_all]),
  find_cdylib_line(Lines).

-spec find_cdylib_line([binary()]) -> {ok, string()} | error.
find_cdylib_line([]) ->
  error;
find_cdylib_line([Line | Rest]) ->
  try json:decode(Line) of
    #{<<"reason">> := <<"compiler-artifact">>,
      <<"target">> := #{<<"kind">> := Kinds},
      <<"filenames">> := [Path | _]} when is_list(Kinds) ->
      case lists:member(<<"cdylib">>, Kinds) of
        true  -> {ok, binary_to_list(Path)};
        false -> find_cdylib_line(Rest)
      end;
    _ ->
      find_cdylib_line(Rest)
  catch
    _:_ ->
      find_cdylib_line(Rest)
  end.

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

-spec to_str(atom() | string() | binary()) -> string().
to_str(V) when is_atom(V)   -> atom_to_list(V);
to_str(V) when is_list(V)   -> V;
to_str(V) when is_binary(V) -> binary_to_list(V).
