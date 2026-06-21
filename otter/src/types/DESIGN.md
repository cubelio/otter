# Types

Every Erlang term that crosses the NIF boundary is represented by one of the
types in this directory. The resolution model (`AnyTerm` → `TypedTerm` →
concrete) lets callers choose how much work to pay for: zero cost with
`AnyTerm`, one `enif_term_type` call with `TypedTerm`, or full decoding with
`Decoder`. Every term carries only its env's **brand** `'id` — never the env
itself — so accessors take an `env: impl Env<'id>` of that brand explicitly.


## TypedTerm Resolution

```
RawTerm (machine word)
  │
  ├─ AnyTerm<'id>     zero cost, no type check (repr(transparent) over RawTerm)
  │    │
  │    └─ .resolve(env)  one enif_term_type call → Option (None = unknown type)
  │         │
  │         └─ TypedTerm<'id>   typed enum (Atom | Bitstring | ... | Tuple)
  │              │
  │              └─ T::decode(term, env)   full extraction (e.g. Integer → i64)
```

`Term<'id>` is the *trait* every term implements (`raw_term`, `copy_to`);
`AnyTerm<'id>` is the bare-word handle. `TypedTerm` mirrors `ErlNifTermType`
exactly — one variant per tag. The `Bitstring` variant covers both byte-aligned
binaries and sub-byte bitstrings (BEAM treats every binary as a bitstring); call
`Bitstring::is_binary` or `Bitstring::to_binary` to refine. `resolve(env)` is
uniformly one NIF call regardless of variant.


## Brand Model

Types that reference data on the BEAM heap carry a generative **brand** `'id`,
tied to the `Env<'id>` that owns that heap, via `Invariant<'id> =
PhantomData<*mut &'id ()>`. The brand is minted per-call through a `for<'id>`
closure and cannot escape it, so a term cannot outlive — or be used outside —
the NIF call that created it. The term stores only the brand marker (a ZST), not
the env; every accessor takes an `env: impl Env<'id>` of the matching brand.

Three types have **no brand**: `Atom`, `LocalPid`, `LocalPort`. These are global
or carry their identity in the term word itself (an internal pid/port is a
tagged immediate), so they implement `FreeTerm: for<'id> Term<'id>` and are valid
in any env. `Pid<'id>`/`Port<'id>` — pids/ports of unestablished locality — *do*
carry `'id`, because an external one is heap-boxed (see the Pid section).

`Encoder` for a same-brand term wraps its word for free (no copy); the general
cross-env copy is `Term::copy_to(env)` (`enif_make_copy`).

All concrete types implement `From<T> for TypedTerm<'id>`, enabling `let t:
TypedTerm = atom.into()`. Resolution is `AnyTerm::resolve(self, env) ->
Option<TypedTerm<'id>>`; `TypedTerm` is itself a `Decoder` (decode =
`resolve().ok_or(UnknownTermType)`). There is no `TryFrom<Term>`.


## Codec Traits

```rust
pub trait Encoder<'id> {
    fn encode(&self, env: impl Env<'id>) -> Result<AnyTerm<'id>, CodecError>;
}

pub trait Decoder<'id>: Sized {
    fn decode(term: AnyTerm<'id>, env: impl Env<'id>) -> Result<Self, CodecError>;
}
```

Both directions are fallible. `CodecError` has seven variants: `WrongType`,
`IntegerOverflow`, `NotFinite`, `FloatRange`, `NotUtf8`, `WrongArity`,
`UnknownTermType`. The `#[otter::nif]` macro converts a `Decoder` failure on an
argument into `badarg`, and an `Encoder` failure on a return into `badret`.

Every type in this directory implements both traits and never fails (encode
wraps the word; decode checks the type then rewraps). The fallible impls are the
native-Rust-type conversions in `codec/` (a non-finite float on encode; an
out-of-range integer, bad UTF-8, or wrong-arity tuple on decode). A
return-position `impl Encoder for Result<T, Raised<'id>>` is the `?`-propagation
mechanism — `Ok` encodes the value, `Err` returns the non-value word with the
exception already pending.

---


## Atom

### NIF C Functions

| Function | Signature |
|---|---|
| `enif_make_atom` | `(env, name) → ERL_NIF_TERM` |
| `enif_make_atom_len` | `(env, name, len) → ERL_NIF_TERM` |
| `enif_make_new_atom` | `(env, name, atom_out, encoding) → int` |
| `enif_make_new_atom_len` | `(env, name, len, atom_out, encoding) → int` |
| `enif_make_existing_atom` | `(env, name, atom_out, encoding) → int` |
| `enif_make_existing_atom_len` | `(env, name, len, atom_out, encoding) → int` |
| `enif_is_atom` | `(env, term) → int` |
| `enif_get_atom` | `(env, term, buf, len, encoding) → int` |
| `enif_get_atom_length` | `(env, term, len_out, encoding) → int` |

