-module(rebar3_otter).
-moduledoc """
rebar3 plugin for building Rust NIF crates with otter.

Registers the compile, clean, and new providers.
""".

-export([init/1]).

%%------------------------------------------------------------------------------

-spec init(rebar_state:t()) -> {ok, rebar_state:t()}.
init(State0) ->
  {ok, State1} = rebar3_otter__compile:init(State0),
  {ok, State2} = rebar3_otter__clean:init(State1),
  {ok, State3} = rebar3_otter__new:init(State2),
  {ok, State3}.
