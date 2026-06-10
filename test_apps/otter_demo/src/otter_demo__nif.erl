-module(otter_demo__nif).
-moduledoc """
Erlang side of the otter_demo NIF.

Declares stubs for every NIF and the `-on_load` callback that loads the
shared library. Each NIF stub exits with `nif_not_loaded` if it is ever
called before `erlang:load_nif/2` has succeeded — in practice unreachable
because the load happens automatically at module load time.

EUnit tests live in `otter_demo__nif_test`; run them with `rebar3 eunit`.
""".

-on_load(on_load/0).

-export([hello/0, add/2, echo/1, type_of/1, reverse_binary/1, sum_list/1]).
-export([test_eq/2, test_ord/2, test_debug/1, test_try_from/1,
         test_binary_traits/0, test_from_str/1, reverse_list/1, list_tail/1]).
-export([atom_name/1]).
-export([hm_new/0, hm_put/3, hm_get/2]).
-export([test_map/0, test_tuple/0, double_float/1, test_pid/0, new_ref/0]).
-export([divide/2, dirty_cpu_thread_type/0, send_from_thread/0]).
-export([panicking_resource_new/0]).

%%------------------------------------------------------------------------------


%%%=============================================================================
%%% Callbacks

-spec on_load() -> ok | {error, term()}.
on_load() ->
  case code:priv_dir(otter_demo) of
    {error, _} -> error(unreachable);
    Name ->
      Name2 = case filename:join(Name, "native/otter_demo") of
        Binary when is_binary(Binary) -> binary_to_list(Binary);
        String -> String
      end,
      erlang:load_nif(Name2, 0)
  end.


%%%=============================================================================
%%% Public — NIFs
%%
%% Stubs replaced at load time by the otter-generated wrappers in lib.rs.

%%------------------------------------------------------------------------------
%% Basic operations

-spec hello() -> atom().
hello() -> exit(nif_not_loaded).

-spec add(integer(), integer()) -> integer().
add(_A, _B) -> exit(nif_not_loaded).

-spec echo(term()) -> term().
echo(_Term) -> exit(nif_not_loaded).

-spec type_of(term()) -> atom().
type_of(_Term) -> exit(nif_not_loaded).

%%------------------------------------------------------------------------------
%% Binary and list operations

-spec reverse_binary(binary()) -> binary().
reverse_binary(_B) -> exit(nif_not_loaded).

-spec sum_list([integer()]) -> integer().
sum_list(_L) -> exit(nif_not_loaded).

-spec test_from_str(binary()) -> string().
test_from_str(_B) -> exit(nif_not_loaded).

-spec reverse_list(list()) -> list().
reverse_list(_L) -> exit(nif_not_loaded).

-spec list_tail(list()) -> term().
list_tail(_L) -> exit(nif_not_loaded).

%%------------------------------------------------------------------------------
%% Comparison and inspection

-spec test_eq(term(), term()) -> boolean().
test_eq(_A, _B) -> exit(nif_not_loaded).

-spec test_ord(term(), term()) -> less | equal | greater.
test_ord(_A, _B) -> exit(nif_not_loaded).

-spec test_debug(term()) -> binary().
test_debug(_V) -> exit(nif_not_loaded).

-spec test_try_from(integer()) -> integer().
test_try_from(_V) -> exit(nif_not_loaded).

-spec test_binary_traits() -> ok.
test_binary_traits() -> exit(nif_not_loaded).

-spec atom_name(atom()) -> binary().
atom_name(_A) -> exit(nif_not_loaded).

%%------------------------------------------------------------------------------
%% Resource (HashMap)

-spec hm_new() -> reference().
hm_new() -> exit(nif_not_loaded).

-spec hm_put(binary(), binary(), reference()) -> ok.
hm_put(_K, _V, _HM) -> exit(nif_not_loaded).

-spec hm_get(binary(), reference()) -> {ok, binary()} | error.
hm_get(_K, _HM) -> exit(nif_not_loaded).

%%------------------------------------------------------------------------------
%% Containers and primitive types

-spec test_map() -> ok.
test_map() -> exit(nif_not_loaded).

-spec test_tuple() -> ok.
test_tuple() -> exit(nif_not_loaded).

-spec double_float(float()) -> float().
double_float(_V) -> exit(nif_not_loaded).

-spec test_pid() -> pid().
test_pid() -> exit(nif_not_loaded).

-spec new_ref() -> reference().
new_ref() -> exit(nif_not_loaded).

%%------------------------------------------------------------------------------
%% Errors and scheduling

-doc """
Integer division. Raises `error:division_by_zero` when `B` is `0`. The Rust
side returns `Result<Integer, Atom>` and the otter macro maps the `Err`
arm to an `enif_raise_exception` of the encoded atom.
""".
-spec divide(integer(), integer()) -> integer().
divide(_A, _B) -> exit(nif_not_loaded).

-spec dirty_cpu_thread_type() -> atom().
dirty_cpu_thread_type() -> exit(nif_not_loaded).

-spec send_from_thread() -> ok.
send_from_thread() -> exit(nif_not_loaded).

%%------------------------------------------------------------------------------
%% S1 regression — panicking resource destructor

-doc """
Returns a resource whose `Drop` panics. The destructor wrapper in otter
must `catch_unwind` the panic so the VM survives — see the S1 regression
test in `otter_demo__nif_test`.
""".
-spec panicking_resource_new() -> reference().
panicking_resource_new() -> exit(nif_not_loaded).
