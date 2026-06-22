// `init!` rejects an unknown keyword argument.

#[otter::nif]
fn f(_env: otter::types::CallEnv) -> otter::types::Atom {
    unreachable!()
}

fn on_load(_env: otter::types::InitEnv, _info: otter::types::AnyTerm) -> bool {
    true
}

otter::init!("m", [f], frobnicate = on_load);

fn main() {}
