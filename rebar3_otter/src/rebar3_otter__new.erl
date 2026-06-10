-module(rebar3_otter__new).
-moduledoc """
Provider that scaffolds a new Rust NIF crate.

Usage: `rebar3 otter new --name my_nif`

Creates `native/<name>/Cargo.toml` and `native/<name>/src/lib.rs`
with a minimal working NIF.
""".

-behaviour(provider).

-export([init/1, do/1, format_error/1]).

%%------------------------------------------------------------------------------

-define(PROVIDER, new).
-define(NAMESPACE, otter).
-define(DEPS, []).


%%%=============================================================================
%%% Callbacks

-spec init(rebar_state:t()) -> {ok, rebar_state:t()}.
init(State) ->
  Provider = providers:create([
    {name, ?PROVIDER},
    {module, ?MODULE},
    {namespace, ?NAMESPACE},
    {bare, true},
    {deps, ?DEPS},
    {example, "rebar3 otter new --name my_nif"},
    {short_desc, "Scaffold a new Rust NIF crate"},
    {desc, "Creates a Cargo.toml and src/lib.rs under native/<name>/"},
    {opts, [{name, $n, "name", string, "Name of the NIF crate"}]}
  ]),
  {ok, rebar_state:add_provider(State, Provider)}.

-spec do(rebar_state:t()) -> {ok, rebar_state:t()} | {error, {module(), term()}}.
do(State) ->
  {ParsedArgs, _} = rebar_state:command_parsed_args(State),
  case proplists:get_value(name, ParsedArgs) of
    undefined ->
      {error, {?MODULE, no_name}};
    Name ->
      BaseDir = rebar_state:dir(State),
      CrateDir = filename:join([BaseDir, "native", Name]),
      case filelib:is_dir(CrateDir) of
        true ->
          {error, {?MODULE, {already_exists, Name}}};
        false ->
          scaffold(CrateDir, Name),
          rebar_api:info("Created NIF crate at native/~s", [Name]),
          rebar_api:info(
            "Add to rebar.config:~n~n"
            "  {otter_crates, [~n"
            "      #{name => ~s, path => \"native/~s\"}~n"
            "  ]}.~n~n"
            "  {provider_hooks, [~n"
            "      {pre, [{compile, otter_compile}, {clean, otter_clean}]}~n"
            "  ]}.~n",
            [Name, Name]),
          {ok, State}
      end
  end.

-spec format_error(term()) -> string() | iolist().
format_error(no_name) ->
  "missing required --name argument";
format_error({already_exists, Name}) ->
  io_lib:format("directory native/~s already exists", [Name]);
format_error(Other) ->
  io_lib:format("~p", [Other]).


%%%=============================================================================
%%% Private

-spec scaffold(string(), string()) -> ok.
scaffold(CrateDir, Name) ->
  SrcDir = filename:join(CrateDir, "src"),
  ok = filelib:ensure_dir(filename:join(SrcDir, ".")),
  ok = file:write_file(filename:join(CrateDir, "Cargo.toml"), cargo_toml(Name)),
  ok = file:write_file(filename:join(SrcDir, "lib.rs"), lib_rs(Name)).

-spec cargo_toml(string()) -> iolist().
cargo_toml(Name) ->
  io_lib:format(
    "[package]\n"
    "name = \"~s\"\n"
    "version = \"0.1.0\"\n"
    "edition = \"2024\"\n"
    "\n"
    "[lib]\n"
    "crate-type = [\"cdylib\"]\n"
    "\n"
    "[dependencies]\n"
    "otter = { git = \"https://github.com/cubelio/otter.git\" }\n",
    [Name]).

-spec lib_rs(string()) -> iolist().
lib_rs(Name) ->
  io_lib:format(
    "use otter::env::Env;\n"
    "use otter::types::Atom;\n"
    "\n"
    "#[otter::nif]\n"
    "fn hello(env: Env) -> Atom {\n"
    "    Atom::new(env, \"world\").unwrap()\n"
    "}\n"
    "\n"
    "otter::init!(\"~s\", [hello]);\n",
    [Name]).