### Otter API

```rust
struct Atom { term: RawTerm }  // no brand — atoms are global (FreeTerm)
```

| Method | Does | Calls |
|---|---|---|
| `intern(env, name) → Result<Atom, AtomError>` | Create/intern atom from UTF-8 `&str` | `enif_make_new_atom_len` |
| `try_existing(env, name) → Option<Atom>` | Look up without creating | `enif_make_existing_atom_len` |
| `is_atom(env, term) → bool` | Type predicate | `enif_is_atom` |
| `name(self, env) → String` | Read atom's name | `enif_get_atom_length` + `enif_get_atom` |

**`AtomError`** — a single variant `NameTooLong` (a plain Rust error, never a
pending BEAM exception; the env is untouched, so the caller can recover).

**`StaticAtom`** — a pre-declared atom, `OnceLock<Atom>` storage (no term-repr
assumption):

| Method | Does |
|---|---|
| `const new(name) → StaticAtom` | Uninitialized handle |
| `init(&self, env) → Result<(), AtomError>` | Intern at load (from `init!`'s `atoms=[]` it's length-checked at compile time, so infallible there) |
| `get(&self) → Atom` | Acquire load of the `OnceLock<Atom>`; panics if used before `init` |

> **DoS warning:** the atom table is global, fixed-size, and never shrinks;
> exhausting it terminates the VM. Never `intern` untrusted input — use
> `try_existing` and treat `None` as "not recognized, reject", or pre-declare
> trusted names in `init!`'s `atoms = [...]`.

### Internals

`intern` calls `enif_make_new_atom_len` (NIF 2.17), which returns a success/fail
int rather than creating atoms unconditionally. For a Rust `&str` the only
reachable failure is an over-length name (> 255 chars), reported as
`Err(AtomError::NameTooLong)` — a plain Rust error, never a pending exception.
Bad encoding cannot occur (a `&str` is always valid UTF-8), and atom-table
exhaustion is not surfaced here: it aborts the VM (`erts_exit` in `index_put`)
before `intern` could return. `try_existing` stays `Option` because `None` =
"not interned yet" is an expected, non-error outcome. `name` does two calls:
first to get the byte length, then to read into a buffer.

### Not Exposed

`enif_make_atom` (null-terminated variant, `_len` version preferred),
`enif_is_atom` (handled by `enif_term_type` in `resolve()`).

---


## Binary

### NIF C Functions

| Function | Signature |
|---|---|
| `enif_inspect_binary` | `(env, term, bin_out) → int` |
| `enif_alloc_binary` | `(size, bin_out) → int` |
| `enif_realloc_binary` | `(bin, size) → int` |
| `enif_release_binary` | `(bin) → void` |
| `enif_make_binary` | `(env, bin) → ERL_NIF_TERM` |
| `enif_make_new_binary` | `(env, size, term_out) → unsigned char*` |
| `enif_make_sub_binary` | `(env, bin_term, pos, size) → ERL_NIF_TERM` |
| `enif_is_binary` | `(env, term) → int` |
| `enif_inspect_iolist_as_binary` | `(env, term, bin_out) → int` |
| `enif_term_to_binary` | `(env, term, bin_out) → int` |
| `enif_binary_to_term` | `(env, data, size, term_out, opts) → size_t` |

### Otter API

```rust
struct Binary<'id> { raw_term: RawTerm, _id: Invariant<'id> }
struct Bitstring<'id> { raw_term: RawTerm, _id: Invariant<'id> }
```

Accessors take `env: impl Env<'id>` of the binary's brand (the term carries only
the brand).

| Method | Does | Calls |
|---|---|---|
| `as_bytes(self, env) → &'id [u8]` | Zero-copy view of binary data | `enif_inspect_binary` |
| `len(self, env) → usize` | Byte count | `enif_inspect_binary` |
| `is_empty(self, env) → bool` | Empty check | `enif_inspect_binary` |
| `try_str(self, env) → Result<&'id str, Utf8Error>` | Zero-copy UTF-8 view | `enif_inspect_binary` + `std::str::from_utf8` |
| `sub(self, env, pos, len) → Binary<'id>` | Zero-copy sub-binary (panics on OOB) | `enif_make_sub_binary` |
| `from_bytes(env, data) → Binary<'id>` | Allocate and copy bytes onto BEAM heap | `enif_make_new_binary` |
| `is_binary(env, term) → bool` | Type predicate (byte-aligned) | `enif_is_binary` |
| `deserialize(self, env, safe) → Option<AnyTerm<'id>>` | Deserialize a term from this binary's ETF bytes | `enif_binary_to_term` |

