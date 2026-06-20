// `init!` rejects a duplicate keyword argument.

#[otter::nif]
fn f(_env: otter::types::CallEnv) -> otter::types::Atom {
    unreachable!()
}

fn on_load(_env: otter::types::InitEnv, _info: otter::types::AnyTerm) -> bool {
    true
}

otter::init!("m", [f], load = on_load, load = on_load);

fn main() {}
