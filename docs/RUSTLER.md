# Otter vs Rustler

There is already an established library that builds Erlang NIFs from Rust,
`rustler`. As a regular user of `rustler`, I ran up against many points of
friction. The design and documentation lean toward Elixir over Erlang. The API
surface made several opinionated decisions, like how to convert terms and when
to raise an exception. It prefers syntactic sugar to explicitness.

I built `otter` to be on the opposite end of the spectrum. Everything is
explicit and as close to the original NIF C API as possible. The design
philosophy was to expose the full capabilities of the NIF API in the most
idiomatic Rust way without any opinionated decisions hidden in the scaffolding.
If a NIF programmer wouldn't recognize a concept, it doesn't belong.

This document is a long-form comparison of `rustler` and `otter`: where the two
libraries genuinely differ, with the mechanism named on each side so every claim
is checkable against source.

Two kinds of difference run through it. First, **capabilities otter has that rustler's
current public surface does not** — things you cannot do in a rustler NIF at all.
Second, **shared capabilities otter implements more faithfully or more efficiently** —
where both libraries can do the thing, but otter mirrors the NIF C API as it is
rather than reshaping it, and pays fewer runtime costs as a result.

The unifying axis: **rustler maps BEAM concepts onto convenient Rust shapes; otter
mirrors the `enif_*` API as the BEAM team published it.** Faithfulness is not pedantry
— it is what buys both the correctness (improper lists, range-checked integers) and
the efficiency (control over when each `enif_*` call happens) below.

---

## What rustler is

Rustler is a library for writing Erlang NIFs in Rust. It is callable from both Erlang
and Elixir; rustler's own README states that "Elixir is favored as of now,"
operationalized through the `rustler_mix` build tool, the `mix rustler.new`
getting-started flow, Elixir-flavored examples, and Elixir-specific derive macros
(`NifStruct`, `NifException`). It is mature, widely deployed, and well engineered.
This document assumes rustler 0.38 (the `_oss/rustler` tree).

Repository: https://github.com/rusterlium/rustler

---

## Capabilities otter has that rustler does not

These are not "done differently." They are absent from rustler's public surface.

### Hot code upgrade and unload

A NIF library can define `load`, `upgrade`, and `unload` lifecycle callbacks (a
fourth, `reload`, has been deprecated since OTP 20; both libraries correctly
leave it NULL). Rustler's `init!` wires exactly one of the three. Reading the
generated `ErlNifEntry`, `load` points at a generated function; `upgrade` and
`unload` are hardcoded to `None`. The `init!` input parser accepts only a `load`
key — there is no syntax to supply an `upgrade` or `unload` function. And the
`priv_data` slot handed to `load` is bound as `_priv_data` and never written.

The consequences for a rustler-backed module:

- **No `upgrade` callback.** Per the `erl_nif` contract, a library with a NULL
  `upgrade` pointer cannot replace a live one — the load is rejected. A rustler NIF
  module therefore cannot be hot-code-upgraded in place.
- **No `unload` callback.** No notification on module purge; no hook to release
  library-global resources.
- **No `priv_data`.** No managed per-library state, and therefore nothing to carry
  across an upgrade even if `upgrade` existed. State must live in Rust `static`s,
  which get no migration hook and whose cross-version behavior is undefined.

Otter wires all three. `load` and `upgrade` run with an `InitEnv`; `unload` runs with
a `DeinitEnv`. `upgrade` is *always* installed (a user who supplies no `upgrade`
callback gets a no-op that returns "ok") precisely so the module stays upgradeable.
`priv_data` is an otter-owned `#[repr(C)]` value with a frozen two-field cross-build
header (`magic`, `user_priv_data`) followed by a build-private resource registry that
is never read across the upgrade boundary.

The hot-upgrade story is the subject of its own design invariant. otter treats the
upgrade boundary as a **foreign-ABI boundary**: outside the `raw` feature, it never
assumes two builds of the library share an allocator, a `std` layout, or even a
layout for identical source. Resource takeover across an upgrade is gated by an ABI
fingerprint so a new build does not blindly adopt another build's payloads. See
`DESIGN.md` ("Core safety invariant") and `docs/UPGRADE.md` for the full scheme.
Rustler has no equivalent because it has no `priv_data` and no upgrade hook to make
assumptions in.

### Improper lists

