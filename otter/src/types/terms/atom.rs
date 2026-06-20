use std::ffi::{c_char, c_uint};
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::types::sealed::Sealed;
use crate::types::{Env, FreeTerm, RawTerm, Term};

/// An Erlang atom.
///
/// Atoms are tagged immediates encoding an index into the BEAM's global atom
/// table. The table never shrinks and lives for the life of the VM, so `Atom`
/// has no brand, is `Copy`, and is valid in any env ([`FreeTerm`]).
#[derive(Clone, Copy)]
pub struct Atom {
    pub(crate) term: RawTerm,
}

impl Atom {
    /// Intern an atom in the BEAM's global atom table (`enif_make_new_atom_len`).
    ///
    /// Returns `None` if `name` is not valid UTF-8 or the atom table is full.
    ///
    /// # Atom-table exhaustion
    ///
    /// The atom table is global, fixed-size, and **never shrinks**. Interning
    /// attacker-influenced input is a well-known BEAM denial-of-service vector
    /// that crashes the whole VM. **Never call `intern` on untrusted input** —
    /// use [`Atom::try_existing`] and treat `None` as "not recognized, reject."
    /// Reserve `intern` for compile-time-known names, preferably declared in the
    /// `atoms = [...]` list of [`init!`](crate::init).
    pub fn intern<'id>(env: impl Env<'id>, name: &str) -> Option<Atom> {
        let mut term: RawTerm = 0;
        let ok = unsafe {
            enif_ffi::make_new_atom_len(
                env.raw_env(),
                name.as_ptr() as *const c_char,
                name.len(),
                &mut term,
                enif_ffi::CharEncoding::Utf8,
            )
        };
        (ok != 0).then_some(Atom { term })
    }

    /// Look up an existing atom by name without creating it
    /// (`enif_make_existing_atom_len`). `None` if no such atom exists. Use this
    /// instead of [`intern`](Atom::intern) on untrusted input.
    pub fn try_existing<'id>(env: impl Env<'id>, name: &str) -> Option<Atom> {
        let mut term: RawTerm = 0;
        let ok = unsafe {
            enif_ffi::make_existing_atom_len(
                env.raw_env(),
                name.as_ptr() as *const c_char,
                name.len(),
                &mut term,
                enif_ffi::CharEncoding::Utf8,
            )
        };
        (ok != 0).then_some(Atom { term })
    }

    pub(crate) fn from_raw(term: RawTerm) -> Atom {
        Atom { term }
    }

    /// Returns `true` if `term` is an atom (`enif_is_atom`).
    pub fn is_atom<'id>(env: impl Env<'id>, term: impl Term<'id>) -> bool {
        unsafe { enif_ffi::is_atom(env.raw_env(), term.raw_term()) != 0 }
    }

    /// The atom's name as a `String` (`enif_get_atom_length` + `enif_get_atom`).
    ///
    /// Infallible for a validated `Atom`: we request `ERL_NIF_UTF8`, so the BEAM
    /// encodes the name as UTF-8 for us.
    pub fn name<'id>(self, env: impl Env<'id>) -> String {
        let mut len: c_uint = 0;
        let ok = unsafe {
            enif_ffi::get_atom_length(env.raw_env(), self.term, &mut len, enif_ffi::CharEncoding::Utf8)
        };
        assert!(ok != 0, "enif_get_atom_length failed for a validated Atom");
        let mut buf = vec![0u8; len as usize + 1];
        let written = unsafe {
            enif_ffi::get_atom(
                env.raw_env(),
                self.term,
                buf.as_mut_ptr() as *mut c_char,
                buf.len() as c_uint,
                enif_ffi::CharEncoding::Utf8,
            )
        };
        assert!(written > 0, "enif_get_atom failed for a validated Atom");
        buf.truncate((written - 1) as usize); // strip null terminator
        // SAFETY: BEAM guarantees UTF-8 when requested with Utf8 encoding.
        unsafe { String::from_utf8_unchecked(buf) }
    }
}

impl PartialEq for Atom {
    fn eq(&self, other: &Self) -> bool {
        unsafe { enif_ffi::is_identical(self.term, other.term) != 0 }
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
        let c = unsafe { enif_ffi::compare(self.term, other.term) };
        c.cmp(&0)
    }
}

impl std::fmt::Debug for Atom {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Atom")
    }
}

impl Sealed for Atom {}

impl<'id> Term<'id> for Atom {
    fn raw_term(self) -> RawTerm {
        self.term
    }
}

impl FreeTerm for Atom {}

// ---------------------------------------------------------------------------
// StaticAtom — pre-declared atom with eager initialization
// ---------------------------------------------------------------------------

/// A pre-declared atom, interned once at NIF load and retrieved thereafter as a
/// single atomic load.
///
/// Declare atoms in the `atoms = [...]` list of [`init!`](crate::init) and
/// retrieve them with [`atom!`](crate::atom); the generated load scaffolding
/// interns them (and re-interns on hot upgrade). For manual control, construct
/// `StaticAtom`s directly and call [`init`](Self::init) yourself.
pub struct StaticAtom {
    name: &'static str,
    term: AtomicUsize,
}

impl StaticAtom {
    /// Create a new uninitialized `StaticAtom`. Must call [`init`](Self::init)
    /// before [`get`](Self::get).
    pub const fn new(name: &'static str) -> Self {
        Self { name, term: AtomicUsize::new(0) }
    }

    /// Initialize this atom by interning it in the BEAM atom table. Must be
    /// called from a NIF load/upgrade callback.
    pub fn init<'id>(&self, env: impl Env<'id>) {
        let atom = Atom::intern(env, self.name).expect("StaticAtom::init: failed to create atom");
        // Relaxed is sufficient: `init` runs in the load/upgrade callback, which
        // completes before the BEAM publishes the library and dispatches any NIF
        // call. That load barrier supplies the happens-before to every later
        // `get`, so no acquire/release pairing is needed on the atomic itself.
        self.term.store(atom.term, Ordering::Relaxed);
    }

    /// Retrieve the cached atom — a single atomic load, no lookup cost.
    ///
    /// # Panics
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
