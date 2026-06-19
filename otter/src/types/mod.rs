pub mod terms;

pub use terms::*;


use core::marker::PhantomData;

mod sealed {
    pub trait Sealed {}
}

pub type Invariant<'id> = PhantomData<*mut &'id ()>;

// --- Env ---

pub(crate) type RawEnv = *mut enif_ffi::Env;

pub trait Env<'id>: Copy + sealed::Sealed {
    fn raw_env(&self) -> RawEnv;
}

/// Run `f` with a freshly branded handle to a process-bound environment — the
/// env the VM hands to a NIF call or a `load`/`upgrade`/`unload` callback. The
/// `for<'id>` bound mints a unique brand per call, so terms built inside `f` are
/// confined to it and cannot escape or mix with another env's terms. `R` is
/// brand-free by construction, which is exactly the raw `Term`/status word the C
/// ABI hands back.
///
/// # Safety
/// `raw` must be the live environment pointer the VM supplied for this callback,
/// used only for the duration of `f`.
pub unsafe fn with_env<R>(raw: RawEnv, f: impl for<'id> FnOnce(AnyEnv<'id>) -> R) -> R {
    f(AnyEnv { raw_env: raw, _id: PhantomData })
}

#[derive(Clone, Copy)]
pub struct AnyEnv<'id> {
    raw_env: RawEnv,
    _id: Invariant<'id>,
}

impl<'id> sealed::Sealed for AnyEnv<'id> {}

impl<'id> Env<'id> for AnyEnv<'id> {
    fn raw_env(&self) -> RawEnv {
        self.raw_env
    }
}

// --- Term ---

pub(crate) type RawTerm = enif_ffi::Term;

pub trait Term<'id>: sealed::Sealed {
    fn raw_term(self) -> RawTerm;
}

pub trait FreeTerm: for<'id> Term<'id> {}

#[derive(Clone, Copy)]
pub struct AnyTerm<'id> {
    raw_term: RawTerm,
    _id: Invariant<'id>,
}

impl<'id> AnyTerm<'id> {
    pub(crate) fn wrap(raw_term: RawTerm, _env: impl Env<'id>) -> Self {
        Self { raw_term, _id: PhantomData }
    }
}

impl sealed::Sealed for AnyTerm<'_> {}

impl<'id> Term<'id> for AnyTerm<'id> {
    fn raw_term(self) -> RawTerm {
        self.raw_term
    }
}

#[derive(Clone, Copy)]
pub struct AnyFreeTerm {
    raw_term: RawTerm,
}

impl AnyFreeTerm {
    pub(crate) fn wrap(raw_term: RawTerm) -> Self {
        Self { raw_term }
    }
}

impl sealed::Sealed for AnyFreeTerm {}

impl Term<'_> for AnyFreeTerm {
    fn raw_term(self) -> RawTerm {
        self.raw_term
    }
}

impl FreeTerm for AnyFreeTerm {}


#[derive(Clone, Copy)]
pub struct OwnedEnvTerm {
    env: RawEnv,
    version: usize,
    term: RawTerm,
}

pub struct OwnedEnvArena {
    env: RawEnv,
    version: usize,
    is_dirty: bool,
}

impl OwnedEnvArena {
    pub fn new() -> Self {
        let env = unsafe { enif_ffi::alloc_env() };
        assert!(!env.is_null(), "enif_alloc_env returned null");
        OwnedEnvArena { env, version: 0, is_dirty: false }
    }

    /// Drop all stored terms and wipe the env heap, in lockstep, for reuse.
    pub fn clear(&mut self) {
        unsafe { enif_ffi::clear_env(self.env) };
        self.version = self.version.wrapping_add(1);
        self.is_dirty = false;
    }

    pub fn copy_in<'a>(&mut self, term: impl Term<'a>) -> OwnedEnvTerm {
        assert!(!self.is_dirty);
        self.export(unsafe { enif_ffi::make_copy(self.env, term.raw_term()) })
    }

    pub fn copy_out<'a>(&self, oterm: OwnedEnvTerm, env: impl Env<'a>) -> AnyTerm<'a> {
        assert!(!self.is_dirty);
        let remote_term = unsafe { enif_ffi::make_copy(env.raw_env(), self.import(oterm)) };
        AnyTerm { raw_term: remote_term, _id: PhantomData }
    }

    fn export(&self, raw_term: RawTerm) -> OwnedEnvTerm {
        assert!(!self.is_dirty);
        OwnedEnvTerm { env: self.env, version: self.version, term: raw_term }
    }

    fn import(&self, oterm: OwnedEnvTerm) -> RawTerm {
        assert!(!self.is_dirty);
        assert!(self.env == oterm.env);
        assert!(self.version == oterm.version);
        oterm.term
    }

    pub fn run<R>(&mut self, f: impl for<'id> FnOnce(OwnedEnv<'_, 'id>) -> R) -> R {
        assert!(!self.is_dirty);
        f(OwnedEnv { owner: self, _id: PhantomData })
    }
}

