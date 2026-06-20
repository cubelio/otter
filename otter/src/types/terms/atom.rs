use std::ffi::{c_char, c_uint};
use std::sync::OnceLock;

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
    /// Fails only with [`AtomError::NameTooLong`] when `name` exceeds 255
    /// characters (`MAX_ATOM_CHARACTERS`) — the single reachable failure for a
    /// Rust `&str`. The encoding can never be rejected (a `&str` is always valid
    /// UTF-8), and atom-table exhaustion is *not* reported here: it aborts the VM
    /// (`erts_exit`) before this could return. The error is a plain Rust
    /// condition, never a pending BEAM exception — the env stays clean.
    ///
    /// # Atom-table exhaustion
    ///
    /// The atom table is global, fixed-size, and **never shrinks**; exhausting
    /// it terminates the whole VM. Interning attacker-influenced input is thus a
    /// well-known BEAM denial-of-service vector. **Never call `intern` on
    /// untrusted input** — use [`Atom::try_existing`] and treat `None` as "not
    /// recognized, reject." Reserve `intern` for trusted or compile-time-known
    /// names, preferably declared in the `atoms = [...]` list of
    /// [`init!`](crate::init).
    pub fn intern<'id>(env: impl Env<'id>, name: &str) -> Result<Atom, AtomError> {
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
        // The only way `false` can come back for a valid-UTF-8 `&str`: the name
        // is longer than MAX_ATOM_CHARACTERS. (Bad encoding is impossible; table
        // exhaustion aborts the VM rather than returning.)
        if ok != 0 { Ok(Atom { term }) } else { Err(AtomError::NameTooLong) }
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
// AtomError
// ---------------------------------------------------------------------------

/// Why [`Atom::intern`] could not create an atom.
///
/// A plain Rust error, never a pending BEAM exception (unlike `Raised`): the
/// env is untouched, so the caller is free to recover. The enum has a single
/// variant because, for a Rust `&str`, exactly one failure is reachable —
/// over-length. Invalid encoding cannot occur (a `&str` is always valid UTF-8),
/// and atom-table exhaustion aborts the VM instead of returning. A future
/// bytes-based intern would add a `BadEncoding` variant; `&str` never needs it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AtomError {
    /// The name exceeded `MAX_ATOM_CHARACTERS` (255 characters).
    NameTooLong,
}

impl std::fmt::Display for AtomError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AtomError::NameTooLong => write!(f, "atom name exceeds 255 characters"),
        }
    }
}

impl std::error::Error for AtomError {}

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
    atom: OnceLock<Atom>,
}

impl StaticAtom {
    /// Create a new uninitialized `StaticAtom`. Must call [`init`](Self::init)
    /// before [`get`](Self::get).
    pub const fn new(name: &'static str) -> Self {
        Self { name, atom: OnceLock::new() }
    }

    /// Initialize this atom by interning it in the BEAM atom table. Must be
    /// called from a NIF load/upgrade callback.
    ///
    /// Returns [`AtomError::NameTooLong`] if the name exceeds 255 characters.
    /// Atoms declared in the `atoms = [...]` list of [`init!`](crate::init) are
    /// length-checked at compile time, so this never fails for them — the error
    /// is reachable only through a hand-constructed `StaticAtom`. Calling `init`
    /// more than once is harmless: the name maps to the same VM-global atom, so
    /// the second `set` is a no-op.
    pub fn init<'id>(&self, env: impl Env<'id>) -> Result<(), AtomError> {
        let atom = Atom::intern(env, self.name)?;
        let _ = self.atom.set(atom);
        Ok(())
    }

    /// Retrieve the cached atom — an acquire load plus a field read, no lookup.
    ///
    /// Stores the [`Atom`] itself, so it assumes nothing about the underlying
    /// term representation.
    ///
    /// # Panics
    /// Panics if called before [`init`](Self::init).
    #[inline]
    pub fn get(&self) -> Atom {
        *self.atom.get().expect("StaticAtom::get called before init")
    }
}
