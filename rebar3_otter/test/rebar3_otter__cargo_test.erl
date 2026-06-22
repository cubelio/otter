-module(rebar3_otter__cargo_test).
-moduledoc """
Unit tests for the by-convention cdylib artifact path logic
(`rebar3_otter__cargo:artifact_path/4`), which replaced parsing cargo's JSON
output (issue audit-19 / SAFETY_AUDIT.md M1).

The explicit-`--target` cases are host-independent and pin the layout
(`<dir>/<triple>/<profile>/<file>`), the `lib` prefix / extension per target
platform, and the `-`→`_` crate-name normalization. The host case (`Target =
undefined`) is checked against `os:type/0` so it stays correct on whatever
platform runs the suite.
""".

-include_lib("eunit/include/eunit.hrl").

-define(C, rebar3_otter__cargo).

%%------------------------------------------------------------------------------

-spec linux_target_test() -> _.
linux_target_test() ->
  ?assertEqual("/t/x86_64-unknown-linux-gnu/release/libmy_nif.so",
               ?C:artifact_path("/t", "my_nif", release, "x86_64-unknown-linux-gnu")).

-spec windows_target_test() -> _.
windows_target_test() ->
  %% No `lib` prefix, `.dll`, and the debug profile dir.
  ?assertEqual("/t/x86_64-pc-windows-msvc/debug/my_nif.dll",
               ?C:artifact_path("/t", "my_nif", debug, "x86_64-pc-windows-msvc")).

-spec darwin_target_test() -> _.
darwin_target_test() ->
  %% macOS cdylib source artifact is `.dylib` (nif_filename/1 maps the
  %% destination to `.so` separately).
  ?assertEqual("/t/aarch64-apple-darwin/release/libmy_nif.dylib",
               ?C:artifact_path("/t", "my_nif", release, "aarch64-apple-darwin")).

-spec name_normalization_test() -> _.
name_normalization_test() ->
  %% A `-` in the crate name becomes `_` in the artifact filename.
  ?assertEqual("/t/x86_64-unknown-linux-gnu/release/libmy_nif.so",
               ?C:artifact_path("/t", "my-nif", release, "x86_64-unknown-linux-gnu")).

-spec target_atom_test() -> _.
target_atom_test() ->
  %% Target may arrive as an atom (config sugar); it is stringified.
  ?assertEqual("/t/x86_64-unknown-linux-gnu/release/libmy_nif.so",
               ?C:artifact_path("/t", "my_nif", release, 'x86_64-unknown-linux-gnu')).

-spec host_no_target_test() -> _.
host_no_target_test() ->
  %% No `--target`: the triple subdir is absent and prefix/ext follow the host.
  Expected = filename:join(["/t", "release", host_filename("my_nif")]),
  ?assertEqual(Expected, ?C:artifact_path("/t", "my_nif", release, undefined)).

%%------------------------------------------------------------------------------

-spec host_filename(string()) -> string().
host_filename(Norm) ->
  case os:type() of
    {win32, _}     -> Norm ++ ".dll";
    {unix, darwin} -> "lib" ++ Norm ++ ".dylib";
    {unix, _}      -> "lib" ++ Norm ++ ".so"
  end.
