use std::sync::atomic::{AtomicUsize, Ordering};

use crate::codec::{CodecError, Decoder, Encoder};
use crate::env::Env;
use crate::sys::NifTerm;
use crate::term::Term;

/// An Erlang atom.
///
/// Atoms are tagged immediates encoding an index into the BEAM's global atom
/// table. The table never shrinks and lives for the life of the VM, so `Atom`
/// has no lifetime and is `Copy`.
#[derive(Clone, Copy)]
pub struct Atom {
    pub(crate) term: NifTerm,
}

impl Atom {
    /// Intern an atom in the BEAM's global atom table.
    ///
    /// Returns `None` if `name` is not valid UTF-8 or the atom table is full.
    ///
    /// # Safety against atom-table exhaustion
    ///
    /// The atom table is global, has a fixed maximum size (default
    /// 1,048,576), and **never shrinks** — every interned name persists for
    /// the life of the VM. A NIF that calls `intern` with attacker-influenced
    /// input (a network protocol field, a binary parsed from a file, etc.)
    /// turns each unique string into a permanent atom-table entry, which is
    /// a well-known BEAM denial-of-service vector that crashes the entire
    /// VM, not just the NIF.
    ///
    /// **Never call `intern` on untrusted input.** For input handling, use
    /// [`Atom::try_existing`] and treat `None` as "atom not recognized,
    /// reject input." Reserve `intern` for compile-time-known names — and
    /// even there, prefer the [`declare_atoms!`](crate::declare_atoms)
    /// macro, which interns each name exactly once at NIF load and
    /// retrieves it thereafter as a single atomic load.
    ///
    /// Wraps `enif_make_atom_len`.
    pub fn intern(env: Env<'_>, name: &str) -> Option<Atom> {
        let term = unsafe {
            crate::wrapper::atom::make_atom(env.as_ptr(), name.as_bytes())
        }?;
        Some(Atom { term })
    }

    /// Look up an existing atom by name without creating it.
    ///
    /// Returns `None` if no atom with this name exists in the atom table.
    /// Use this instead of [`Atom::intern`] when looking up atoms from
    /// untrusted input — `None` means "not a known name, reject."
    /// Wraps `enif_make_existing_atom_len`.
    pub fn try_existing(env: Env<'_>, name: &str) -> Option<Atom> {
        let term = unsafe {
            crate::wrapper::atom::make_existing_atom(env.as_ptr(), name.as_bytes())
        }?;
        Some(Atom { term })
    }

    pub(crate) fn from_raw(term: NifTerm) -> Atom {
        Atom { term }
    }

    /// Return the atom's name as a `String`.
    ///
    /// Infallible: we request `ERL_NIF_UTF8` from `enif_get_atom`, so the BEAM
    /// encodes the name as UTF-8 for us.
    pub fn name(self, env: Env<'_>) -> String {
        let mut buf = Vec::new();
        unsafe { crate::wrapper::atom::get_atom_into(env.as_ptr(), self.term, &mut buf) };
        // SAFETY: BEAM guarantees UTF-8 when requested with NifCharEncoding::Utf8
        unsafe { String::from_utf8_unchecked(buf) }
    }
}

// ---------------------------------------------------------------------------
// StaticAtom — pre-declared atom with eager initialization
// ---------------------------------------------------------------------------

/// A pre-declared atom that is initialized once at NIF load time and
/// retrieved thereafter as a single atomic load.
///
/// Use via the [`declare_atoms!`], [`init_atoms!`], and [`atom!`] macros.
///
/// ```ignore
/// otter::declare_atoms![ok, error, not_found];
///
/// fn on_load(env: Env, _load_info: TypedTerm) -> bool {
///     otter::init_atoms!(env);
///     true
/// }
///
/// // in a NIF:
/// let ok = otter::atom![ok];
/// ```
pub struct StaticAtom {
    name: &'static str,
    term: AtomicUsize,
}

impl StaticAtom {
    /// Create a new uninitialized `StaticAtom`. Must call [`init`](Self::init)
    /// before [`get`](Self::get).
    pub const fn new(name: &'static str) -> Self {
        Self {
            name,
            term: AtomicUsize::new(0),
        }
    }

    /// Initialize this atom by interning it in the BEAM atom table.
    /// Must be called from the NIF load callback.
    pub fn init(&self, env: Env<'_>) {
        let atom = Atom::intern(env, self.name)
            .expect("StaticAtom::init: failed to create atom");
        self.term.store(atom.term, Ordering::Relaxed);
    }

    /// Retrieve the cached atom. Returns an `Atom` with no lookup cost —
    /// just a single atomic load.
    ///
    /// # Panics
    ///
    /// Panics if called before [`init`](Self::init).
    #[inline]
    pub fn get(&self) -> Atom {
        let term = self.term.load(Ordering::Relaxed);
        assert!(term != 0, "StaticAtom::get called before init");
        Atom { term }
    }
}

// SAFETY: StaticAtom is just an atomic integer + a static string.
unsafe impl Sync for StaticAtom {}

impl PartialEq for Atom {
    fn eq(&self, other: &Self) -> bool {
        unsafe { crate::wrapper::term::is_identical(self.term, other.term) }
    }
}

impl Eq for Atom {}

impl PartialOrd for Atom {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Atom {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let c = unsafe { crate::wrapper::term::compare(self.term, other.term) };
        c.cmp(&0)
    }
}

impl std::fmt::Debug for Atom {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Atom")
    }
}

impl Encoder for Atom {
    fn encode<'a>(&self, env: Env<'a>) -> Term<'a> {
        // Atoms are global tagged immediates — valid in any environment.
        Term::new(env, self.term)
    }
}

impl<'a> Decoder<'a> for Atom {
    fn decode(term: Term<'a>) -> Result<Self, CodecError> {
        if unsafe { crate::wrapper::check::is_atom(term.env.as_ptr(), term.term) } {
            Ok(Atom::from_raw(term.term))
        } else {
            Err(CodecError::WrongType)
        }
    }
}
