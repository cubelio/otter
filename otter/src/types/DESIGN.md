# Types

Every Erlang term that crosses the NIF boundary is represented by one of the
types in this directory. The two-level resolution model (`Term` → `TypedTerm`)
lets callers choose how much work to pay for: zero cost with `Term`, one
`enif_term_type` call with `TypedTerm`, or full decoding with `Decoder`.


## TypedTerm Resolution

```
NifTerm (u64 machine word)
  │
  ├─ Term<'a>     zero cost, no type check
  │    │
  │    └─ .resolve()  one enif_term_type call
  │         │
  │         └─ TypedTerm<'a>   typed enum (Atom | Bitstring | ... | Tuple)
  │              │
  │              └─ T::decode()   full extraction (e.g. Integer → i64)
```

`TypedTerm` mirrors `ErlNifTermType` exactly — one variant per tag, no
peer variants. The `Bitstring` variant covers both byte-aligned binaries
and sub-byte bitstrings (BEAM treats every binary as a bitstring); call
`Bitstring::is_binary` or `Bitstring::try_into_binary` to refine.
`resolve()` is uniformly one NIF call regardless of variant.


## Lifetime Model

Types that reference data on the BEAM heap carry a lifetime `'a` tied to the
`Env<'a>` that owns that heap. This prevents terms from escaping the NIF call
that created them.

Three types have no lifetime: `Atom`, `Pid`, `Port`. These are global or
carry their identity in the term word itself. Their `Encoder` impls return the
term directly rather than copying.

All other types' `Encoder` impls call `enif_make_copy` to copy the term into
the destination environment.

All concrete types implement `From<T> for TypedTerm<'a>`, enabling `let t: TypedTerm = atom.into()`.
`Term` converts to `TypedTerm` via `From` (calls `resolve()`).


## Codec Traits

```rust
pub trait Encoder {
    fn encode<'a>(&self, env: Env<'a>) -> Term<'a>;
}

pub trait Decoder<'a>: Sized {
    fn decode(term: Term<'a>) -> Result<Self, CodecError>;
}
```

`CodecError` has two variants: `WrongType` and `IntegerOverflow`. The
`#[otter::nif]` macro converts any `CodecError` into a `badarg` exception
automatically.

Every type in this directory implements both traits. `Decoder` accepts only
the matching `TypedTerm` variant and rejects everything else with `WrongType`.

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
struct Atom { term: NifTerm }  // no lifetime — atoms are global
```

| Method | Does | Calls |
|---|---|---|
| `new(env, name) → Option<Atom>` | Create/intern atom from UTF-8 `&str` | `enif_make_new_atom_len` |
| `try_existing(env, name) → Option<Atom>` | Look up without creating | `enif_make_existing_atom_len` |
| `name(self, env) → String` | Read atom's name | `enif_get_atom_length` + `enif_get_atom` |

### Internals

`new` calls `enif_make_new_atom_len` (NIF 2.17) which returns a success/fail
int rather than creating atoms unconditionally. Returns `None` if the atom
table is full. `name` does two calls: first to get the byte length, then to
read into a buffer.

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
struct Binary<'a> { term: NifTerm, env: Env<'a> }
struct Bitstring<'a> { term: NifTerm, env: Env<'a> }
```

| Method | Does | Calls |
|---|---|---|
| `as_bytes(self) → &'a [u8]` | Zero-copy view of binary data | `enif_inspect_binary` |
| `len(self) → usize` | Byte count | `enif_inspect_binary` |
| `is_empty(self) → bool` | Empty check | `enif_inspect_binary` |
| `try_str(self) → Result<&'a str, Utf8Error>` | Zero-copy UTF-8 view | `enif_inspect_binary` + `std::str::from_utf8` |
| `sub(self, pos, len) → Binary<'a>` | Zero-copy sub-binary (panics on OOB) | `enif_make_sub_binary` |
| `from_bytes(env, data) → Binary<'a>` | Allocate and copy bytes onto BEAM heap | `enif_alloc_binary` + `enif_make_binary` |
| `to_term(self, env, safe) → Option<TypedTerm<'a>>` | Deserialize from external binary format | `enif_binary_to_term` |
| `impl Deref<Target=[u8]>` | Auto-coerce to `&[u8]` | `enif_inspect_binary` |
| `impl AsRef<[u8]>` | Trait-based byte access | `enif_inspect_binary` |
| `impl Debug` | `Binary(N bytes)` | `enif_inspect_binary` |