`Bitstring` adds `is_binary(self, env) → bool` and `to_binary(self, env) →
Option<Binary<'id>>` to refine to the byte-aligned case. (`Binary` implements
`Debug` and the ordering traits, but **not** `Deref`/`AsRef` — read bytes through
`as_bytes`.)

**BinaryBuf** — growable owned buffer mirroring `Vec<u8>` (no env, no brand; it
owns its `enif_alloc_binary` allocation directly, so it is not a term):

```rust
struct BinaryBuf { bin: enif_ffi::Binary, len: usize, released: bool }
```

| Method | Does | Calls |
|---|---|---|
| `new() → BinaryBuf` | Empty buffer | `enif_alloc_binary(0)` |
| `with_capacity(cap) → BinaryBuf` | Preallocated buffer | `enif_alloc_binary(cap)` |
| `push(&mut self, byte)` | Append one byte, grow if needed | `enif_realloc_binary` |
| `extend_from_slice(&mut self, &[u8])` | Append slice, grow if needed | `enif_realloc_binary` |
| `resize(&mut self, new_len, value)` | Resize and fill new bytes with value | `enif_realloc_binary` |
| `as_bytes(&self) → &[u8]` | View written bytes (zero-copy) | — |
| `as_bytes_mut(&mut self) → &mut [u8]` | Mutable view of written bytes | — |
| `len(&self) → usize` | Bytes written | — |
| `capacity(&self) → usize` | Bytes allocated | — |
| `reserve(&mut self, additional)` | Ensure room for more bytes | `enif_realloc_binary` |
| `into_binary(self, env) → Binary<'a>` | Consume, handing the allocation to the BEAM as a term (shrinks first) | `enif_realloc_binary` + `enif_make_binary` |
| `impl Write` | `write!` and `write_all` support | — |
| `impl Deref<Target=[u8]>` | Auto-coerce to `&[u8]` (written bytes); gives `.to_vec()` | — |
| `impl DerefMut` | Auto-coerce to `&mut [u8]` (written bytes) | — |
| `impl AsRef<[u8]>` / `AsMut<[u8]>` | Trait-based byte access | — |
| `impl Extend<u8>` | Iterator-based appending | — |
| `impl Debug` | `BinaryBuf { len: N, capacity: M }` | — |
| `Drop` | Release if not converted to a term | `enif_release_binary` |

**Serialization** — type-agnostic free verbs in `ops.rs` (re-exported from
`types`), not methods:

| Function | Does | Calls |
|---|---|---|
| `serialize(env, term) → Option<BinaryBuf>` | Serialize any term to ETF bytes; `.into_binary(env)` for a term, `.as_bytes()`/`.to_vec()` for bytes | `enif_term_to_binary` |
| `deserialize(env, &[u8], safe) → Option<AnyTerm<'id>>` | Reconstruct a term from ETF bytes (`safe` rejects unknown atoms) | `enif_binary_to_term` |

### Internals

`as_bytes` calls `enif_inspect_binary` which returns a pointer and size into
the BEAM heap. The returned slice borrows from the environment lifetime `'a`,
so it cannot outlive the NIF call. `BinaryBuf` mirrors `Vec<u8>`: it
tracks `len` (bytes written) and `capacity` (bytes allocated via
`enif_alloc_binary`) separately. `push` and `extend_from_slice` grow via
`enif_realloc_binary` with amortized doubling. `into_binary` calls
`enif_realloc_binary` to shrink to exact `len`, then `enif_make_binary` to
transfer ownership to the BEAM. The `Drop` impl calls `enif_release_binary`
if the buffer is dropped without being converted to a term, preventing leaks.
`BinaryBuf` is also what `Term::serialize` returns, so the same RAII owner
covers both building binaries and holding `enif_term_to_binary` output.

`Bitstring` is a pass-through type with no inspection methods (the NIF API
provides none for sub-byte bitstrings). It implements `Encoder`, `Decoder`,
and `Debug`. It exists because `enif_term_type` cannot distinguish binaries
from non-byte-aligned bitstrings; `resolve()` uses `enif_is_binary` to split
them.

### Not Exposed

`enif_inspect_iolist_as_binary` (iolist flattening is a higher-level operation).
(`from_bytes` uses `enif_make_new_binary` for the one-step alloc+copy+term;
`BinaryBuf` is the growable path with more control.)

---


## Integer

### NIF C Functions

