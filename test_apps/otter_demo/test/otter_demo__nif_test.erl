-module(otter_demo__nif_test).
-moduledoc """
EUnit tests for `otter_demo__nif`.

`smoke_test_/0` is a generator that returns a list of per-NIF assertions.
Each `?_assert*` runs as its own test, so a single failure does not mask
the others.

A handful of tests need multiple statements in the same process (the
`test_pid` assertion captures `self()`, and `send_from_thread` issues a
synchronous send then waits for the message). Those use
`?_test(begin … end)` to run inline in one process. The HashMap resource
handle `HM` is created once in the generator and captured by the
`hm_put`/`hm_get` thunks; EUnit's default `inorder` execution preserves
the put-before-get ordering.
""".

-include_lib("eunit/include/eunit.hrl").


%%%=============================================================================
%%% Tests

-spec smoke_test_() -> [_].
smoke_test_() ->
  Pid = self(),
  Ref = make_ref(),
  HM = otter_demo__nif:hm_new(),
  [
    %% Basic operations
    ?_assertEqual(world, otter_demo__nif:hello()),
    ?_assertEqual(3, otter_demo__nif:add(1, 2)),
    ?_assertEqual(0, otter_demo__nif:add(-5, 5)),
    ?_assertEqual(hello, otter_demo__nif:echo(hello)),
    ?_assertEqual([1,2,3], otter_demo__nif:echo([1,2,3])),

    %% Type inspection
    ?_assertEqual(atom,      otter_demo__nif:type_of(hello)),
    ?_assertEqual(integer,   otter_demo__nif:type_of(42)),
    ?_assertEqual(float,     otter_demo__nif:type_of(3.14)),
    ?_assertEqual(binary,    otter_demo__nif:type_of(<<"hello">>)),
    ?_assertEqual(list,      otter_demo__nif:type_of([1,2])),
    ?_assertEqual(tuple,     otter_demo__nif:type_of({a, b})),
    ?_assertEqual(map,       otter_demo__nif:type_of(#{a => 1})),
    ?_assertEqual(pid,       otter_demo__nif:type_of(self())),
    ?_assertEqual(reference, otter_demo__nif:type_of(make_ref())),

    %% Binary and list operations
    ?_assertEqual(<<"olleh">>, otter_demo__nif:reverse_binary(<<"hello">>)),
    ?_assertEqual(<<>>,        otter_demo__nif:reverse_binary(<<>>)),
    ?_assertEqual(15, otter_demo__nif:sum_list([1, 2, 3, 4, 5])),
    ?_assertEqual(0,  otter_demo__nif:sum_list([])),

    %% Equality (PartialEq)
    ?_assertEqual(true,  otter_demo__nif:test_eq(hello, hello)),
    ?_assertEqual(false, otter_demo__nif:test_eq(hello, world)),
    ?_assertEqual(true,  otter_demo__nif:test_eq(42, 42)),
    ?_assertEqual(false, otter_demo__nif:test_eq(42, 43)),
    ?_assertEqual(true,  otter_demo__nif:test_eq(3.14, 3.14)),
    ?_assertEqual(false, otter_demo__nif:test_eq(3.14, 2.71)),
    ?_assertEqual(true,  otter_demo__nif:test_eq(<<"abc">>, <<"abc">>)),
    ?_assertEqual(false, otter_demo__nif:test_eq(<<"abc">>, <<"def">>)),
    ?_assertEqual(true,  otter_demo__nif:test_eq([1,2], [1,2])),
    ?_assertEqual(false, otter_demo__nif:test_eq([1,2], [3,4])),
    ?_assertEqual(true,  otter_demo__nif:test_eq({a, b}, {a, b})),
    ?_assertEqual(false, otter_demo__nif:test_eq({a, b}, {c, d})),
    ?_assertEqual(true,  otter_demo__nif:test_eq(#{a => 1}, #{a => 1})),
    ?_assertEqual(false, otter_demo__nif:test_eq(#{a => 1}, #{b => 2})),
    ?_assertEqual(true,  otter_demo__nif:test_eq(Pid, Pid)),
    ?_assertEqual(true,  otter_demo__nif:test_eq(Ref, Ref)),

    %% Ordering (Ord)
    ?_assertEqual(equal,   otter_demo__nif:test_ord(hello, hello)),
    ?_assertEqual(less,    otter_demo__nif:test_ord(abc, xyz)),
    ?_assertEqual(greater, otter_demo__nif:test_ord(xyz, abc)),
    ?_assertEqual(equal,   otter_demo__nif:test_ord(42, 42)),
    ?_assertEqual(less,    otter_demo__nif:test_ord(1, 2)),
    ?_assertEqual(greater, otter_demo__nif:test_ord(2, 1)),
    ?_assertEqual(less,    otter_demo__nif:test_ord(<<"aaa">>, <<"bbb">>)),
    ?_assertEqual(greater, otter_demo__nif:test_ord(<<"bbb">>, <<"aaa">>)),

    %% Debug formatting
    ?_assertEqual(<<"Atom">>,      otter_demo__nif:test_debug(hello)),
    ?_assertEqual(<<"Integer">>,   otter_demo__nif:test_debug(42)),
    ?_assertEqual(<<"Float">>,     otter_demo__nif:test_debug(3.14)),
    ?_assertEqual(<<"List">>,      otter_demo__nif:test_debug([1,2])),
    ?_assertEqual(<<"Tuple">>,     otter_demo__nif:test_debug({a, b})),
    ?_assertEqual(<<"Map">>,       otter_demo__nif:test_debug(#{a => 1})),
    ?_assertEqual(<<"Pid">>,       otter_demo__nif:test_debug(self())),
    ?_assertEqual(<<"Reference">>, otter_demo__nif:test_debug(make_ref())),

    %% TryFrom<Integer>
    ?_assertEqual(42, otter_demo__nif:test_try_from(42)),
    ?_assertEqual(-7, otter_demo__nif:test_try_from(-7)),
    ?_assertEqual(0,  otter_demo__nif:test_try_from(0)),

    %% Binary/BinaryBuilder traits
    ?_assertEqual(ok, otter_demo__nif:test_binary_traits()),

    %% List::from_str
    ?_assertEqual("hello",  otter_demo__nif:test_from_str(<<"hello">>)),
    ?_assertEqual("",       otter_demo__nif:test_from_str(<<>>)),
    ?_assertEqual("héllo",  otter_demo__nif:test_from_str(<<"héllo"/utf8>>)),

    %% List::reverse
    ?_assertEqual([3,2,1], otter_demo__nif:reverse_list([1,2,3])),
    ?_assertEqual([],      otter_demo__nif:reverse_list([])),
    ?_assertEqual([c,b,a], otter_demo__nif:reverse_list([a,b,c])),

    %% ListIterator::tail
    ?_assertEqual([],   otter_demo__nif:list_tail([1,2,3])),
    ?_assertEqual([],   otter_demo__nif:list_tail([])),
    % eqwalizer:ignore
    ?_assertEqual(done, otter_demo__nif:list_tail([1,2|done])),

    %% Atom name
    ?_assertEqual(<<"hello">>, otter_demo__nif:atom_name(hello)),
    ?_assertEqual(<<"world">>, otter_demo__nif:atom_name(world)),

    %% Resource (HashMap) — HM is created once above, captured here
    ?_assertEqual(ok,                otter_demo__nif:hm_put(<<"key">>, <<"value">>, HM)),
    ?_assertEqual({ok, <<"value">>}, otter_demo__nif:hm_get(<<"key">>, HM)),
    ?_assertEqual(error,             otter_demo__nif:hm_get(<<"missing">>, HM)),

    %% Map and tuple operations
    ?_assertEqual(ok, otter_demo__nif:test_map()),
    ?_assertEqual(ok, otter_demo__nif:test_tuple()),

    %% Float roundtrip
    ?_assertEqual(6.28, otter_demo__nif:double_float(3.14)),
    ?_assertEqual(+0.0, otter_demo__nif:double_float(+0.0)),
    ?_assertEqual(-4.0, otter_demo__nif:double_float(-2.0)),

    %% make_double on a non-finite value raises badarg; the pending exception
    %% is surfaced through Raised and the VM survives a follow-up call.
    ?_test(begin
      ?assertError(badarg, otter_demo__nif:nan_float()),
      ?assertEqual(6.28, otter_demo__nif:double_float(3.14))
    end),

    %% Pid — must capture self() inside the thunk
    ?_test(begin
      Self = self(),
      ?assertEqual(Self, otter_demo__nif:test_pid())
    end),

    %% Reference
    ?_test(?assert(is_reference(otter_demo__nif:new_ref()))),

    %% Result return type
    ?_assertEqual(5,  otter_demo__nif:divide(10, 2)),
    ?_assertEqual(-3, otter_demo__nif:divide(-7, 2)),
    ?_assertError(division_by_zero, otter_demo__nif:divide(1, 0)),

    %% Dirty scheduler
    ?_assertEqual(dirty_cpu, otter_demo__nif:dirty_cpu_thread_type()),

    %% OwnedEnv — send + receive must run in the same process
    ?_test(begin
      ?assertEqual(ok, otter_demo__nif:send_from_thread()),
      receive
        from_thread -> ok
      after 5000 ->
        ?assert(false)
      end
    end),

    %% In-NIF send — copy a term into our own mailbox and receive it.
    ?_test(begin
      ?assertEqual(ok, otter_demo__nif:send_to(self(), {hello, 42})),
      receive
        {hello, 42} -> ok
      after 5000 ->
        ?assert(false)
      end
    end),

    %% cpu_time returns an erlang:timestamp()-format 3-tuple.
    ?_assertMatch({_, _, _}, otter_demo__nif:cpu_time()),

    %% S1 regression — panicking resource destructor must not abort the VM.
    %% Create a resource whose Drop panics, drop the reference, force GC.
    %% The destructor wrapper in otter catches the panic via catch_unwind;
    %% if it did not, the panic would unwind across the extern "C" boundary
    %% and the BEAM would abort. The follow-up NIF call proves the VM is
    %% still alive.
    ?_test(begin
      _ = otter_demo__nif:panicking_resource_new(),
      erlang:garbage_collect(),
      ?assertEqual(3, otter_demo__nif:add(1, 2))
    end)
  ].

%% audit-01 regression — the select-stop path must invoke the resource's
%% stop callback, not call a NULL function pointer and segfault the VM.
%% Register READ interest on a socket-pair fd, then drive ERL_NIF_SELECT_STOP.
%% The stop callback bumps a counter; we poll it (STOP may run directly or be
%% scheduled to a poller thread). Reaching the assertions at all proves the VM
%% survived the stop dispatch.
-spec select_stop_test() -> _.
select_stop_test() ->
  R = otter_demo__nif:select_resource_new(),
  Reg = otter_demo__nif:select_register(R),
  ?assert(is_integer(Reg)),
  Stop = otter_demo__nif:select_stop(R),
  %% STOP_CALLED (1) or STOP_SCHEDULED (2) — the stop was accepted.
  ?assert(Stop band 3 =/= 0),
  ?assertEqual(ok, wait_for_stop(R, 100)),
  ?assertEqual(1, otter_demo__nif:select_stop_count(R)).

-spec wait_for_stop(reference(), non_neg_integer()) -> ok | timeout.
wait_for_stop(_R, 0) -> timeout;
wait_for_stop(R, N) ->
  case otter_demo__nif:select_stop_count(R) of
    C when C >= 1 -> ok;
    _ -> timer:sleep(10), wait_for_stop(R, N - 1)
  end.

%% test-01 — resource monitor down callback. Monitoring a process via the
%% resource and letting it exit must dispatch to Resource::down, not a NULL
%% pointer. Monitor a blocking process, kill it, poll the down counter.
-spec monitor_down_test() -> _.
monitor_down_test() ->
  R = otter_demo__nif:monitor_resource_new(),
  Pid = spawn(fun () -> receive die -> ok end end),
  ?assertEqual(ok, otter_demo__nif:monitor_pid(R, Pid)),
  exit(Pid, kill),
  ?assertEqual(ok, wait_for_down(R, 100)),
  ?assertEqual(1, otter_demo__nif:monitor_down_count(R)).

-spec wait_for_down(reference(), non_neg_integer()) -> ok | timeout.
wait_for_down(_R, 0) -> timeout;
wait_for_down(R, N) ->
  case otter_demo__nif:monitor_down_count(R) of
    C when C >= 1 -> ok;
    _ -> timer:sleep(10), wait_for_down(R, N - 1)
  end.