**BinaryBuilder** — growable buffer mirroring `Vec<u8>`:

```rust
struct BinaryBuilder { bin: NifBinary, len: usize, released: bool }
```

| Method | Does | Calls |
|---|---|---|
| `new() → BinaryBuilder` | Empty builder | `enif_alloc_binary(0)` |
| `with_capacity(cap) → BinaryBuilder` | Preallocated builder | `enif_alloc_binary(cap)` |
| `push(&mut self, byte)` | Append one byte, grow if needed | `enif_realloc_binary` |
| `extend_from_slice(&mut self, &[u8])` | Append slice, grow if needed | `enif_realloc_binary` |
| `resize(&mut self, new_len, value)` | Resize and fill new bytes with value | `enif_realloc_binary` |
| `as_slice(&self) → &[u8]` | View written bytes | — |
| `as_mut_slice(&mut self) → &mut [u8]` | Mutable view of written bytes | — |
| `len(&self) → usize` | Bytes written | — |
| `capacity(&self) → usize` | Bytes allocated | — |
| `reserve(&mut self, additional)` | Ensure room for more bytes | `enif_realloc_binary` |
| `finish(self, env) → Binary<'a>` | Shrink to len, finalize | `enif_realloc_binary` + `enif_make_binary` |
| `impl Write` | `write!` and `write_all` support | — |
| `impl Deref<Target=[u8]>` | Auto-coerce to `&[u8]` (written bytes) | — |
| `impl DerefMut` | Auto-coerce to `&mut [u8]` (written bytes) | — |
| `impl AsRef<[u8]>` / `AsMut<[u8]>` | Trait-based byte access | — |
| `impl Extend<u8>` | Iterator-based appending | — |
| `impl Debug` | `BinaryBuilder { len: N, capacity: M }` | — |
| `Drop` | Release if not finalized | `enif_release_binary` |

**TypedTerm methods** (on `TypedTerm<'a>`):

| Method | Does | Calls |
|---|---|---|
| `to_binary(self, env) → Option<Binary<'a>>` | Serialize any term to external binary format | `enif_term_to_binary` + `enif_make_binary` |

### Internals

`as_bytes` calls `enif_inspect_binary` which returns a pointer and size into
the BEAM heap. The returned slice borrows from the environment lifetime `'a`,
so it cannot outlive the NIF call. `BinaryBuilder` mirrors `Vec<u8>`: it
tracks `len` (bytes written) and `capacity` (bytes allocated via
`enif_alloc_binary`) separately. `push` and `extend_from_slice` grow via
`enif_realloc_binary` with amortized doubling. `finish` calls
`enif_realloc_binary` to shrink to exact `len`, then `enif_make_binary` to
transfer ownership to the BEAM. The `Drop` impl calls `enif_release_binary`
if the builder is dropped without finishing, preventing leaks.

`Bitstring` is a pass-through type with no inspection methods (the NIF API
provides none for sub-byte bitstrings). It implements `Encoder`, `Decoder`,
and `Debug`. It exists because `enif_term_type` cannot distinguish binaries
from non-byte-aligned bitstrings; `resolve()` uses `enif_is_binary` to split
them.

### Not Exposed

`enif_inspect_iolist_as_binary` (iolist flattening is a higher-level
operation), `enif_make_new_binary` (one-step alloc+term; BinaryBuilder
covers this with more control).

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
struct Integer<'a> { term: NifTerm, env: Env<'a> }
```

| Method | Does | Calls |
|---|---|---|
| `impl TryFrom<Integer> for i64` | Extract as signed 64-bit | `enif_get_int64` or `enif_get_long` |
| `impl TryFrom<Integer> for u64` | Extract as unsigned 64-bit | `enif_get_uint64` or `enif_get_ulong` |
| `impl TryFrom<Integer> for i128` | Extract as signed 128-bit | tries i64 path, falls back to u64 |
| `from_i64(env, val) → Integer<'a>` | Construct from signed 64-bit | `enif_make_int64` or `enif_make_long` |
| `from_u64(env, val) → Integer<'a>` | Construct from unsigned 64-bit | `enif_make_uint64` or `enif_make_ulong` |

### Internals

The `enif.rs` binding uses platform-conditional compilation. On 64-bit systems,
`enif_get_long`/`enif_make_long` are 64-bit and equivalent to the `_int64`
variants. On 32-bit systems, the explicit `_int64` functions are used instead.

