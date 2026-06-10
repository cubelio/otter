-module(rebar3_otter__compile).
-moduledoc """
Provider that compiles Rust NIF crates via cargo.

Runs as a pre-compile hook so the shared library is in place
before the Erlang compiler runs.
""".

-behaviour(provider).

-export([init/1, do/1, format_error/1]).

%%------------------------------------------------------------------------------

-define(PROVIDER, otter_compile).
-define(DEPS, [{default, app_discovery}]).


%%%=============================================================================
%%% Callbacks

-spec init(rebar_state:t()) -> {ok, rebar_state:t()}.
init(State) ->
  Provider = providers:create([
    {name, ?PROVIDER},
    {module, ?MODULE},
    {namespace, default},
    {bare, true},
    {deps, ?DEPS},
    {short_desc, "Compile Rust NIF crates"},
    {desc, "Compiles all Rust NIF crates listed in otter_crates"}
  ]),
  {ok, rebar_state:add_provider(State, Provider)}.

-spec do(rebar_state:t()) -> {ok, rebar_state:t()} | {error, {module(), term()}}.
do(State) ->
  case rebar3_otter__config:validate(State) of
    {ok, Crates} ->
      BaseDir = rebar_state:dir(State),
      case compile_crates(Crates, BaseDir, State) of
        {ok, _} = Ok ->
          Ok;
        {error, {?MODULE, Reason}} ->
          %% Same pre-hook quirk as the config-error branch below.
          rebar_api:abort("~s", [format_error(Reason)])
      end;
    {error, Reason} ->
      %% rebar3's pre-hook machinery rewrites any {error, _} from our do/1
      %% into a misleading "command not found in namespace" message
      %% (rebar_hooks.erl:70-73), so config errors have to halt the build
      %% directly with the formatted message instead of riding the
      %% format_error/1 path that top-level providers use.
      rebar_api:abort("~s", [rebar3_otter__config:format_error(Reason)])
  end.

-spec format_error(term()) -> string() | iolist().
format_error(cargo_not_found) ->
  "cargo not found on PATH. Install the Rust toolchain: https://rustup.rs";
format_error({cargo_failed, Name, Code}) ->
  io_lib:format("cargo build failed for crate '~s' (exit code ~p)", [Name, Code]);
format_error({no_cdylib, Name}) ->
  io_lib:format("no cdylib artifact found for crate '~s'. "
                "Ensure crate-type = [\"cdylib\"] is set in Cargo.toml", [Name]);
format_error({copy_failed, Name, Reason}) ->
  io_lib:format("failed to copy artifact for '~s': ~p", [Name, Reason]);
format_error(Other) ->
  io_lib:format("~p", [Other]).


%%%=============================================================================
%%% Private

-spec compile_crates([rebar3_otter__config:crate()], string(), rebar_state:t()) ->
  {ok, rebar_state:t()} | {error, {module(), term()}}.
compile_crates([], _BaseDir, State) ->
  {ok, State};
compile_crates([Crate | Rest], BaseDir, State) ->
  case compile_crate(Crate, BaseDir) of
    ok              -> compile_crates(Rest, BaseDir, State);
    {error, Reason} -> {error, {?MODULE, Reason}}
  end.

-spec compile_crate(rebar3_otter__config:crate(), string()) -> ok | {error, term()}.
compile_crate(#{name := Name, path := Path, mode := Mode,
                features := Features, target := Target}, BaseDir) ->
  CratePath = filename:join(BaseDir, Path),
  OutDir = filename:join([BaseDir, "priv", "native"]),
  OutFile = filename:join(OutDir, rebar3_otter__cargo:nif_filename(Name)),
  rebar_api:info("Compiling Rust crate ~s", [Name]),
  case rebar3_otter__cargo:build(CratePath, Name, Mode, Features, Target) of
    {ok, ArtifactPath} ->
      install_artifact(ArtifactPath, OutDir, OutFile, Name);
    {error, _} = Err ->
      Err
  end.

-spec install_artifact(string(), string(), string(), string()) -> ok | {error, term()}.
install_artifact(ArtifactPath, OutDir, OutFile, Name) ->
  ok = filelib:ensure_dir(filename:join(OutDir, ".")),
  case file:copy(ArtifactPath, OutFile) of
    {ok, _} ->
      rebar_api:info("Installed ~s", [OutFile]),
      ok;
    {error, Reason} ->
      {error, {copy_failed, Name, Reason}}
  end.
