-module(rebar3_otter__clean).
-moduledoc """
Provider that cleans Rust NIF build artifacts.

Removes the installed shared libraries from `priv/native/` and
runs `cargo clean` for each configured crate.
""".

-behaviour(provider).

-export([init/1, do/1, format_error/1]).

%%------------------------------------------------------------------------------

-define(PROVIDER, otter_clean).
-define(DEPS, []).


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
    {short_desc, "Clean Rust NIF build artifacts"},
    {desc, "Removes compiled NIF shared libraries and runs cargo clean"}
  ]),
  {ok, rebar_state:add_provider(State, Provider)}.

-spec do(rebar_state:t()) -> {ok, rebar_state:t()}.
do(State) ->
  case rebar3_otter__config:validate(State) of
    {ok, Crates} ->
      BaseDir = rebar_state:dir(State),
      lists:foreach(fun(Crate) -> clean_crate(Crate, BaseDir) end, Crates),
      {ok, State};
    {error, Reason} ->
      %% Same pre-hook quirk as in rebar3_otter__compile: halt directly.
      rebar_api:abort("~s", [rebar3_otter__config:format_error(Reason)])
  end.

-spec format_error(term()) -> string() | iolist().
format_error(Other) ->
  io_lib:format("~p", [Other]).


%%%=============================================================================
%%% Private

-spec clean_crate(rebar3_otter__config:crate(), string()) -> ok.
clean_crate(#{name := Name, path := Path}, BaseDir) ->
  CratePath = filename:join(BaseDir, Path),
  OutFile = filename:join([BaseDir, "priv", "native",
                           rebar3_otter__cargo:nif_filename(Name)]),
  case filelib:is_regular(OutFile) of
    true ->
      rebar_api:info("Removing ~s", [OutFile]),
      _ = file:delete(OutFile);
    false ->
      ok
  end,
  rebar3_otter__cargo:clean(CratePath).
