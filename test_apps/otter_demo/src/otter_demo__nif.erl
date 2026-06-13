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
-export([test_map/0, test_tuple/0, double_float/1, nan_float/0, test_pid/0, new_ref/0]).
-export([divide/2, dirty_cpu_thread_type/0, send_from_thread/0]).
-export([send_to/2, cpu_time/0]).
-export([panicking_resource_new/0]).
-export([select_resource_new/0, select_register/1, select_stop/1, select_stop_count/1]).
-export([select_x_register/2]).
-export([monitor_resource_new/0, monitor_pid/2, monitor_down_count/1]).
-export([test_time/0, test_consume_timeslice/0]).
-export([port_send/2]).

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

-spec nan_float() -> no_return().
nan_float() -> exit(nif_not_loaded).

-spec test_pid() -> pid().
test_pid() -> exit(nif_not_loaded).

-spec new_ref() -> reference().
new_ref() -> exit(nif_not_loaded).

%%------------------------------------------------------------------------------
%% Errors and scheduling

-doc """
Integer division. Raises `error:division_by_zero` when `B` is `0`. The Rust
side returns `Result<Integer, Raised>`; the `Raised` is produced by
`env.raise_exception(division_by_zero)` and propagated out with `?`.
""".
-spec divide(integer(), integer()) -> integer().
divide(_A, _B) -> exit(nif_not_loaded).

-spec dirty_cpu_thread_type() -> atom().
dirty_cpu_thread_type() -> exit(nif_not_loaded).

-spec send_from_thread() -> ok.
send_from_thread() -> exit(nif_not_loaded).

-spec send_to(pid(), term()) -> ok.
send_to(_To, _Msg) -> exit(nif_not_loaded).

-spec cpu_time() -> erlang:timestamp().
cpu_time() -> exit(nif_not_loaded).

%%------------------------------------------------------------------------------
%% S1 regression — panicking resource destructor

-doc """
Returns a resource whose `Drop` panics. The destructor wrapper in otter
must `catch_unwind` the panic so the VM survives — see the S1 regression
test in `otter_demo__nif_test`.
""".
-spec panicking_resource_new() -> reference().
panicking_resource_new() -> exit(nif_not_loaded).

%%------------------------------------------------------------------------------
%% audit-01 regression — select stop callback

-doc """
Returns a resource owning a connected socket pair, for exercising the
`enif_select` stop path. See the `select_stop` test in
`otter_demo__nif_test`.
""".
-spec select_resource_new() -> reference().
select_resource_new() -> exit(nif_not_loaded).

-doc "Registers READ interest on the resource's fd. Returns the select flags.".
-spec select_register(reference()) -> integer().
select_register(_R) -> exit(nif_not_loaded).

-doc """
Drives the select-stop path (`ERL_NIF_SELECT_STOP`), invoking the
resource's `stop` callback. Returns the select flags.
""".
-spec select_stop(reference()) -> integer().
select_stop(_R) -> exit(nif_not_loaded).

-doc "Number of times the resource's `stop` callback has run.".
-spec select_stop_count(reference()) -> non_neg_integer().
select_stop_count(_R) -> exit(nif_not_loaded).

-doc """
Selects READ on the resource's fd with `Msg` as the custom notification,
then makes the fd readable so the BEAM delivers `Msg` to the caller.
Returns the select flags.
""".
-spec select_x_register(reference(), term()) -> integer().
select_x_register(_R, _Msg) -> exit(nif_not_loaded).

%%------------------------------------------------------------------------------
%% Resource monitor — down callback

-doc """
Returns a resource for exercising `enif_monitor_process`. See the
`monitor_down` test in `otter_demo__nif_test`.
""".
-spec monitor_resource_new() -> reference().
monitor_resource_new() -> exit(nif_not_loaded).

-doc """
Monitors `Pid` via the resource. When `Pid` exits, the resource's `down`
callback runs. Returns `ok` if the monitor was established, `error`
otherwise.
""".
-spec monitor_pid(reference(), pid()) -> ok | error.
monitor_pid(_R, _Pid) -> exit(nif_not_loaded).

-doc "Number of times the resource's `down` callback has run.".
-spec monitor_down_count(reference()) -> non_neg_integer().
monitor_down_count(_R) -> exit(nif_not_loaded).

%%------------------------------------------------------------------------------
%% Time and scheduling helpers

-doc "Exercises the time module (monotonic_time, time_offset, convert_time_unit).".
-spec test_time() -> ok.
test_time() -> exit(nif_not_loaded).

-doc "Drives enif_consume_timeslice to exhaustion. Returns ok if reported used up.".
-spec test_consume_timeslice() -> ok | error.
test_consume_timeslice() -> exit(nif_not_loaded).

-doc """
Sends `Data` to `Port` via `enif_port_command`. The calling process must
own the port. Returns `ok` if the command was accepted, `error` otherwise.
""".
-spec port_send(port(), binary()) -> ok | error.
port_send(_Port, _Data) -> exit(nif_not_loaded).
