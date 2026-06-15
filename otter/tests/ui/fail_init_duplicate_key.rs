// `init!` rejects a duplicate keyword argument.

#[otter::nif]
fn f(_env: otter::env::Env) -> otter::types::Atom {
    unreachable!()
}

fn on_load(_env: otter::env::Env, _info: otter::term::Term) -> bool {
    true
}

otter::init!("m", [f], load = on_load, load = on_load);

fn main() {}
