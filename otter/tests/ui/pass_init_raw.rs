// With the `raw` feature, `init!` accepts the tier-2 `_raw` lifecycle
// callbacks and generates the load/upgrade/unload wrappers that hand the user
// the library's `priv_data` void*. This pins that the generated raw code
// compiles. Requires `--features raw` (gated in codegen_ui.rs).

use std::ffi::c_void;

#[otter::nif]
fn f(_env: otter::env::Env) -> otter::types::Atom {
    unreachable!()
}

fn on_load(_env: otter::env::Env, _priv: &mut *mut c_void, _info: otter::term::Term) -> bool {
    true
}

fn on_upgrade(
    _env: otter::env::Env,
    _priv: &mut *mut c_void,
    _old: &mut *mut c_void,
    _info: otter::term::Term,
) -> bool {
    true
}

fn on_unload(_env: otter::env::Env, _priv: *mut c_void) {}

otter::init!("m", [f],
    load_raw = on_load,
    upgrade_raw = on_upgrade,
    unload_raw = on_unload);

fn main() {}