| Function | Signature |
|---|---|
| `enif_get_int` | `(env, term, int_out) → int` |
| `enif_get_uint` | `(env, term, uint_out) → int` |
| `enif_get_long` | `(env, term, long_out) → int` |
| `enif_get_ulong` | `(env, term, ulong_out) → int` |
| `enif_get_int64` | `(env, term, i64_out) → int` |
| `enif_get_uint64` | `(env, term, u64_out) → int` |
| `enif_make_int` | `(env, i) → ERL_NIF_TERM` |
| `enif_make_uint` | `(env, i) → ERL_NIF_TERM` |
| `enif_make_long` | `(env, i) → ERL_NIF_TERM` |
| `enif_make_ulong` | `(env, i) → ERL_NIF_TERM` |
| `enif_make_int64` | `(env, i) → ERL_NIF_TERM` |
| `enif_make_uint64` | `(env, i) → ERL_NIF_TERM` |
| `enif_is_number` | `(env, term) → int` |

### Otter API

```rust
struct Integer<'id> { raw_term: RawTerm, _id: Invariant<'id> }
```

| Method | Does | Calls |
|---|---|---|
| `from_i64(env, val) → Integer<'id>` | Construct from signed 64-bit | `enif_make_int64` |
| `from_u64(env, val) → Integer<'id>` | Construct from unsigned 64-bit | `enif_make_uint64` |
| `to_i64(self, env) → Option<i64>` | Extract as signed 64-bit; `None` on overflow | `enif_get_int64` |
| `to_u64(self, env) → Option<u64>` | Extract as unsigned 64-bit; `None` if negative/overflow | `enif_get_uint64` |
| `to_bigint(self, env) → BigInt` | Read any integer incl. bignums (`bigint` feature) | `enif_term_to_binary` + ETF parse |
| `from_bigint(env, &BigInt) → Integer<'id>` | Build any integer (`bigint` feature) | i64/u64 fast-path else `enif_binary_to_term` of ETF |

### Internals

Construction uses inherent methods (`from_i64`/`from_u64`) because `From` cannot
accept an `Env` parameter. Extraction returns `Option` — `None` when the value
does not fit the requested Rust width.

Erlang integers are arbitrary precision, and the NIF API has no accessor beyond
`enif_get_int64`/`enif_get_uint64`. To read (or write) bignums that exceed
`i64`/`u64`, enable the `bigint` feature: `to_bigint` serializes the term to the
external term format and parses the ETF integer tag (total over every integer
term); `from_bigint` fast-paths the i64/u64 range and otherwise emits an ETF
bignum parsed back with `enif_binary_to_term`. `BigInt` is `num_bigint::BigInt`,
re-exported as `otter::num_bigint`. (The old `to_i128` convenience was removed —
it only spanned `i64::MIN..=u64::MAX`; use `to_bigint` for the unbounded case.)

### Not Exposed

The 32-bit `enif_get_int`/`enif_make_int` and `enif_get_uint`/`enif_make_uint`
are redundant on 64-bit systems (the `_long` variants cover the full range).
`enif_is_number` is not needed (covers both integers and floats;
`enif_term_type` provides the exact distinction).

---


## Float

### NIF C Functions

| Function | Signature |
|---|---|
| `enif_get_double` | `(env, term, double_out) → int` |
| `enif_make_double` | `(env, d) → ERL_NIF_TERM` |

### Otter API

```rust
struct Float<'id> { raw_term: RawTerm, _id: Invariant<'id> }
```

| Method | Does | Calls |
|---|---|---|
| `to_f64(self, env) → f64` | Extract the float value (asserts; always valid) | `enif_get_double` |
| `from_f64(env, val) → Option<Float<'id>>` | Construct from f64; `None` if not finite | `enif_make_double` |

### Internals

Erlang floats are IEEE 754 doubles. The C API and otter both use `f64`/`double`
directly — no precision loss. The non-finite check in `from_f64` is done in
Rust and returns `None` (NaN / infinity), so a rejected value never calls into
the BEAM and the env is never left with a pending exception; a caller that wants
a `badarg` raises one from a `CallEnv` itself. `to_f64` asserts success — a
validated `Float` is always a float term and every Erlang float is an `f64`.

---


## List

### NIF C Functions

| Function | Signature |
|---|---|
| `enif_get_list_cell` | `(env, term, head_out, tail_out) → int` |
| `enif_get_list_length` | `(env, term, len_out) → int` |
| `enif_make_list` | `(env, cnt, ...) → ERL_NIF_TERM` |
| `enif_make_list_from_array` | `(env, arr, cnt) → ERL_NIF_TERM` |
| `enif_make_list_cell` | `(env, head, tail) → ERL_NIF_TERM` |
| `enif_make_reverse_list` | `(env, term, list_out) → int` |
| `enif_is_list` | `(env, term) → int` |
| `enif_is_empty_list` | `(env, term) → int` |
| `enif_make_string` | `(env, string, encoding) → ERL_NIF_TERM` |
| `enif_make_string_len` | `(env, string, len, encoding) → ERL_NIF_TERM` |
| `enif_get_string` | `(env, term, buf, len, encoding) → int` |
| `enif_get_string_length` | `(env, term, len_out, encoding) → int` |

