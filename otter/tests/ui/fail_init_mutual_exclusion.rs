// `load` and `load_raw` are mutually exclusive. Requires `--features raw`
// (otherwise `load_raw` is rejected by the feature gate first); gated in
// codegen_ui.rs.

use std::ffi::c_void;

#[otter::nif]
fn f(_env: otter::types::CallEnv) -> otter::types::Atom {
    unreachable!()
}

fn on_load(_env: otter::types::InitEnv, _info: otter::types::AnyTerm) -> bool {
    true
}

fn on_load_raw(_env: otter::types::InitEnv, _priv: &mut *mut c_void, _info: otter::types::AnyTerm) -> bool {
    true
}

otter::init!("m", [f], load = on_load, load_raw = on_load_raw);

fn main() {}
