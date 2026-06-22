//! Write Erlang NIFs in Rust — a direct, honest mapping of the Erlang NIF C ABI
//! into safe Rust types, with compile-time lifetime safety and no hidden magic.
//!
//! # Design principle
//!
//! **Writing a NIF with otter should feel like working directly with Erlang. If
//! an Erlang programmer would not recognize a concept, it does not belong here.**
//!
//! The surface is terms, envs, resources, and the `enif_*` operations on them —
//! named the way the BEAM names them — and registration is explicit. Anything an
//! Erlang programmer would not recognize is left out.
//!
//! # The mental model: branded envs and lazy terms
//!
//! Two ideas carry the whole API.
//!
//! **A term is branded by the env that produced it.** [`Env<'id>`](types::Env)
//! is a sealed trait, and the lifetime `'id` is a *generative brand* — a fresh,
//! non-escaping marker minted for each entry point. Every term type carries that
//! brand (`Integer<'id>`, `Binary<'id>`, …), so a term from one env cannot be
//! used with another; the compiler rejects it. The BEAM treats cross-env terms
//! as undefined behavior, and this construction makes that mistake impossible to
//! write. (This is the [`GhostCell`]/[branded-types] technique.)
//!
//! **A term is lazy.** Construction is always free — every concrete term is one
//! machine word plus a zero-sized brand marker. Nothing is read off the BEAM
//! heap until you ask for it with an env-passing accessor: `bin.as_bytes(env)`,
//! `i.to_i64(env)`, `map.get(env, key)`.
//!
//! Because the term carries only the brand and not the env, **operations take
//! the env explicitly**:
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
//! # A guided tour
//!
//! The crate is organized into the following logical sections.
//!
//! ## The env/term spine — [`types`]
//!
//! [`types`] is the core. It defines the [`Env`](types::Env) trait and its
//! concrete *kinds* — [`CallEnv`](types::CallEnv) (a NIF call),
//! [`InitEnv`](types::InitEnv) (`load`/`upgrade`),
//! [`CallbackEnv`](types::CallbackEnv) (resource callbacks),
//! [`DeinitEnv`](types::DeinitEnv) (`unload`), and
//! [`OwnedEnv`](types::OwnedEnv) (the process-independent arena). It defines the
//! [`Term`](types::Term) trait, the three levels of term resolution
//! ([`AnyTerm`](types::AnyTerm) → [`TypedTerm`](types::TypedTerm) → concrete
//! types), the [`OwnedEnvArena`](types::OwnedEnvArena) for building messages off
//! a process, the four message [send verbs](types#functions), and the
//! [`Raised`](types::Raised) exception witness.
//!
//! ## Term types — [`types::terms`]
//!
//! One concrete type per Erlang type: [`Atom`](types::Atom),
//! [`Integer`](types::Integer), [`Float`](types::Float),
//! [`Binary`](types::Binary)/[`Bitstring`](types::Bitstring),
//! [`List`](types::List), [`Tuple`](types::Tuple), [`Map`](types::Map),
//! [`Pid`](types::Pid), [`Port`](types::Port),
//! [`Reference`](types::Reference), and [`Fun`](types::Fun). Each is lazy and
//! one word wide; accessors pull data out on demand.
//!
//! ## Owned buffers — [`BinaryBuf`](types::BinaryBuf)
//!
//! A growable, owned byte buffer (a `Vec<u8>` model) that you fill on a worker
//! thread with no env in hand, then hand to the BEAM as a binary in one move.
//!
//! ## Codecs — [`codec`]
//!
//! [`Encoder`](codec::Encoder)/[`Decoder`](codec::Decoder) convert between terms
//! and native Rust types: integers, floats, `bool`, `str`/`String`, tuples,
//! `Vec<T>`, `HashMap<K, V>`. A failed decode on a NIF argument becomes `badarg`;
//! a failed encode on a return becomes `badret`.
//!
//! ## Resources and per-build state — [`resource`], [`priv_data`]
//!
//! [`resource`] gives you [`ResourceArc<T>`](resource::ResourceArc), a
//! refcounted handle to a Rust value the BEAM can hold and hand back. [`priv_data`]
//! is the per-build private-data slot and the resource-type registry.
//!
//! ## Runtime services — [`time`], [`system`], [`select`], [`alloc`]
//!
//! [`time`] (BEAM monotonic clock), [`system`] (thread-type introspection),
//! [`select`] (I/O event multiplexing), and [`alloc`] (route Rust's global
//! allocator through the BEAM allocator).
//!
//! ## Macros — [`nif!`](macro@nif), [`init!`](macro@init), [`atom!`]
//!
//! [`nif!`](macro@nif) marks a Rust function as a NIF; [`init!`](macro@init)
//! declares the module entry point, the NIF table, the pre-interned atoms, and
//! the resource types; [`atom!`] retrieves a pre-interned atom.
//!
//! # Quickstart
//!
//! ```no_run
//! use otter::types::CallEnv;
//!
//! // A NIF that adds two integers, decoded straight to `i64` and encoded back.
//! #[otter::nif]
//! fn add(_env: CallEnv<'_>, a: i64, b: i64) -> i64 {
//!     a + b
//! }
//!
//! // Declare the module: name, NIF table, and any atoms to pre-intern.
//! otter::init!("my_nif", [add], atoms = [ok, error]);
//! ```
//!
//! # Cargo features
//!
//! All features are off by default; otter always binds NIF 2.17 (OTP 26).
//!
//! | Feature | Effect |
//! |---|---|
//! | `nif_2_18` | Enable the NIF 2.18 (OTP 29) additions. |
//! | `bigint` | Pull in the optional `num-bigint` dependency (compiled only when this feature is enabled) and add arbitrary-precision integers: `types::BigInt` (a re-export of `num_bigint::BigInt`), its `Encoder`/`Decoder`, and `Integer::to_bigint`/`from_bigint`. |
//! | `raw` | Expose the raw, all-`unsafe` [`enif_ffi`] crate as the escape hatch, widen the brand-bridging term constructors to `pub`, and accept the `_raw` lifecycle callbacks in [`init!`](macro@init). |
//!
//! # Core safety invariant: no cross-build ABI assumptions
//!
//! Erlang upgrades a running system in place: a second build of a NIF library
//! can load beside the first and inherit its live state (resources, `priv_data`).
//! The two builds may come from different compilers and allocators and need not
//! be byte-identical source. **The upgrade boundary is a foreign-ABI boundary.**
//!
//! Therefore, outside the `raw` feature, otter never assumes across that boundary
//! that allocators or drop glue are compatible, that std datatypes share a layout,
//! or that identical source compiles to a compatible layout. The safe path holds
//! by construction. Code that relies on cross-build ABI compatibility belongs only
//! behind `raw`, where the caller takes responsibility. The full treatment is in
//! the project's `docs/UPGRADE.md`.
//!
//! [`GhostCell`]: https://plv.mpi-sws.org/rustbelt/ghostcell/
//! [branded-types]: https://docs.rs/generativity
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