impl Drop for OwnedEnvArena {
    fn drop(&mut self) {
        unsafe { enif_ffi::free_env(self.env) };
    }
}


/// A branded handle to a process-independent environment, handed to the closure
/// by [`OwnedEnvArena::run`]. It is an [`Env`] like any other kind — generic verbs
/// take `impl Env<'id>` — and additionally carries the owned-env-specific verbs
/// `export`/`import` for moving terms across the closure boundary.
#[derive(Clone, Copy)]
pub struct OwnedEnv<'a, 'id> {
    owner: &'a OwnedEnvArena,
    _id: Invariant<'id>,
}

impl<'a, 'id> OwnedEnv<'a, 'id> {
    pub fn export(self, term: impl Term<'id>) -> OwnedEnvTerm {
        self.owner.export(term.raw_term())
    }

    pub fn import(self, oterm: OwnedEnvTerm) -> AnyTerm<'id> {
        AnyTerm { raw_term: self.owner.import(oterm), _id: PhantomData }
    }
}

impl<'a, 'id> sealed::Sealed for OwnedEnv<'a, 'id> {}

impl<'a, 'id> Env<'id> for OwnedEnv<'a, 'id> {
    fn raw_env(&self) -> RawEnv {
        self.owner.env
    }
}


/// Send the stored term at `index` to `pid`, stealing this env's heap (O(1)),
/// then clearing it for reuse. A free verb: the env and pid are ingredients,
/// not owners of the operation.
pub fn send(pid: &LocalPid, env: &mut OwnedEnvArena, term: OwnedEnvTerm) -> bool {
    assert!(!env.is_dirty);
    let msg = env.import(term);
    let ok = unsafe { enif_ffi::send(std::ptr::null_mut(), &pid.pid, env.env, msg) != 0 };
    env.is_dirty = ok;
    ok
}

#[cfg(test)]
mod brand_tests {
    use super::*;

    // Ties an env and a term to ONE brand: only a term from this env type-checks.
    fn use_in<'id>(_env: AnyEnv<'id>, t: AnyTerm<'id>) -> RawTerm {
        t.raw_term()
    }

    // Two independently branded envs are distinct: a term from env 1 used with
    // env 2's brand is rejected at compile time — the 1:1 brand↔identity
    // guarantee. The same-brand call compiles; uncomment the cross-brand line to
    // re-verify rejection ("borrowed data escapes outside of closure": the two
    // invariant brands from the nested `for<'id>` closures cannot be unified).
    #[allow(dead_code)]
    fn brands_are_distinct() {
        unsafe {
            with_env(std::ptr::null_mut(), |e1| {
                with_env(std::ptr::null_mut(), |e2| {
                    let t1 = AnyTerm::wrap(0, e1);
                    let _ = use_in(e1, t1); // same brand: compiles
                    let _ = e2;
                    // let _ = use_in(e2, t1); // cross brand: rejected
                });
            });
        }
    }
}

#[cfg(test)]
mod owned_env_tests {
    use super::*;

    // Positive: build a term in the scope, save it, return the index. Compiles.
    #[allow(dead_code)]
    fn positive(b: &mut OwnedEnvArena) -> OwnedEnvTerm {
        b.run(|env| {
            let t = AnyTerm::wrap(0, env);
            env.export(t)
        })
    }

    // Escape test — verified to FAIL with "lifetime may not live long enough"
    // (`AnyTerm<'id>` is invariant over `'id`, so the rigid brand can't leak
    // into `run`'s return type `R`). Kept commented so the crate builds; uncomment
    // to re-verify.
    //
    // fn escape(b: &mut OwnedEnvArena) {
    //     let _leaked = b.run(|ctx, env| ctx.import(OwnedEnvTerm { env: b.env, version: b.version, term: 0 }));
    // }
}