`TryFrom<Integer> for i128` is a Rust-side convenience: it tries the signed
path first; if that fails with overflow (value > i64::MAX), it tries the
unsigned path and converts. This covers the full range of Erlang integers
that fit in 128 bits. Construction uses inherent methods (`from_i64`,
`from_u64`) because `From` cannot accept an `Env` parameter.

Erlang integers are arbitrary precision. Values larger than 2^64 cannot be
extracted by any of these functions and will return `IntegerOverflow`.

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
struct Float<'a> { term: NifTerm, env: Env<'a> }
```

| Method | Does | Calls |
|---|---|---|
| `impl From<Float> for f64` | Extract the float value | `enif_get_double` |
| `from_f64(env, val) → Float<'a>` | Construct from f64 | `enif_make_double` |

### Internals

Erlang floats are IEEE 754 doubles. The C API and otter both use `f64`/`double`
directly. There is no precision loss or conversion.

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
struct List<'a> { term: NifTerm, env: Env<'a> }

enum Node<'a> {
    Nil,
    Cell(Term<'a>, Term<'a>),  // head, tail — unresolved
}
```

| Method | Does | Calls |
|---|---|---|
| `node(self) → Node<'a>` | Decompose into nil or cons cell | `enif_get_list_cell` |
| `iter(self) → ListIterator<'a>` | Iterator over head elements | `enif_get_list_cell` per `next()` |
| `try_string(self) → Result<String, CodecError>` | Extract string as UTF-8 `String` | `enif_get_string_length` + `enif_get_string` |
| `len(self) → Option<usize>` | Element count; `None` for improper lists | `enif_get_list_length` |
| `reverse(self) → Option<List<'a>>` | Reverse a proper list; `None` for improper | `enif_make_reverse_list` |
| `from_terms(env, impl IntoIterator<Item: AsNifTerm<'a>>) → List<'a>` | Construct from iterable | `enif_make_list_from_array` |
| `from_str(env, &str) → List<'a>` | Construct string (list of codepoints) from UTF-8 | `enif_make_string_len` |
| `cons(env, impl AsNifTerm<'a>, impl AsNifTerm<'a>) → List<'a>` | Construct cons cell `[head \| tail]` | `enif_make_list_cell` |

**ListIterator** — yields `Term<'a>` heads, one `enif_get_list_cell` per step:

| Method | Does |
|---|---|
| `next() → Option<Term<'a>>` | Yield next head; `None` when a non-cell tail is reached |
| `tail() → Option<TypedTerm<'a>>` | Terminal value after iteration: `[]` for proper lists, improper tail otherwise |

### Internals

Lists in Erlang are cons cells, and otter mirrors this directly. `node`
returns `Term`s for head and tail — the caller chooses whether to resolve
them. `iter()` builds on this: it yields heads as `Term`s and stops when
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
struct Tuple<'a> { term: NifTerm, env: Env<'a> }
```

| Method | Does | Calls |
|---|---|---|
| `len(self) → usize` | Arity | `enif_get_tuple` |
| `is_empty(self) → bool` | Zero-element check | `enif_get_tuple` |
| `element(self, i) → TypedTerm<'a>` | Element at zero-based index; panics if out of bounds | `enif_get_tuple` |
| `from_terms(env, impl IntoIterator<Item: AsNifTerm<'a>>) → Tuple<'a>` | Construct from iterable | `enif_make_tuple_from_array` |

### Internals

`enif_get_tuple` returns a pointer to the tuple's element array and the arity
in one call. `element` dereferences the pointer at the given offset. The
pointer is valid for the lifetime of the environment.

`element` panics on out-of-bounds access. This is deliberate — an incorrect
index is a programmer error, like indexing past the end of a Rust slice.

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
struct Map<'a> { term: NifTerm, env: Env<'a> }
struct MapIterator<'a> { iter: Box<NifMapIterator>, env: Env<'a>, exhausted: bool }
```

| Method | Does | Calls |
|---|---|---|
| `new(env) → Map<'a>` | Create empty map | `enif_make_new_map` |
| `size(self) → usize` | Key-value pair count | `enif_get_map_size` |
| `get(self, impl AsNifTerm<'a>) → Option<TypedTerm<'a>>` | Look up key | `enif_get_map_value` |
| `put(self, impl AsNifTerm<'a>, impl AsNifTerm<'a>) → Map<'a>` | Insert or replace | `enif_make_map_put` |
| `update(self, impl AsNifTerm<'a>, impl AsNifTerm<'a>) → Option<Map<'a>>` | Update existing key; `None` if absent | `enif_make_map_update` |
| `remove(self, impl AsNifTerm<'a>) → Option<Map<'a>>` | Remove key; `None` if absent | `enif_make_map_remove` |
| `iter(self) → MapIterator<'a>` | Forward iterator over key-value pairs | `enif_map_iterator_create` |

