//! The concrete Erlang term types — one per BEAM type tag.
//!
//! Each type is a lazy, one-machine-word handle: construction is free and data
//! is read from the BEAM heap only on demand through an env-passing accessor.
//! Most carry their env's brand `'id` ([`Integer`], [`Float`], [`Binary`],
//! [`Bitstring`], [`List`], [`Tuple`], [`Map`], [`Pid`], [`Port`],
//! [`Reference`], [`Fun`]) and so cannot escape the env that produced them; the
//! immediate types — [`Atom`], [`LocalPid`], [`LocalPort`] — carry no brand
//! ([`FreeTerm`](crate::types::FreeTerm)) and are valid in any env.
//!
//! Each concrete type implements `PartialEq`/`Eq` via `enif_is_identical` (`=:=`,
//! so `1` and `1.0` are *not* equal) and `PartialOrd`/`Ord` via `enif_compare`
//! (Erlang term order, which *does* rank `1` and `1.0` equal) — the one exception
//! is [`TupleView`], which deliberately offers neither.
//!
//! See [`TypedTerm`](crate::types::TypedTerm) for resolving a term of unknown
//! type to its concrete variant.
pub mod atom;
pub mod binary;
pub mod float;
pub mod fun;
pub mod integer;
pub mod list;
pub mod map;
pub mod pid;
pub mod port;
pub mod reference;
pub mod tuple;

pub use atom::{Atom, AtomError};
pub use binary::{Binary, Bitstring};
pub use float::Float;
pub use fun::Fun;
pub use integer::Integer;
pub use list::{List, ListIterator, Node};
pub use map::{Map, MapIterator};
pub use pid::{LocalPid, Pid};
pub use port::{LocalPort, Port};
pub use reference::Reference;
pub use tuple::{Tuple, TupleView};
