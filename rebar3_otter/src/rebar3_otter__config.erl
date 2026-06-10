-module(rebar3_otter__config).
-moduledoc """
Validation and normalization of the `otter_crates` rebar3 config key.

Both the compile and clean providers go through `validate/1` so they
agree on the schema. Errors are tagged tuples that `format_error/1`
renders into human-readable strings.

Schema:

```
{otter_crates, [
    #{
        name     := atom() | string() | binary(),  % required
        path     := string() | binary(),           % required
        mode     => release | debug,               % default release
        features => [atom() | string() | binary()],% default []
        target   => atom() | string() | binary()   % default undefined
    }
]}.
```

Unknown map keys are rejected.
""".

-export([validate/1, format_error/1]).
-export_type([crate/0]).

%%------------------------------------------------------------------------------

-type crate() :: #{
    name     := string(),
    path     := string(),
    mode     := release | debug,
    features := [string()],
    target   := string() | undefined
}.

-define(KNOWN_KEYS, [name, path, mode, features, target]).


%%%=============================================================================
%%% Public

-spec validate(rebar_state:t()) -> {ok, [crate()]} | {error, term()}.
validate(State) ->
  Raw = rebar_state:get(State, otter_crates, []),
  case is_list(Raw) of
    false ->
      {error, {otter_crates_not_a_list, Raw}};
    true ->
      validate_entries(Raw, [])
  end.

-spec format_error(term()) -> string() | iolist().
format_error({otter_crates_not_a_list, Value}) ->
  io_lib:format("otter_crates must be a list, got: ~p", [Value]);
format_error({crate_entry_not_a_map, Value}) ->
  io_lib:format("each otter_crates entry must be a map, got: ~p", [Value]);
format_error({missing_field, Field, Identity}) ->
  io_lib:format("crate ~s is missing required field '~s'",
                [Identity, Field]);
format_error({invalid_field_type, Field, Expected, Value, Identity}) ->
  io_lib:format("crate ~s field '~s' must be ~s, got: ~p",
                [Identity, Field, Expected, Value]);
format_error({invalid_mode, Value, Identity}) ->
  io_lib:format("crate ~s field 'mode' must be 'release' or 'debug', got: ~p",
                [Identity, Value]);
format_error({invalid_feature, Value, Identity}) ->
  io_lib:format("crate ~s field 'features' must be a list of atoms/strings, "
                "got element: ~p", [Identity, Value]);
format_error({unknown_key, Key, Identity}) ->
  io_lib:format("crate ~s has unknown key '~s'", [Identity, Key]);
format_error(Other) ->
  io_lib:format("~p", [Other]).


%%%=============================================================================
%%% Private

-spec validate_entries([term()], [crate()]) -> {ok, [crate()]} | {error, term()}.
validate_entries([], Acc) ->
  {ok, lists:reverse(Acc)};
validate_entries([Entry | Rest], Acc) ->
  case validate_entry(Entry) of
    {ok, Crate}     -> validate_entries(Rest, [Crate | Acc]);
    {error, _} = Err -> Err
  end.

-spec validate_entry(term()) -> {ok, crate()} | {error, term()}.
validate_entry(Entry) when not is_map(Entry) ->
  {error, {crate_entry_not_a_map, Entry}};
validate_entry(Entry) ->
  Identity = identify(Entry),
  case check_unknown_keys(Entry, Identity) of
    {error, _} = Err1 -> Err1;
    ok ->
      case require_field(name, Entry, Identity) of
        {error, _} = Err2 -> Err2;
        ok ->
          case require_field(path, Entry, Identity) of
            {error, _} = Err3 -> Err3;
            ok -> normalize(Entry, Identity)
          end
      end
  end.

-spec check_unknown_keys(map(), iolist()) -> ok | {error, term()}.
check_unknown_keys(Entry, Identity) ->
  Keys = maps:keys(Entry),
  case [K || K <- Keys, not lists:member(K, ?KNOWN_KEYS)] of
    []         -> ok;
    [Bad | _]  -> {error, {unknown_key, Bad, Identity}}
  end.

-spec require_field(atom(), map(), iolist()) -> ok | {error, term()}.
require_field(Field, Entry, Identity) ->
  case maps:is_key(Field, Entry) of
    true  -> ok;
    false -> {error, {missing_field, Field, Identity}}
  end.