An Erlang list is a chain of cons cells; the final tail need not be `[]`. Otter models
this directly. `List<'id>` decomposes via `node(env)` into `Node::Nil` or
`Node::Cell(head, tail)` from one `enif_get_list_cell`, and `cons` builds a cell whose
tail may be a list, `[]`, or any other term. The `Vec<T>` codec walks the cells and,
on reaching a non-`[]`, non-cell tail, returns a clean `CodecError::WrongType`.

Rustler's safe surface cannot represent an improper list. Every list `Encoder`
(`Vec<T>`, `&[T]`) routes through `enif_make_list_from_array`, which only emits proper
lists. On the decode side, `ListIterator::next` **panics** —
`panic!("list iterator found improper list")` — when it reaches a non-cons tail. So a
NIF that decodes a `Vec<T>` from an improper list aborts via panic rather than
returning an error. (Rustler does expose the raw cell builder `Term::list_prepend`, so
one *can* mechanically construct an improper list; it simply cannot round-trip one
through the `Encoder`/`Decoder` layer or iterate it without panicking.)

### Stealing a message heap from inside a NIF

Sending a term to another process can copy it (`enif_send` with a NULL message env)
or *move* it (a non-NULL message env, whose entire heap the BEAM transplants into the
message in O(1) — "stealing"). Crossed with whether the send is attributed to the
calling process, that is a 2×2. Otter exposes all four as free functions in
`otter::types`:

| | copy a live term | steal an owned-env heap |
|---|---|---|
| **no caller** (off-thread) | `send_copy` | `send_move` |
| **attributed** (in-NIF) | `send_copy_from` | `send_move_from` |

The `_from` verbs take a `CallingEnv` and attribute the send to the running process;
the plain verbs pass a NULL caller and are for threads the BEAM does not manage. They
are free functions, not methods, because the pid and env are ingredients of the
operation, not its owner.

Rustler exposes two send functions. `Env::send` copies (always NULL message env). Its
one steal path, `OwnedEnv::send_and_clear`, **panics if called on a scheduler thread**
and always passes a NULL caller. So in rustler:

- You cannot steal-send from **inside a NIF** at all — the only steal path is
  off-thread-only.
- You cannot attribute a steal-send to the calling process — it is always NULL-caller.

