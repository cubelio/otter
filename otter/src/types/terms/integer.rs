//! [`Integer`] — arbitrary-precision Erlang integers, with `i64`/`u64`
//! accessors and, under the `bigint` feature, full `BigInt` conversion for
//! magnitudes the fixed-width accessors cannot reach.

use core::marker::PhantomData;

#[cfg(feature = "bigint")]
use num_bigint::{BigInt, Sign};

use crate::types::sealed::Sealed;
use crate::types::{Env, Invariant, RawTerm, Term};

/// An Erlang integer. Arbitrary precision — small integers are tagged
/// immediates, large integers (bignums) are heap-allocated on the env.
///
/// Carries only its env's brand `'id`, not the env itself. An integer can be
/// read back only with an env of the same brand (the 1:1 identity guarantee),
/// so the accessors take the env explicitly rather than the term carrying it.
#[derive(Clone, Copy)]
pub struct Integer<'id> {
    raw_term: RawTerm,
    _id: Invariant<'id>,
}

impl<'id> Integer<'id> {
    #[crate::raw]
    pub(crate) fn from_raw(raw_term: RawTerm) -> Self {
        Self { raw_term, _id: PhantomData }
    }

    /// Construct an integer term from an `i64` (`enif_make_int64`).
    pub fn from_i64(env: impl Env<'id>, val: i64) -> Self {
        let raw_term = unsafe { enif_ffi::make_int64(env.raw_env(), val) };
        Integer { raw_term, _id: PhantomData }
    }

    /// Construct an integer term from a `u64` (`enif_make_uint64`).
    pub fn from_u64(env: impl Env<'id>, val: u64) -> Self {
        let raw_term = unsafe { enif_ffi::make_uint64(env.raw_env(), val) };
        Integer { raw_term, _id: PhantomData }
    }

    /// Read back an `i64` (`enif_get_int64`). `None` if the term does not fit in
    /// `i64`. `env` must carry the same brand as this term.
    pub fn to_i64(self, env: impl Env<'id>) -> Option<i64> {
        let mut val: i64 = 0;
        (unsafe { enif_ffi::get_int64(env.raw_env(), self.raw_term, &mut val) } != 0).then_some(val)
    }

    /// Read back a `u64` (`enif_get_uint64`). `None` if the term does not fit in
    /// `u64` (including negatives).
    pub fn to_u64(self, env: impl Env<'id>) -> Option<u64> {
        let mut val: u64 = 0;
        (unsafe { enif_ffi::get_uint64(env.raw_env(), self.raw_term, &mut val) } != 0).then_some(val)
    }

    /// Read this integer as an arbitrary-precision [`BigInt`], including bignums
    /// that exceed `i64`/`u64` — which [`to_i64`](Self::to_i64) /
    /// [`to_u64`](Self::to_u64) cannot reach, since the NIF API has no accessor
    /// beyond `enif_get_int64`/`enif_get_uint64`.
    ///
    /// Total over every integer term: it serializes the term to the external
    /// term format (`enif_term_to_binary`) and reconstructs the value from the
    /// ETF integer encoding. Infallible — the term is already a validated
    /// integer, so it always serializes to one of the four ETF integer tags.
    ///
    /// Requires the `bigint` feature.
    #[cfg(feature = "bigint")]
    #[cfg_attr(docsrs, doc(cfg(feature = "bigint")))]
    pub fn to_bigint(self, env: impl Env<'id>) -> BigInt {
        let buf = crate::types::serialize(env, self)
            .expect("enif_term_to_binary failed on an integer term");
        etf_integer_to_bigint(buf.as_bytes())
    }

    /// Construct an integer term from an arbitrary-precision [`BigInt`],
    /// including values beyond `i64`/`u64`.
    ///
    /// Values within range go straight through `enif_make_int64` /
    /// `enif_make_uint64`; larger magnitudes are emitted as an ETF bignum
    /// (`SMALL_BIG_EXT`/`LARGE_BIG_EXT`) and parsed back with
    /// `enif_binary_to_term`. Infallible.
    ///
    /// Requires the `bigint` feature.
    #[cfg(feature = "bigint")]
    #[cfg_attr(docsrs, doc(cfg(feature = "bigint")))]
    pub fn from_bigint(env: impl Env<'id>, val: &BigInt) -> Self {
        if let Ok(i) = i64::try_from(val) {
            return Self::from_i64(env, i);
        }
        if let Ok(u) = u64::try_from(val) {
            return Self::from_u64(env, u);
        }
        let etf = bigint_to_etf(val);
        let term = crate::types::deserialize(env, &etf, false)
            .expect("enif_binary_to_term failed on otter-authored bignum ETF");
        Integer::from_raw(term.raw_term())
    }
}