%% Validate and normalize each field, accumulating the converted values.
%% First field to fail short-circuits the whole entry.
-spec normalize(map(), iolist()) -> {ok, crate()} | {error, term()}.
normalize(Entry, Identity) ->
  Fields = [
    {name,     fun normalize_name/2,     fun(Entry1) -> maps:get(name, Entry1) end},
    {path,     fun normalize_path/2,     fun(Entry1) -> maps:get(path, Entry1) end},
    {mode,     fun normalize_mode/2,     fun(Entry1) -> maps:get(mode, Entry1, release) end},
    {features, fun normalize_features/2, fun(Entry1) -> maps:get(features, Entry1, []) end},
    {target,   fun normalize_target/2,   fun(Entry1) -> maps:get(target, Entry1, undefined) end}
  ],
  normalize_fields(Fields, Entry, Identity, #{}).

-spec normalize_fields(
        [{atom(), fun(), fun()}], map(), iolist(), map()) ->
  {ok, crate()} | {error, term()}.
normalize_fields([], _Entry, _Identity, Acc) ->
  {ok, Acc};
normalize_fields([{Key, Normalizer, Reader} | Rest], Entry, Identity, Acc) ->
  case Normalizer(Reader(Entry), Identity) of
    {ok, Value}      -> normalize_fields(Rest, Entry, Identity, Acc#{Key => Value});
    {error, _} = Err -> Err
  end.

-spec normalize_name(term(), iolist()) -> {ok, string()} | {error, term()}.
normalize_name(V, _Identity) when is_atom(V); is_list(V); is_binary(V) ->
  {ok, to_str(V)};
normalize_name(V, Identity) ->
  {error, {invalid_field_type, name, "an atom, string, or binary", V, Identity}}.

-spec normalize_path(term(), iolist()) -> {ok, string()} | {error, term()}.
normalize_path(V, _Identity) when is_list(V); is_binary(V) ->
  {ok, to_str(V)};
normalize_path(V, Identity) ->
  {error, {invalid_field_type, path, "a string or binary", V, Identity}}.

-spec normalize_mode(term(), iolist()) -> {ok, release | debug} | {error, term()}.
normalize_mode(release, _Identity) -> {ok, release};
normalize_mode(debug,   _Identity) -> {ok, debug};
normalize_mode(V, Identity)        -> {error, {invalid_mode, V, Identity}}.

-spec normalize_features(term(), iolist()) -> {ok, [string()]} | {error, term()}.
normalize_features(L, Identity) when is_list(L) ->
  case [F || F <- L, not is_feature_elem(F)] of
    []        -> {ok, [to_str(F) || F <- L]};
    [Bad | _] -> {error, {invalid_feature, Bad, Identity}}
  end;
normalize_features(V, Identity) ->
  {error, {invalid_field_type, features, "a list", V, Identity}}.

-spec is_feature_elem(term()) -> boolean().
is_feature_elem(V) -> is_atom(V) orelse is_list(V) orelse is_binary(V).

-spec normalize_target(term(), iolist()) ->
  {ok, string() | undefined} | {error, term()}.
normalize_target(undefined, _Identity) ->
  {ok, undefined};
normalize_target(V, _Identity) when is_atom(V); is_list(V); is_binary(V) ->
  {ok, to_str(V)};
normalize_target(V, Identity) ->
  {error, {invalid_field_type, target, "an atom, string, or binary", V, Identity}}.

%%------------------------------------------------------------------------------
%% Helpers

%% A human-readable identity for an entry, used in error messages. Falls back
%% to the raw entry shape when no name is available, so the message is still
%% specific enough to locate the offending config.
-spec identify(map()) -> iolist().
identify(Entry) ->
  case maps:get(name, Entry, undefined) of
    undefined ->
      io_lib:format("~p", [Entry]);
    Name when is_atom(Name) ->
      io_lib:format("'~s'", [atom_to_list(Name)]);
    Name when is_list(Name) ->
      io_lib:format("'~s'", [Name]);
    Name when is_binary(Name) ->
      io_lib:format("'~s'", [binary_to_list(Name)]);
    Name ->
      io_lib:format("~p", [Name])
  end.

-spec to_str(atom() | string() | binary()) -> string().
to_str(V) when is_atom(V)   -> atom_to_list(V);
to_str(V) when is_list(V)   -> V;
to_str(V) when is_binary(V) -> binary_to_list(V).
