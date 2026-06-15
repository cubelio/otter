// `init!` rejects an unknown keyword argument. Tier-2 `_raw` callbacks are
// not yet supported and fall through to this same error.

#[otter::nif]
fn f(_env: otter::env::Env) -> otter::types::Atom {
    unreachable!()
}

fn on_load(_env: otter::env::Env, _info: otter::term::Term) -> bool {
    true
}

otter::init!("m", [f], load_raw = on_load);

fn main() {}
