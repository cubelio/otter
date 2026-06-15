// The tier-2 `_raw` callbacks require otter's `raw` feature. Without it, the
// `init!` macro rejects the `load_raw`/`upgrade_raw`/`unload_raw` keys.

use std::ffi::c_void;

#[otter::nif]
fn f(_env: otter::env::Env) -> otter::types::Atom {
    unreachable!()
}

fn on_load(_env: otter::env::Env, _priv: &mut *mut c_void, _info: otter::term::Term) -> bool {
    true
}

otter::init!("m", [f], load_raw = on_load);

fn main() {}
