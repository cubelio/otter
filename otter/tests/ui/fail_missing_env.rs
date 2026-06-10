// A NIF with no arguments at all must fail with a clear message
// pointing at the missing env. The macro requires every NIF to
// declare `Env` as its first argument.

#[otter::nif]
fn empty() -> otter::types::Atom {
    unreachable!()
}

fn main() {}
