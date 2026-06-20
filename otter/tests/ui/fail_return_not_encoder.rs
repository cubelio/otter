// A NIF whose return type does not implement `Encoder` must fail with a
// diagnostic that names the missing `Encoder` bound, not a `method not
// found` error deep in the wrapper. The bound is surfaced by the
// `T: Encoder` signature of `__codegen::encode_result`, through which the
// generated wrapper routes the return value — so the trait-bound error
// lands cleanly at the user's call site.

use otter::types::CallEnv;

struct NotEncodable;

#[otter::nif]
fn returns_not_encodable(_env: CallEnv) -> NotEncodable {
    NotEncodable
}

fn main() {}
