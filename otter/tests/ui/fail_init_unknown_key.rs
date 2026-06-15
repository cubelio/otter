// `init!` rejects an unknown keyword argument.

#[otter::nif]
fn f(_env: otter::env::Env) -> otter::types::Atom {
    unreachable!()
}

fn on_load(_env: otter::env::Env, _info: otter::term::Term) -> bool {
    true
}

otter::init!("m", [f], frobnicate = on_load);

fn main() {}