### Otter API

```rust
struct List<'id> { raw_term: RawTerm, _id: Invariant<'id> }

enum Node<'id> {
    Nil,
    Cell(AnyTerm<'id>, AnyTerm<'id>),  // head, tail — unresolved
}
```

| Method | Does | Calls |
|---|---|---|
| `node(self, env) → Node<'id>` | Decompose into nil or cons cell | `enif_get_list_cell` |
| `iter(self, env) → ListIterator<'id>` | Iterator over head elements | `enif_get_list_cell` per `next()` |
| `try_string(self, env) → Option<String>` | Extract string as UTF-8 `String` | `enif_get_string_length` + `enif_get_string` |
| `len(self, env) → Option<usize>` | Element count; `None` for improper lists | `enif_get_list_length` |
| `is_empty(self, env) → bool` | Empty-list check | `enif_get_list_cell` |
| `reverse(self, env) → Option<List<'id>>` | Reverse a proper list; `None` for improper | `enif_make_reverse_list` |
| `from_terms(env, IntoIterator<Item: Term<'id>>) → List<'id>` | Construct from iterable | `enif_make_list_from_array` |
| `from_str(env, &str) → List<'id>` | Construct string (list of codepoints) from UTF-8 | `enif_make_string_len` |
| `cons(env, impl Term<'id>, impl Term<'id>) → List<'id>` | Construct cons cell `[head \| tail]` | `enif_make_list_cell` |
| `is_list(env, term) → bool` | Type predicate (incl. improper/empty) | `enif_is_list` |

**ListIterator** — yields `AnyTerm<'id>` heads, one `enif_get_list_cell` per step
(`FusedIterator`):

| Method | Does |
|---|---|
| `next() → Option<AnyTerm<'id>>` | Yield next head; `None` when a non-cell tail is reached |
| `tail() → Option<AnyTerm<'id>>` | Terminal value after iteration: `[]` for proper lists, improper tail otherwise |

### Internals

Lists in Erlang are cons cells, and otter mirrors this directly. `node`
returns `AnyTerm`s for head and tail — the caller chooses whether to resolve
them. `iter()` builds on this: it yields heads as `AnyTerm`s and stops when
the tail is not a cons cell. After exhaustion, `tail()` returns the terminal
value — `[]` (nil) for proper lists, or the improper tail term. This means
every list walk, proper or improper, is fully observable.

`try_string` uses `enif_get_string_length` to get the UTF-8 byte count, then
`enif_get_string` to extract the string in one pass. The BEAM guarantees
valid UTF-8, so the result is created via `String::from_utf8_unchecked`.

`from_terms` with an empty slice produces the empty list `[]`.

### Not Exposed

`enif_make_list` (variadic — cannot be called from Rust; `from_terms` covers
the same ground), `enif_is_list`/`enif_is_empty_list` (handled by `enif_term_type` +
`enif_get_list_cell`), `enif_make_string` (null-terminated; `from_str` uses
`enif_make_string_len` instead).

---


## Tuple

### NIF C Functions

| Function | Signature |
|---|---|
| `enif_get_tuple` | `(env, tpl, arity_out, array_out) → int` |
| `enif_make_tuple` | `(env, cnt, ...) → ERL_NIF_TERM` |
| `enif_make_tuple_from_array` | `(env, arr, cnt) → ERL_NIF_TERM` |
| `enif_is_tuple` | `(env, term) → int` |

### Otter API

```rust
struct Tuple<'id> { raw_term: RawTerm, _id: Invariant<'id> }              // lean one-word handle
struct TupleView<'id> { raw_term, raw_elements: &'id [RawTerm], _id }     // elements resolved
```

| Method | Does | Calls |
|---|---|---|
| `with_elements(self, env) → TupleView<'id>` | Resolve elements (the single fetch); asserts | `enif_get_tuple` |
| `from_terms(env, IntoIterator<Item: Term<'id>>) → Tuple<'id>` | Construct from iterable | `enif_make_tuple_from_array` |
| `is_tuple(env, term) → bool` | Type predicate | `enif_is_tuple` |

**TupleView** (a `Term`, and a fixed-size collection):

| Method | Does |
|---|---|
| `len(self) → usize` / `is_empty(self) → bool` | Arity (no env) |
| `Index<usize> → AnyTerm<'id>` | `view[i]`, zero-copy; panics OOB |
| `IntoIterator (Item = AnyTerm<'id>)` | Iterate elements |

