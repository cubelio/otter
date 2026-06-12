use std::ffi::{c_char, c_uint};
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::codec::{CodecError, Decoder, Encoder};
use crate::env::Env;
use crate::sys::{NifCharEncoding, NifTerm};
use crate::term::{Term, AsNifTerm};

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
        env.make_atom(name)
    }

    /// Look up an existing atom by name without creating it.
    ///
    /// Returns `None` if no atom with this name exists in the atom table.
    /// Use this instead of [`Atom::intern`] when looking up atoms from
    /// untrusted input — `None` means "not a known name, reject."
    /// Wraps `enif_make_existing_atom_len`.
    pub fn try_existing(env: Env<'_>, name: &str) -> Option<Atom> {
        env.make_existing_atom(name)
    }

    pub(crate) fn from_raw(term: NifTerm) -> Atom {
        Atom { term }
    }

    /// Return the atom's name as a `String`.
    ///
    /// Infallible: we request `ERL_NIF_UTF8` from `enif_get_atom`, so the BEAM
    /// encodes the name as UTF-8 for us.
    pub fn name(self, env: Env<'_>) -> String {
        env.atom_name(self)
            .expect("enif_get_atom failed for a validated Atom")
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
/// fn on_load(env: Env, _load_info: Term) -> bool {
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

impl<'a> Env<'a> {
    /// Returns `true` if `term` is an atom (`enif_is_atom`).
    pub fn is_atom(self, term: impl AsNifTerm<'a>) -> bool {
        unsafe { crate::enif::is_atom(self.as_ptr(), term.as_nif_term()) != 0 }
    }

    /// Create (or intern) an atom from a string (`enif_make_new_atom_len`).
    ///
    /// Returns `None` if the atom table is full. See [`Atom::intern`] for the
    /// atom-table-exhaustion warning — never call this on untrusted input.
    pub fn make_atom(self, name: &str) -> Option<Atom> {
        let mut term: NifTerm = 0;
        let ok = unsafe {
            crate::enif::make_new_atom_len(
                self.as_ptr(),
                name.as_ptr() as *const c_char,
                name.len(),
                &mut term,
                NifCharEncoding::Utf8,
            )
        };
        if ok != 0 { Some(Atom { term }) } else { None }
    }

    /// Look up an existing atom by name without creating it
    /// (`enif_make_existing_atom_len`). `None` if no such atom exists.
    pub fn make_existing_atom(self, name: &str) -> Option<Atom> {
        let mut term: NifTerm = 0;
        let ok = unsafe {
            crate::enif::make_existing_atom_len(
                self.as_ptr(),
                name.as_ptr() as *const c_char,
                name.len(),
                &mut term,
                NifCharEncoding::Utf8,
            )
        };
        if ok != 0 { Some(Atom { term }) } else { None }
    }

    /// The atom's UTF-8 name as a `String` (`enif_get_atom_length` +
    /// `enif_get_atom`). `None` if `atom` is not an atom.
    pub fn atom_name(self, atom: Atom) -> Option<String> {
        let mut len: c_uint = 0;
        let ok = unsafe {
            crate::enif::get_atom_length(
                self.as_ptr(),
                atom.term,
                &mut len,
                NifCharEncoding::Utf8,
            )
        };
        if ok == 0 {
            return None;
        }
        let mut buf = vec![0u8; len as usize + 1];
        let written = unsafe {
            crate::enif::get_atom(
                self.as_ptr(),
                atom.term,
                buf.as_mut_ptr() as *mut c_char,
                buf.len() as c_uint,
                NifCharEncoding::Utf8,
            )
        };
        if written > 0 {
            buf.truncate((written - 1) as usize); // strip null terminator
            // SAFETY: BEAM guarantees UTF-8 when requested with Utf8 encoding.
            Some(unsafe { String::from_utf8_unchecked(buf) })
        } else {
            None
        }
    }
}

impl<'a> Decoder<'a> for Atom {
    fn decode(term: Term<'a>) -> Result<Self, CodecError> {
        if term.env.is_atom(term) {
            Ok(Atom::from_raw(term.term))
        } else {
            Err(CodecError::WrongType)
        }
    }
}