`send_move` and `send_move_from` have no rustler equivalent. (The per-send and
per-clear *costs* of rustler's send path are covered under efficiency, below.)

### Term types rustler's surface omits

- **`Bitstring<'id>`.** Erlang distinguishes byte-aligned binaries from
  arbitrary-length bitstrings. Otter decodes `Bitstring<'id>` as a type distinct from
  `Binary<'id>`. Rustler's surface only goes through `enif_inspect_binary`;
  non-byte-aligned bitstrings are not first-class.
- **`Port<'id>` and `Fun<'id>`.** Otter decodes both. Rustler's public surface
  includes neither — a NIF receiving a port or fun keeps it as an opaque term.

### `enif_select` / `enif_select_x` and `enif_set_option`

Otter wraps `enif_select` and `enif_select_x` (integrating async file descriptors with
the scheduler) and `enif_set_option` (per-NIF tuning such as `delay_halt`). Rustler
exposes a safe wrapper for neither.

---

## Compile-time where rustler is run-time

Otter pushes work the type system can do out of the running NIF. The recurring shape:
rustler tracks a fact at runtime (a stored pointer, an enum tag, a branch); otter
encodes the same fact in a type and lets the compiler discharge it.

### The centerpiece: env identity is a compile-time brand, not a runtime check

Both libraries make `Env` and `Term` invariant over a synthetic lifetime
(`PhantomData<*mut &'a ()>`) so a term cannot escape the env it belongs to. The
mechanisms diverge in one decisive way: **who guarantees that two different envs carry
two different brands.**

In rustler this is a *manual, unchecked* obligation. `Env::new` is `unsafe`, and its
doc states the rule in those words — *"Don't create multiple `Env`s with the same
lifetime"* — calling it "the most important safety rule of Rustler." Nothing enforces
it. Two envs constructed with the same lifetime are the same type, and the invariance
device only blocks `'a → 'b` *coercion*; it does nothing when two envs are handed the
*same* `'a`.

Because the brand is advisory, rustler cannot trust it for the safety-critical
decision — *is this term's heap the same as this env's heap?* — and so it pays for that
decision at runtime instead:

- **Every `Term` stores its own `Env`.** `struct Term<'a> { term: NIF_TERM, env: Env<'a> }`.
  A rustler term is two words.
- **Every cross-env encode runtime-compares env pointers.** `Encoder for Term` calls
  `in_env`, which does `if self.get_env() == env { reuse } else { enif_make_copy }`,
  and `Env`'s equality compares the raw `NIF_ENV` pointers. The comment on `in_env`
  concedes the point: it can skip the copy only because it "just proved" — at runtime —
  that the pointers match.

Otter's brand is a *compile-time proof*. There is no public `Env::new(marker)`; an env
exists only inside a `for<'id>` closure (`CallEnv::with_raw`, `InitEnv::with_raw`, … —
the GhostCell construction), so the user can neither name nor reuse a brand. Two
independently entered envs carry two brands the compiler cannot unify. Combining a
term from one with the other is a compile error:

```rust
// otter — cross-brand use is rejected by the compiler. No runtime check exists.
CallEnv::with_raw(raw_env_1, |env1| {
    CallEnv::with_raw(raw_env_2, |env2| {
        let t = env1.error_tuple("boom");
        let _ = use_in(env2, t);   // error[E0521]: borrowed data escapes outside of closure
    });
});
```

```rust
// rustler — the equivalent compiles and is safe, but only because the term carries
// its env and the encode runtime-compares and copies on mismatch.
let t2 = t1.in_env(env2);          // pointers differ -> enif_make_copy at run time
```

(The otter rejection is asserted in-tree by `brand_tests::brands_are_distinct`.)

This is not a soundness claim against rustler — rustler is sound; the runtime compare
backstops the advisory brand. It is a **representation and cost** difference that
follows directly from the manual-contract design:

- otter terms are **one word** (no stored env); rustler terms are **two**.
- otter never runtime-compares envs to decide copy-vs-reuse: same brand is statically
  known to be the same env (free reuse), cross brand is a compile error (you write an
  explicit `copy_to`). rustler makes that decision on every encode.

The brand otter chose costs the user nothing to uphold, because the compiler upholds
it. The brand rustler chose must be defended at runtime on every term, because the
user might break it.

### Env kind and pid locality are types, not runtime tags

The same move appears in the small.

**Env kind.** Rustler stores the kind of an env as a runtime enum field
(`EnvKind { ProcessBound, ProcessIndependent, Init }`) and checks it with a branch —
`Env::send` does `if self.kind == EnvKind::ProcessIndependent { return Err(...) }`.
Otter makes each kind a distinct type (`CallEnv`, `InitEnv`, `CallbackEnv`,
`DeinitEnv`). Operations that require a kind take that type: `register::<T>` takes an
`InitEnv`, so registering a resource outside `load`/`upgrade` is a compile error, not
a runtime check; the attributed sends require a `CallingEnv`, implemented only for
`CallEnv` and `CallbackEnv`. There is no runtime kind field and no branch.

**Pid locality.** `enif_send` requires a *local* pid. Rustler has a single `LocalPid`
type, obtained by a runtime `enif_get_local_pid` check that returns `BadArg` on a
non-local pid. Otter models the two `enif` concepts as two types: `Pid<'id>` (a pid
*term*) and `LocalPid` (the resolved `ErlNifPid`), bridged by an explicit
`Pid::to_local(env) -> Option<LocalPid>`. Because the send verbs take `&LocalPid`,
"this pid has been resolved and is sendable" is carried in the type — you resolve once,
at a point you choose, rather than re-deriving it per operation.

### NIF registration is verified at compile time

Rustler collects NIFs with the `inventory` crate. Each `#[rustler::nif]` expands into
an `inventory::submit!` that writes a record into a linker section
(`.init_array` / `.ctors`); at load time `inventory::iter` walks that section to
discover what was registered. The source never names the list — registration is
whatever survived linking. There is no compile-time check that the set is complete, no
greppable export list, and the mechanism leans on the linker preserving inserted
symbols across optimization modes and link types.

Otter requires every NIF to be listed explicitly in `init!`. The list is visible,
greppable, and checked at compile time — the way Erlang itself declares NIFs.

---

## Faithful types: no silent reinterpretation

Where rustler reshapes an `enif` value into a more convenient Rust shape, the
convenience hides an edge case. Otter reflects the value as the API defines it.

### Integers are range-checked, not truncated

Rustler decodes the narrow integer types (`i8`/`u8`/`i16`/`u16`) by reading through a
wider `enif_get_int`/`enif_get_uint` and then `as`-casting to the target width — a
cast that truncates with no range check. Decoding the Erlang integer `300` into a
`u8` silently yields `44`.

Otter decodes narrow integers through a checked `try_from` and returns
`CodecError::IntegerOverflow` when the value does not fit. A value that cannot be
represented is an error, never a wrong number.

### Floats do not absorb integers

Rustler's `f64` decoder falls back to decoding an `i64` and casting when
`enif_get_double` fails, so an Erlang *integer* term decodes into a Rust float. Otter's
float codec accepts only floats (and range-checks `f32`), so the term type you ask for
is the term type you get.

### Raise and return stay distinct

Rustler's NIF error type is an `Error` enum with five variants. Two of them —
`Error::Atom(&str)` and `Error::Term(Box<dyn Encoder>)` — do not raise: they *return*
a value (the latter as an `{error, Term}` tuple). The other three — `BadArg`,
`RaiseAtom`, `RaiseTerm` — raise. One `Err` channel thus encodes two different
control-flow behaviors, and a baked-in `{error, _}` convention, depending on which
variant you pick.

The NIF C API has exactly two outcomes: return a term, or set a pending exception
(`enif_make_badarg` / `enif_raise_exception`) that the BEAM raises on return. Otter
mirrors that split. Raising is `CallEnv::badarg()` / `CallEnv::raise(reason)`, each
returning `Result<T, Raised<'id>>` (always `Err`, generic over the success type). A
NIF's idiomatic shape is `Result<T, Raised<'id>>`: `Ok(v)` returns; `Err(Raised)`
carries the already-pending exception straight out. `Raised<'id>` is a term-less
typestate token — it can only exist *after* a real raise — so exit never re-raises and
there is no enum dispatch. The encode side is symmetric: a failed `Encoder` raises
`badret`, mirroring the `badarg` a failed decode raises.

### Atoms are UTF-8, not Latin-1

Default-configured rustler can only create Latin-1 atoms; passing UTF-8 bytes silently
produces the wrong atom — `"é"` becomes `Ã©` — with no error. (Enabling the
`nif_version_2_17` feature switches to the `enif_make_new_atom_len` call otter uses
unconditionally.) Otter's `Atom::intern` always calls
`enif_make_new_atom_len(..., ERL_NIF_UTF8)`, has no Latin-1 path, and returns
`Result<Atom, AtomError>` so the one reachable failure (`NameTooLong`, > 255 chars) is
a typed error rather than a silent mis-encoding.

### Three term-resolution levels

Rustler exposes a single `Term<'a>` — a thin `NIF_TERM` wrapper that defers all type
information. Otter exposes four levels, so the user controls how much work happens at
argument receipt:

- `AnyTerm<'id>` — bare machine word, zero work (`Term` is the trait it implements)
- `TypedTerm<'id>` — typed enum, one `enif_term_type` call
- concrete types (`Integer<'id>`, `Bitstring<'id>`, …) — type known, data still lazy
- native Rust types (`i64`, `String`, `Vec<T>`, `HashMap<K, V>`, …) — decoded
  straight into an owned Rust value via `Decoder`, no term left to inspect

---

## Doing the same work with fewer `enif_*` calls

Because otter does not reshape terms into convenience structures, it keeps control over
exactly which `enif_*` call happens and when. The differences below are step-by-step
call sequences for the same operation.

**Build a binary from `&[u8]`.** Rustler's idiomatic path goes through `OwnedBinary`:
`enif_alloc_binary` → fill → `enif_make_binary` — two FFI calls and an owned-binary
handoff. (Its one-call `NewBinary` exists but is a separate, less-emphasized type.)
Otter's primary constructor `Binary::from_bytes` is one call — `enif_make_new_binary`
returns the buffer pointer, which is filled in place.

| | rustler (`OwnedBinary`) | otter (`Binary::from_bytes`) |
|---|---|---|
| FFI calls | `enif_alloc_binary`; `enif_make_binary` | `enif_make_new_binary` |

**Steal-send a term, then clear the env** (the producer hot loop). Rustler's
`send_and_clear` probes the thread, re-runs an encoder closure, sends, and clears — and
`clear` allocates a fresh `Arc` every call (the generation token for `SavedTerm`
invalidation). Otter's `send_move` sends the prepared term and marks the arena dirty;
the later `clear` increments an atomic stamp.

| step | rustler (`send_and_clear`) | otter (`send_move` + `clear`) |
|---|---|---|
| thread probe | `enif_thread_type` every send | — |
| send | `enif_send` | `enif_send` |
| clear | `Arc::new` (heap alloc) + `enif_clear_env` | `enif_clear_env` + `AtomicU64` increment |

**Encode a tuple of arity N.** Both build an element array and call
`enif_make_tuple_from_array` once. Rustler heaps that array in a `Vec`; otter builds it
on the stack (`[RawTerm; N]`).

**The `OwnedEnv` generation token.** This is the same trade as the `clear` row above,
stated as a design point. Rustler detects use-after-clear of a `SavedTerm` with
`Arc<NIF_ENV>` / `Weak<NIF_ENV>`: each save downgrades the `Arc`, each `clear`
replaces it (one heap allocation), and a stale `SavedTerm` fails its `Weak::upgrade`.
Otter records a process-global monotonic `AtomicU64` stamp on each `OwnedEnvTerm`;
`clear` takes a fresh stamp; a stale term fails one `u64` compare. No reference
counting, no `Weak` upgrade — one atomic increment per generation, one equality check
per use. (Because the stamp is globally unique, a match alone identifies the exact
arena generation, so otter needs no env-pointer comparison and is immune to the
freed-then-reused-env aliasing that a pointer check is exposed to.)

### The honest counter-trade

Faithfulness is not free everywhere. Otter's `Binary<'id>` is a lean one-word term that
does not cache the inspected buffer, so each `as_bytes(env)` re-runs
`enif_inspect_binary` and requires an env. Rustler's `Binary` caches `buf`/`size` at
inspect time, so its `as_slice` is a pointer read with no env and no FFI call. Otter
trades a per-read inspect for a smaller, env-free-to-store term (and offers
`BinaryBuf`, whose owner *does* cache the allocation, for the read-heavy case). The
point of the comparison is the trade, stated in both directions — not a clean sweep.

---

## What otter takes from rustler

Rustler got a great deal right, and otter keeps it.

- **The invariance insight.** Making `Env`/`Term` invariant over a synthetic lifetime
  so terms cannot escape a NIF call, at zero runtime cost, is rustler's core idea.
  Otter keeps the invariance and changes only how the brand is minted (above).
- **The reusable owned-env arena.** `OwnedEnv` with `run`/`clear`, save/restore, and a
  heap-stealing send is a good shape; otter's `OwnedEnvArena` adopts it (and changes
  the generation token, above).
- **The layered architecture.** Concentrating unsafety in a low FFI layer behind a safe
  surface is sound. Otter follows the same shape: the external `enif-ffi` crate is the
  whole FFI floor (raw types + 1:1 `unsafe` shims + the load-time loader); otter is the
  safe env-as-receiver layer on top.
- **Panic catching at the C boundary.** Every NIF wrapper catches panics via
  `catch_unwind`, turning a panic into a BEAM exception rather than undefined behavior.
- **Dynamic symbol loading.** `enif_*` is resolved at load time — `dlsym` on Unix, the
  BEAM-supplied callback table on Windows. Otter does this through `enif-ffi`'s
  `nif_init!`; both platforms are supported.

---

## What otter deliberately excludes

These are rustler conveniences with no faithful `enif` counterpart. Their absence is a
design choice, not a gap.

- **Elixir-specific derives.** `NifStruct` (maps to an Elixir struct with a
  `__struct__` key) and `NifException` have no Erlang equivalent. (`NifRecord`,
  `NifMap`, and `NifTuple` map to real Erlang shapes and are unobjectionable in
  principle; the Elixir-struct derives are the ones excluded.)
- **`NifUntaggedEnum`.** Try-each-variant structural dispatch is not an `enif` concept;
  a value's decode order becomes its type discriminator. Otter hands you a `TypedTerm`
  and you pattern-match explicitly.
- **Serde integration.** The serde data model does not map cleanly to Erlang terms (no
  atoms, no records, strings-vs-binaries), and rustler's mapping is overtly
  Elixir-shaped (`None → :nil`, structs → `%Struct{}`). Users implement
  `Encoder`/`Decoder` directly.
- **Elixir value conventions.** `Option ⇄ :nil` (Erlang uses `undefined`; rustler ships
  a separate `ErlOption` to recover it), Elixir `Truthy`, and the `__struct__` /
  `__exception__` atom conventions are not part of otter's surface.

*On Elixir.* These exclusions are about Elixir-specific *conveniences*, not a
position against Elixir. otter currently ships no Elixir tooling because getting the
Erlang-facing surface right is the priority; building Elixir-facing tooling on top of
otter — or as an opt-in feature — is open once the surface stabilizes.