### Internals

The lean/view split keeps a tuple that is only passed through (matched in
`TypedTerm`, re-encoded, compared) from paying for the element fetch. `Tuple` is
the env-less one-word handle; `with_elements` does the single `enif_get_tuple`
(which cannot fail on a validated tuple — hence the assert) and caches the
element pointer. Indexing/iteration is zero-copy: `AnyTerm` is
`#[repr(transparent)]` over `RawTerm`, so the cached `&[RawTerm]` is reinterpreted
in place as `&[AnyTerm<'id>]`. The pointer is valid for the brand's lifetime (the
process heap is fixed for the NIF call). `TupleView` deliberately has **no**
`PartialEq`/`Ord` (compare through the raw word if needed). Indexing panics OOB —
a deliberate programmer-error signal, like a Rust slice.

### Not Exposed

`enif_make_tuple` (variadic), `enif_is_tuple` (handled by `enif_term_type`).

---


## Map

### NIF C Functions

| Function | Signature |
|---|---|
| `enif_make_new_map` | `(env) → ERL_NIF_TERM` |
| `enif_get_map_size` | `(env, map, size_out) → int` |
| `enif_get_map_value` | `(env, map, key, value_out) → int` |
| `enif_make_map_put` | `(env, map, key, value, map_out) → int` |
| `enif_make_map_update` | `(env, map, key, value, map_out) → int` |
| `enif_make_map_remove` | `(env, map, key, map_out) → int` |
| `enif_make_map_from_arrays` | `(env, keys[], values[], cnt, map_out) → int` |
| `enif_is_map` | `(env, term) → int` |
| `enif_map_iterator_create` | `(env, map, iter, entry) → int` |
| `enif_map_iterator_destroy` | `(env, iter) → void` |
| `enif_map_iterator_is_head` | `(env, iter) → int` |
| `enif_map_iterator_is_tail` | `(env, iter) → int` |
| `enif_map_iterator_next` | `(env, iter) → int` |
| `enif_map_iterator_prev` | `(env, iter) → int` |
| `enif_map_iterator_get_pair` | `(env, iter, key_out, value_out) → int` |

### Otter API

```rust
struct Map<'id> { raw_term: RawTerm, _id: Invariant<'id> }
struct MapIterator<'id> { iter: Box<enif_ffi::MapIterator>, env: AnyEnv<'id>, exhausted: bool }
```

| Method | Does | Calls |
|---|---|---|
| `new(env) → Map<'id>` | Create empty map | `enif_make_new_map` |
| `size(self, env) → usize` | Key-value pair count (asserts) | `enif_get_map_size` |
| `get(self, env, impl Term<'id>) → Option<AnyTerm<'id>>` | Look up key | `enif_get_map_value` |
| `put(self, env, impl Term<'id>, impl Term<'id>) → Map<'id>` | Insert or replace | `enif_make_map_put` |
| `update(self, env, impl Term<'id>, impl Term<'id>) → Option<Map<'id>>` | Update existing key; `None` if absent | `enif_make_map_update` |
| `remove(self, env, impl Term<'id>) → Map<'id>` | Remove key (absent → unchanged) | `enif_make_map_remove` |
| `iter(self, env) → MapIterator<'id>` | Forward iterator over key-value pairs | `enif_map_iterator_create` |
| `is_map(env, term) → bool` | Type predicate | `enif_is_map` |

`MapIterator` implements `Iterator<Item = (AnyTerm<'id>, AnyTerm<'id>)>` (unresolved key/value) and `Drop`.

### Internals

Maps are immutable in Erlang. `put`, `update`, and `remove` each return a new
`Map` — the original is unchanged. `update` returns `Option` (its C function
fails on an absent key), but **`remove` returns `Map`, not `Option`**:
`enif_make_map_remove` fails only on a non-map (an absent key yields the map
unchanged), which cannot happen on a validated `Map`, so the None arm was
unreachable. `size` asserts for the same reason.

`MapIterator` is heap-allocated (`Box<enif_ffi::MapIterator>`) to pin the C
iterator struct and is non-`Copy` (a bitwise copy would share the hashmap-iterator
work-stack pointer and double-free on the second `Drop`; see robust-12). It starts
at the first entry, advances with `enif_map_iterator_next`, and `Drop` calls
`enif_map_iterator_destroy`.

### Not Exposed

`enif_make_map_from_arrays` (bulk construction; can be built with repeated
`put`), `enif_is_map` (handled by `enif_term_type`),
`enif_map_iterator_is_head`/`is_tail`/`prev` (forward-only iteration is
sufficient; the exhaustion check uses `get_pair` returning `None`).

