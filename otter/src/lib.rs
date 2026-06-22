// Render the "Available on crate feature X" labels on docs.rs (and any
// `RUSTDOCFLAGS="--cfg docsrs"` nightly doc build). The `feature(doc_cfg)` gate is
// emitted only when `--cfg docsrs` is set, so ordinary stable builds never see it.
// `auto_cfg = false` suppresses rustdoc's automatic pills for *every* `#[cfg]`
// (which would otherwise leak std-internal cfgs like `no_global_oom_handling` and
// platform gates onto our pages); only the explicit `#[doc(cfg(...))]` annotations
// on the feature-gated surface show.
#![cfg_attr(docsrs, feature(doc_cfg))]
#![cfg_attr(docsrs, doc(auto_cfg = false))]
//! ## Overview
//!
//! **Write fast and efficient Erlang NIFs in Rust**
//!
//! `otter` is built from the ground up to give Erlang and Elixir programmers an
//! easy, efficient way to write NIFs in Rust. The design is driven by two
//! foundational principles:
//! 1. **Functionality** — `otter`'s first priority is an API surface that
//! faithfully captures the capabilities of the underlying `erl_nif` C API — and
//! where the safe surface isn't enough, the raw API can be exposed by enabling
//! the `raw` feature.
//! 2. **Speed** — One of the prime motivations for writing NIFs is to achieve
//! speed and memory efficiency not possible in the Erlang VM. `otter`'s
//! datatypes are designed to "pay only for what you need" through controlled
//! layering of `enif_*` calls, with extensive work to employ compile-time
//! constraints rather than runtime ones.
//!
//! ## Quickstart
//!
//! ```no_run
//! use otter::types::CallEnv;
//!
//! // Declare the module: name, NIF table, and any atoms to pre-intern.
//! otter::init!("my_nif", [add], atoms = [ok, error]);
//!
//! // A NIF that adds two integers, decoded straight to `i64` and encoded back.
//! #[otter::nif]
//! fn add(_env: CallEnv<'_>, a: i64, b: i64) -> i64 {
//!     a + b
//! }
//! ```
//!
//! ## Features
//!
//! A few highlights of the surface; the [guided tour](#a-guided-tour) below maps
//! the whole crate.
//!
//! ### Generative env/term lifetimes
//!
//! Each term is branded by the env that produced it. [`Env<'id>`](types::Env) is
//! a sealed trait whose lifetime `'id` is a *generative brand* — a fresh,
//! non-escaping marker minted at every entry point. Every term type carries that
//! brand (`Integer<'id>`, `Binary<'id>`, …), so a term from one env cannot be
//! used with another: the compiler rejects it, with no per-operation runtime
//! check. The BEAM treats a cross-env term as undefined behavior, and this
//! construction makes that mistake impossible to write. (This is the
//! [generativity pattern].)
//!
//! ### Layered term resolution
//!
//! Reading a term through the C API is a cascade: `enif_term_type` and the
//! `enif_is_*` family reveal a type, `enif_get_tuple` hands back elements that
//! are themselves opaque terms, and so on — a full decode can cost many calls
//! into the VM, most of them unnecessary. `otter` mirrors that progression in its
//! types, so you stop at exactly the depth you need: [`AnyTerm`](types::AnyTerm)
//! → [`TypedTerm`](types::TypedTerm) → a concrete term type → a native Rust
//! value.
//!
//! Resolution is lazy. Constructing a term is free — one machine word plus a
//! zero-sized brand marker — and nothing is read off the BEAM heap until an
//! env-passing accessor asks for it:
//!
//! ```no_run
//! # use otter::types::{CallEnv, Atom, Binary, Integer, Map};
//! # fn demo(env: CallEnv<'_>, key_atom: Atom) {
//! // Constructors are associated functions taking an env:
//! let i = Integer::from_i64(env, 42);
//! let b = Binary::from_bytes(env, b"hello");
//! let m = Map::new(env);
//!
//! // Accessors take an env:
//! let n = i.to_i64(env);          // Option<i64>
//! let bytes = b.as_bytes(env);    // &[u8], zero-copy into the BEAM heap
//!
//! // Term inputs are taken as `impl Term<'id>` — pass concrete types directly,
//! // no `.encode()` needed; a term of the wrong brand fails to compile:
//! let m = m.put(env, key_atom, i);
//! # let _ = (n, bytes, m);
//! # }
//! ```
//!
//! ### Honest codecs
//!
//! [`Encoder`](codec::Encoder)/[`Decoder`](codec::Decoder) move between terms and
//! native Rust types — integers, floats, `bool`, `str`/`String`, tuples,
//! `Vec<T>`, `HashMap<K, V>`, and (with the `bigint` feature) arbitrary-precision
//! integers. A decoder never lies: `300` into a `u8` is an `IntegerOverflow`
//! error, not a silently truncated `44`, and a float will not quietly swallow an
//! integer. A failed decode on a NIF argument becomes `badarg`; a failed encode
//! on a return becomes `badret`.
//!
//! ### Faithful Erlang types
//!
//! All eleven term types are present, and they mirror Erlang rather than a
//! convenient subset of it. [`List`](types::List) is a real cons cell
//! (`Nil | Cell(head, tail)`), so improper lists are first-class and a codec
//! rejects a bad tail with a clean error instead of papering over it.
//!
//! ### The full send matrix
//!
//! Message passing is a 2×2 of four free verbs: `send_copy`/`send_move` — copy a
//! live term, or steal an [`OwnedEnvArena`](types::OwnedEnvArena) heap in O(1) —
//! each available plain (NULL caller, off-thread) or `_from` (caller-attributed,
//! in-NIF). You can build a message on a worker thread with no env in hand and
//! move its whole heap into a process in a single send.
//!
//! ### Hot code upgrade without cross-build ABI assumptions
//!
//! Every `otter` module is hot-code-upgradeable. Erlang upgrades a running system
//! in place: a second build of a NIF library can load beside the first and
//! inherit its live state (resources, `priv_data`). The two builds may come from
//! different compilers and allocators and need not be byte-identical source, so
//! **the upgrade boundary is a foreign-ABI boundary.**
//!
//! Outside the `raw` feature, `otter` never assumes across that boundary that
//! allocators or drop glue are compatible, that std datatypes share a layout, or
//! that identical source compiles to a compatible layout; the safe path holds by
//! construction. Code that relies on cross-build ABI compatibility belongs only
//! behind `raw`, where the caller takes responsibility. The full treatment is in
//! the project's `docs/UPGRADE.md`.
//!
//! ### The raw escape hatch
//!
//! Where the safe surface isn't enough, the `raw` feature exposes the
//! all-`unsafe` [`enif_ffi`] crate directly, widens the brand-bridging term
//! constructors to `pub`, and accepts the `_raw` lifecycle callbacks in
//! [`init!`](macro@init) — a deliberate, opt-in boundary where you take
//! responsibility for the ABI. See [Cargo features](#cargo-features).
//!
//! ## A guided tour
//!
//! The crate is organized into a few logical areas:
//!
//! - **The env/term spine** — [`types`] defines the [`Env`](types::Env) trait and
//!   its concrete kinds ([`CallEnv`](types::CallEnv), [`InitEnv`](types::InitEnv),
//!   [`CallbackEnv`](types::CallbackEnv), [`DeinitEnv`](types::DeinitEnv),
//!   [`OwnedEnv`](types::OwnedEnv)), the [`Term`](types::Term) trait and its
//!   resolution levels, the [`OwnedEnvArena`](types::OwnedEnvArena), the four
//!   message [send verbs](types#functions), and the [`Raised`](types::Raised)
//!   exception witness.
//! - **Term types** — [`types::terms`] has one concrete type per Erlang type:
//!   [`Atom`](types::Atom), [`Integer`](types::Integer), [`Float`](types::Float),
//!   [`Binary`](types::Binary)/[`Bitstring`](types::Bitstring),
//!   [`List`](types::List), [`Tuple`](types::Tuple), [`Map`](types::Map),
//!   [`Pid`](types::Pid), [`Port`](types::Port), [`Reference`](types::Reference),
//!   and [`Fun`](types::Fun).
//! - **Owned buffers** — [`BinaryBuf`](types::BinaryBuf), a growable byte buffer
//!   you fill off-thread with no env in hand, then hand to the BEAM in one move.
//! - **Codecs** — [`codec`] holds the
//!   [`Encoder`](codec::Encoder)/[`Decoder`](codec::Decoder) traits and their
//!   impls for the native Rust types.
//! - **Resources and per-build state** — [`resource`] gives
//!   [`ResourceArc<T>`](resource::ResourceArc), a refcounted handle to a Rust
//!   value the BEAM can hold and hand back; [`priv_data`] is the per-build
//!   private-data slot and the resource-type registry.
//! - **Runtime services** — [`time`] (BEAM monotonic clock), [`system`]
//!   (thread-type introspection), [`select`] (I/O event multiplexing), and
//!   [`alloc`] (route Rust's global allocator through the BEAM allocator).
//! - **Macros** — [`nif!`](macro@nif) marks a function as a NIF,
//!   [`init!`](macro@init) declares the module entry point, and [`atom!`]
//!   retrieves a pre-interned atom.
//!
//! ## Cargo features
//!
//! All features are off by default; `otter` always binds NIF 2.17 (OTP 26).
//!
//! | Feature | Effect |
//! |---|---|
//! | `nif_2_18` | Enable the NIF 2.18 (OTP 29) additions. |
//! | `bigint` | Pull in the optional `num-bigint` dependency (compiled only when this feature is enabled) and add arbitrary-precision integers: `types::BigInt` (a re-export of `num_bigint::BigInt`), its `Encoder`/`Decoder`, and `Integer::to_bigint`/`from_bigint`. |
//! | `raw` | Expose the raw, all-`unsafe` [`enif_ffi`] crate as the escape hatch, widen the brand-bridging term constructors to `pub`, and accept the `_raw` lifecycle callbacks in [`init!`](macro@init). |
//!
//! [generativity pattern]: https://arhan.sh/blog/the-generativity-pattern-in-rust/
pub mod types;
pub mod codec;
pub mod resource;
pub mod priv_data;
mod abi;
pub mod alloc;
pub mod time;
pub mod system;
pub mod select;