`MapIterator` implements `Iterator<Item = (TypedTerm<'a>, TypedTerm<'a>)>` and `Drop`.

### Internals

Maps are immutable in Erlang. `put`, `update`, and `remove` each return a
new `Map` — the original is unchanged. `update` and `remove` return `Option`
because the C functions signal failure when the key is absent.

`MapIterator` is heap-allocated (`Box<NifMapIterator>`) to pin the C iterator
struct. It starts at the first entry and advances with `enif_map_iterator_next`.
`Drop` calls `enif_map_iterator_destroy`.

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
struct Pid { term: NifTerm }  // no lifetime — pids are self-contained
```

| Method | Does | Calls |
|---|---|---|
| `self_(env) → Pid` | Get calling process PID | `enif_self` |
| `is_alive(self, env) → bool` | Check if process is alive | `enif_is_process_alive` |
| `whereis(env, name) → Option<Pid>` | Look up by registered name | `enif_whereis_pid` |
| `as_nif_pid(self, env) → Option<NifPid>` | Convert to `NifPid` for `OwnedEnv::send`; `None` for distributed pids | `enif_get_local_pid` |

`is_current_process_alive` is exposed on `Env`, not `Pid`.

### Internals

`Pid` has no lifetime because the term word encodes the process identity
directly (for local pids). `as_nif_pid` extracts the `ErlNifPid` struct needed
by `enif_send`. It returns `None` for external (distributed) pids, which
cannot be used with the local send API.

### Not Exposed

`enif_is_pid` (handled by `enif_term_type`).

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
struct Port { term: NifTerm }  // no lifetime
```

| Method | Does | Calls |
|---|---|---|
| `whereis(env, name) → Option<Port>` | Look up by registered name | `enif_whereis_port` |
| `command(self, env, msg) → bool` | Send command to port | `enif_port_command` |

### Internals

Like `Pid`, `Port` carries no lifetime. `command` passes `NULL` for the
options pointer (no options are currently defined by the NIF API).

### Not Exposed

`enif_is_port` (handled by `enif_term_type`), `enif_get_local_port` (not
needed unless interacting with port drivers at the C level),
`enif_is_port_alive` (could be added if needed).

---


## Fun

### NIF C Functions

| Function | Signature |
|---|---|
| `enif_is_fun` | `(env, term) → int` |

### Otter API

```rust
struct Fun<'a> { term: NifTerm, env: Env<'a> }
```

No methods. The NIF API provides no way to inspect or invoke a fun from C.
`Fun` exists so that `TypedTerm::Fun` can carry the value through — the NIF can
receive a fun as an argument and pass it back to Erlang unchanged.

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
struct Reference<'a> { term: NifTerm, env: Env<'a> }
```

| Method | Does | Calls |
|---|---|---|
| `new(env) → Reference<'a>` | Create a unique reference | `enif_make_ref` |

### Internals

References are unique opaque values. The only operation is creation. Equality
comparison is provided by `TypedTerm`'s `PartialEq` impl (via `enif_is_identical`).

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
| Atom | `new`, `try_existing` | `name` | — | — | yes |
| Binary | `from_bytes`, `BinaryBuilder` | `as_bytes`, `try_str`, `len` | `sub` | — | yes |
| Bitstring | — | — | — | — | yes (pass-through) |
| Integer | `from_i64`, `from_u64` | `TryFrom` for i64/u64/i128 | — | — | yes |
| Float | `from_f64` | `From<Float> for f64` | — | — | yes |
| List | `from_terms`, `from_str`, `cons` | `node`, `iter`, `try_string`, `len`, `reverse` | — | `iter()` | yes |
| Tuple | `from_terms` | `element`, `len` | — | — | yes |
| Map | `new` | `get`, `size` | `put`, `update`, `remove` | `iter` | yes |
| Pid | `self_`, `whereis` | `is_alive`, `as_nif_pid` | — | — | yes |
| Port | `whereis` | — | `command` | — | yes |
| Fun | — | — | — | — | yes (pass-through) |
| Reference | `new` | — | — | — | yes |