---


## Pid

### NIF C Functions

| Function | Signature |
|---|---|
| `enif_self` | `(env, pid_out) → ErlNifPid*` |
| `enif_get_local_pid` | `(env, term, pid_out) → int` |
| `enif_is_pid` | `(env, term) → int` |
| `enif_is_process_alive` | `(env, pid) → int` |
| `enif_is_current_process_alive` | `(env) → int` |
| `enif_whereis_pid` | `(env, name, pid_out) → int` |

### Otter API

```rust
struct Pid<'id> { raw_term: RawTerm, _id: Invariant<'id> }   // unestablished locality — brand-bound
struct LocalPid { pid: enif_ffi::Pid }                       // validated local — no brand, Copy, FreeTerm
```

| Type / Method | Does | Calls |
|---|---|---|
| `Pid::to_local(self, env) → Option<LocalPid>` | Refine to local; `None` if external | `enif_get_local_pid` |
| `Pid::is_pid(env, term) → bool` | Type predicate | `enif_is_pid` |
| `LocalPid::self_(env: CallEnv) → LocalPid` | Calling process PID (always local; `CallEnv` only) | `enif_self` |
| `LocalPid::whereis(env, name: impl Term) → Option<LocalPid>` | Look up by registered name | `enif_whereis_pid` |
| `LocalPid::is_alive(self, env) → bool` | Check if process is alive | `enif_is_process_alive` |

Sending is **four free verbs** in `types`, a 2×2 of **copy vs. move** ×
**caller-attributed (`_from`, in-NIF) vs. not (plain, off-thread)**. `copy` copies
a live term into the recipient's mailbox (`enif_send`, NULL `msg_env`); `move`
transplants (steals) an `OwnedEnvArena`'s whole heap into the message
(`enif_send`, non-NULL `msg_env`), O(1), leaving the arena dirty until cleared.
The `_from` verbs take an `impl CallingEnv` and pass it as the caller env, so the
BEAM attributes the message to the calling process; the plain verbs pass a NULL
caller (use them from a non-scheduler thread).

| Verb | Caller | Payload | Calls |
|---|---|---|---|
| `send_copy(&LocalPid, msg: impl Term) → bool` | off-thread, NULL caller | copy | `enif_send` (NULL caller + NULL msg_env) |
| `send_move(&LocalPid, &mut OwnedEnvArena, OwnedEnvTerm) → bool` | off-thread, NULL caller | steal | `enif_send` (NULL caller, non-NULL msg_env) |
| `send_copy_from(impl CallingEnv, &LocalPid, msg: impl Term) → bool` | in-NIF, attributed | copy | `enif_send` (caller, NULL msg_env) |
| `send_move_from(impl CallingEnv, &LocalPid, &mut OwnedEnvArena, OwnedEnvTerm) → bool` | in-NIF, attributed | steal | `enif_send` (caller, non-NULL msg_env) |

All four route through one pair of private helpers (`send_move_`/`send_copy_`)
differing only in the caller-env pointer. `is_current_process_alive` is a default
method on `Env`.

### Internals

A *local* pid's term word is a tagged immediate that encodes the process
identity directly — so `LocalPid` (validated via `enif_get_local_pid` /
`enif_self` / `enif_whereis_pid`) is lifetime-free, `Copy`, and safe to store.
An *external* (remote-node) pid is a heap-boxed term whose word is a heap
pointer; storing it past the env would dangle, so `Pid<'a>` carries `'a` and
cannot be stored. `LocalPid` holds the `ErlNifPid` directly, so the operations
that require an internal pid (`enif_send`, `enif_monitor_process`,
`enif_select`, `enif_is_process_alive`) take `&LocalPid` and never build an
`ErlNifPid` from an unvalidated term — the soundness fix for the
external-pid/UAF gap (assessment finding `audit-03`).

### Not Exposed

`enif_is_pid` is the associated fn `Pid::is_pid(env, term)`; type identification
also goes through `enif_term_type` in `resolve`.

---


## Port

### NIF C Functions

| Function | Signature |
|---|---|
| `enif_is_port` | `(env, term) → int` |
| `enif_get_local_port` | `(env, term, port_out) → int` |
| `enif_is_port_alive` | `(env, port) → int` |
| `enif_port_command` | `(env, to_port, msg_env, msg) → int` |
| `enif_whereis_port` | `(env, name, port_out) → int` |

### Otter API

```rust
struct Port<'id> { raw_term: RawTerm, _id: Invariant<'id> }   // unestablished locality — brand-bound
struct LocalPort { port: enif_ffi::Port }                     // validated local — no brand, Copy, FreeTerm
```