#[doc(hidden)]
#[path = "__codegen.rs"]
pub mod __codegen;

// The raw, 1:1, all-unsafe `enif_ffi` crate — the escape hatch, available only
// under the `raw` feature. Generated code no longer names this path (it uses
// `__codegen::ffi::*`), and the enif types that appear in otter's own public API
// are re-exported through their modules (`select::{Event, SelectFlags, …}`,
// `types::{TermType, Hash, UniqueInteger}`, `time::*`, `system::SysInfo`,
// `resource::ResourceFlags`), so nothing in the safe surface needs this. (enhance-11)
#[cfg(feature = "raw")]
#[cfg_attr(docsrs, doc(cfg(feature = "raw")))]
pub use enif_ffi;

// enif-ffi's `nif_init!` builds the platform entry point and resolves the
// enif_* table at load. `#[macro_export]` macros aren't reachable through the
// re-exported crate path (`otter::enif_ffi::nif_init!`), so re-export it by name
// into otter's root; the `init!`-generated code invokes `::otter::nif_init!`.
#[doc(hidden)]
pub use enif_ffi::nif_init;

pub use otter_codegen::nif;
pub use otter_codegen::init;
pub use otter_codegen::resource_impl;

// Internal: widen an item to `pub` under the `raw` feature without duplicating
// it. Used within otter to expose selected internals on the raw escape hatch;
// the emitted `cfg(feature = "raw")` resolves against otter's own `raw` feature.
#[doc(hidden)]
pub use otter_codegen::raw;

/// Retrieve an atom pre-declared in the `atoms = [...]` list of
/// [`init!`](crate::init).
///
/// Returns an [`Atom`] via a single atomic load — no hash lookup, no NIF call.
/// The atoms are interned once at NIF load (and re-interned on hot upgrade) by
/// the `init!`-generated scaffolding, so they are ready before any NIF runs.
///
/// ```no_run
/// # use otter::types::{Atom, CallEnv};
/// otter::init!("my_nif", [get_ok], atoms = [ok, error]);
///
/// #[otter::nif]
/// fn get_ok(_env: CallEnv<'_>) -> Atom {
///     otter::atom![ok]
/// }
/// ```
///
/// [`Atom`]: crate::types::Atom
#[macro_export]
macro_rules! atom {
    ($id:ident) => {
        __otter_atoms::$id.get()
    };
}
