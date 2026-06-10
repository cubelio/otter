// A NIF whose return type does not implement `Encoder` must fail with a
// diagnostic that names the missing `Encoder` bound, not a `method not
// found` error deep in the wrapper. The macro routes the return value
// through an `__otter_assert_encoder<T: Encoder>(t: T) -> T` helper
// inside the wrapper specifically to surface this trait-bound error
// cleanly at the user's call site.

use otter::env::Env;

struct NotEncodable;

#[otter::nif]
fn returns_not_encodable(_env: Env) -> NotEncodable {
    NotEncodable
}

fn main() {}