| Type / Method | Does | Calls |
|---|---|---|
| `Port::to_local(self, env) → Option<LocalPort>` | Refine to local; `None` if external | `enif_get_local_port` |
| `Port::is_port(env, term) → bool` | Type predicate | `enif_is_port` |
| `LocalPort::whereis(env, name: impl Term) → Option<LocalPort>` | Look up by registered name | `enif_whereis_port` |
| `LocalPort::is_alive(self, env) → bool` | Check if port is alive | `enif_is_port_alive` |
| `port_command(env: impl CallingEnv, &LocalPort, msg: impl Term) → bool` | Send command to port (free verb; NULL msg_env) | `enif_port_command` |

### Internals

Mirrors `Pid`: a local port is an immediate (`LocalPort`, validated via
`enif_get_local_port` / `enif_whereis_port`, lifetime-free and `Copy`), an
external port is heap-boxed (`Port<'a>`, env-bound). `enif_port_command` and
`enif_is_port_alive` require an internal port, so they take `&LocalPort`.

### Not Exposed

`enif_is_port` is the associated fn `Port::is_port(env, term)`; type
identification also goes through `enif_term_type` in `resolve`.

---


## Fun

### NIF C Functions

| Function | Signature |
|---|---|
| `enif_is_fun` | `(env, term) → int` |

### Otter API

```rust
struct Fun<'id> { raw_term: RawTerm, _id: Invariant<'id> }
```

Only `is_fun(env, term) → bool`. The NIF API provides no way to inspect or invoke
a fun from C. `Fun` exists so that `TypedTerm::Fun` can carry the value through —
the NIF can receive a fun as an argument and pass it back to Erlang unchanged (or
to `apply`).

### Not Exposed

`enif_is_fun` (handled by `enif_term_type`).

---


## Reference

### NIF C Functions

| Function | Signature |
|---|---|
| `enif_make_ref` | `(env) → ERL_NIF_TERM` |
| `enif_is_ref` | `(env, term) → int` |

### Otter API

```rust
struct Reference<'id> { raw_term: RawTerm, _id: Invariant<'id> }
```

| Method | Does | Calls |
|---|---|---|
| `new(env) → Reference<'id>` | Create a unique reference | `enif_make_ref` |
| `is_ref(env, term) → bool` | Type predicate | `enif_is_ref` |

### Internals

References are unique opaque values. The main operation is creation. `Reference`
carries its own `PartialEq`/`Ord` (via `enif_is_identical`/`enif_compare`).

### Not Exposed

`enif_is_ref` (handled by `enif_term_type`).

---


## Coverage Summary

Types that are fully covered have all commonly useful C functions exposed.
"Not exposed" items are either redundant with `enif_term_type` (the `is_*`
predicates), variadic (cannot be called from Rust), or intentionally omitted
per design.

| Type | Create | Inspect | Modify | Iterate | Encode/Decode |
|---|---|---|---|---|---|
| Atom | `intern`, `try_existing` | `name` | — | — | yes |
| Binary | `from_bytes`, `BinaryBuf` | `as_bytes`, `try_str`, `len` | `sub` | — | yes |
| Bitstring | — | `is_binary`, `to_binary` | — | — | yes (pass-through) |
| Integer | `from_i64`, `from_u64`, `from_bigint` | `to_i64`, `to_u64`, `to_bigint` | — | — | yes |
| Float | `from_f64` | `to_f64` | — | — | yes |
| List | `from_terms`, `from_str`, `cons` | `node`, `iter`, `try_string`, `len`, `is_empty`, `reverse` | — | `iter()` | yes |
| Tuple | `from_terms` | `with_elements` → `TupleView` (`len`, index, iter) | — | — | yes |
| Map | `new` | `get`, `size` | `put`, `update`, `remove` | `iter` | yes |
| Pid | `self_`, `whereis` | `to_local`, `is_alive` | — | — | yes |
| Port | `whereis` | `to_local`, `is_alive` | `port_command` (free verb) | — | yes |
| Fun | — | — | — | — | yes (pass-through) |
| Reference | `new` | — | — | — | yes |

> Bignum methods (`to_bigint`/`from_bigint`) require the `bigint` feature.
> Sends (the four free verbs `send_copy`/`send_move` ± `_from` for caller
> attribution) and `serialize`/`deserialize` are not shown here. The
> owned-env messaging tier (`OwnedEnvArena`/`OwnedEnv`/`OwnedEnvTerm`) and the env
> spine (`Env`/`Term` traits, env kinds, `Raised`, `CallingEnv`) live in `mod.rs`
> and `ops.rs` — see `otter/DESIGN.md` Layers 3–4.
