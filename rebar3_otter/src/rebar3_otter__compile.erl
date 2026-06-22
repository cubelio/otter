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
  %% Each project app declares its own otter_crates and owns the priv/native/
  %% directory the artifact is installed into, so the NIF lands where
  %% code:priv_dir/1 for that app resolves (correct in umbrella layouts too).
  Apps = rebar_state:project_apps(State),
  compile_apps(Apps, State).

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

-spec compile_apps([rebar_app_info:t()], rebar_state:t()) ->
  {ok, rebar_state:t()}.
compile_apps([], State) ->
  {ok, State};
compile_apps([App | Rest], State) ->
  Raw = rebar_app_info:get(App, otter_crates, []),
  case rebar3_otter__config:validate(Raw) of
    {ok, []} ->
      compile_apps(Rest, State);
    {ok, Crates} ->
      AppDir = rebar_app_info:dir(App),
      case compile_crates(Crates, AppDir, State) of
        {ok, State1} ->
          compile_apps(Rest, State1);
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

-spec compile_crates([rebar3_otter__config:crate()], string(), rebar_state:t()) ->
  {ok, rebar_state:t()} | {error, {module(), term()}}.
compile_crates([], _AppDir, State) ->
  {ok, State};
compile_crates([Crate | Rest], AppDir, State) ->
  case compile_crate(Crate, AppDir) of
    ok              -> compile_crates(Rest, AppDir, State);
    {error, Reason} -> {error, {?MODULE, Reason}}
  end.

-spec compile_crate(rebar3_otter__config:crate(), string()) -> ok | {error, term()}.
compile_crate(#{name := Name, path := Path, mode := Mode,
                features := Features, target := Target}, AppDir) ->
  CratePath = filename:join(AppDir, Path),
  OutDir = filename:join([AppDir, "priv", "native"]),
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