// ETF integer tags (external term format §). The version byte 131 prefixes
// every term `enif_term_to_binary` produces.
#[cfg(feature = "bigint")]
const ETF_VERSION: u8 = 131;
#[cfg(feature = "bigint")]
const SMALL_INTEGER_EXT: u8 = 97; // 1 unsigned byte
#[cfg(feature = "bigint")]
const INTEGER_EXT: u8 = 98; // 4 bytes, big-endian, signed
#[cfg(feature = "bigint")]
const SMALL_BIG_EXT: u8 = 110; // n:u8, sign:u8, n bytes little-endian magnitude
#[cfg(feature = "bigint")]
const LARGE_BIG_EXT: u8 = 111; // n:u32 big-endian, sign:u8, n bytes LE magnitude

/// Parse a `BigInt` out of the ETF bytes `enif_term_to_binary` produced for an
/// integer term. The term is a validated integer, so the body is exactly one of
/// the four integer tags; any other shape is a contract violation and panics.
#[cfg(feature = "bigint")]
fn etf_integer_to_bigint(bytes: &[u8]) -> BigInt {
    assert!(
        bytes.len() >= 2 && bytes[0] == ETF_VERSION,
        "malformed ETF from enif_term_to_binary"
    );
    let body = &bytes[1..];
    match body[0] {
        SMALL_INTEGER_EXT => BigInt::from(body[1]),
        INTEGER_EXT => BigInt::from(i32::from_be_bytes([body[1], body[2], body[3], body[4]])),
        SMALL_BIG_EXT => {
            let n = body[1] as usize;
            BigInt::from_bytes_le(etf_sign(body[2]), &body[3..3 + n])
        }
        LARGE_BIG_EXT => {
            let n = u32::from_be_bytes([body[1], body[2], body[3], body[4]]) as usize;
            BigInt::from_bytes_le(etf_sign(body[5]), &body[6..6 + n])
        }
        tag => panic!("enif_term_to_binary of an integer produced unexpected ETF tag {tag}"),
    }
}

/// ETF big sign byte: 0 = non-negative, anything else = negative.
#[cfg(feature = "bigint")]
fn etf_sign(byte: u8) -> Sign {
    if byte == 0 { Sign::Plus } else { Sign::Minus }
}

/// Encode a `BigInt` as a version-prefixed ETF bignum. Caller has already ruled
/// out the i64/u64 fast paths, so the value never fits a smaller integer tag.
#[cfg(feature = "bigint")]
fn bigint_to_etf(val: &BigInt) -> Vec<u8> {
    let (sign, mag) = val.to_bytes_le();
    let sign_byte = if sign == Sign::Minus { 1 } else { 0 };
    let mut etf = Vec::with_capacity(mag.len() + 7);
    etf.push(ETF_VERSION);
    if mag.len() <= u8::MAX as usize {
        etf.push(SMALL_BIG_EXT);
        etf.push(mag.len() as u8);
        etf.push(sign_byte);
    } else {
        etf.push(LARGE_BIG_EXT);
        etf.extend_from_slice(&(mag.len() as u32).to_be_bytes());
        etf.push(sign_byte);
    }
    etf.extend_from_slice(&mag);
    etf
}

impl<'id> Sealed for Integer<'id> {}

impl<'id> Term<'id> for Integer<'id> {
    fn raw_term(self) -> RawTerm {
        self.raw_term
    }
}

impl PartialEq for Integer<'_> {
    fn eq(&self, other: &Self) -> bool {
        unsafe { enif_ffi::is_identical(self.raw_term, other.raw_term) != 0 }
    }
}

impl Eq for Integer<'_> {}

impl PartialOrd for Integer<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Integer<'_> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let c = unsafe { enif_ffi::compare(self.raw_term, other.raw_term) };
        c.cmp(&0)
    }
}

impl std::fmt::Debug for Integer<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Integer")
    }
}
