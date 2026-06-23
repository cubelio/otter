use std::collections::HashMap;
use std::os::unix::io::AsRawFd;
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use otter::types::BigInt;
use otter::select::SelectFlags;
use otter::resource::{Resource, ResourceArc};
use otter::types::{
    AnyTerm, Atom, AtomError, Binary, BinaryBuf, CallEnv, CallbackEnv, Env, Float, InitEnv,
    Integer, List, LocalPid, LocalPort, Map, OwnedEnvArena, Raised, Reference, Tuple, TypedTerm,
};

fn atomize_bool(value: bool) -> Atom {
    if value { otter::atom![true_] } else { otter::atom![false_] }
}

// --- hello/0 -----------------------------------------------------------

#[otter::nif]
fn hello(_env: CallEnv) -> Atom {
    otter::atom![world]
}

// --- add/2 --------------------------------------------------------------

#[otter::nif]
fn add<'a>(env: CallEnv<'a>, a: Integer<'a>, b: Integer<'a>) -> Result<Integer<'a>, Raised<'a>> {
    // A bignum argument does not fit i64 — reject it as badarg rather than
    // panicking (which the wrapper would surface as `nif_panicked`).
    let (Some(a), Some(b)) = (a.to_i64(env), b.to_i64(env)) else {
        return env.badarg();
    };
    Ok(Integer::from_i64(env, a + b))
}

// --- echo/1 -------------------------------------------------------------

#[otter::nif]
fn echo<'a>(_env: CallEnv<'a>, val: AnyTerm<'a>) -> AnyTerm<'a> {
    val
}

// --- type_of/1 ----------------------------------------------------------

#[otter::nif]
fn type_of<'a>(env: CallEnv<'a>, val: TypedTerm<'a>) -> Atom {
    match val {
        TypedTerm::Atom(_) => otter::atom![atom],
        TypedTerm::Integer(_) => otter::atom![integer],
        TypedTerm::Float(_) => otter::atom![float],
        TypedTerm::Bitstring(bs) => {
            if bs.is_binary(env) {
                otter::atom![binary]
            } else {
                otter::atom![bitstring]
            }
        }
        TypedTerm::List(_) => otter::atom![list],
        TypedTerm::Tuple(_) => otter::atom![tuple],
        TypedTerm::Map(_) => otter::atom![map],
        TypedTerm::Pid(_) => otter::atom![pid],
        TypedTerm::Port(_) => otter::atom![port],
        TypedTerm::Fun(_) => otter::atom![fun],
        TypedTerm::Reference(_) => otter::atom![reference],
    }
}

// --- reverse_binary/1 ---------------------------------------------------

#[otter::nif]
fn reverse_binary<'a>(env: CallEnv<'a>, bin: Binary<'a>) -> Binary<'a> {
    let bytes = bin.as_bytes(env);
    let mut builder = BinaryBuf::with_capacity(bytes.len());
    for &b in bytes.iter().rev() {
        builder.push(b);
    }
    builder.into_binary(env)
}

// --- etf_encode/1 -------------------------------------------------------

#[otter::nif]
fn etf_encode<'a>(env: CallEnv<'a>, val: AnyTerm<'a>) -> Binary<'a> {
    otter::types::serialize(env, val).expect("serialize").into_binary(env)
}

// --- etf_roundtrip/1 ----------------------------------------------------

#[otter::nif]
fn etf_roundtrip<'a>(env: CallEnv<'a>, val: AnyTerm<'a>) -> AnyTerm<'a> {
    let buf = otter::types::serialize(env, val).expect("serialize");
    otter::types::deserialize(env, buf.as_bytes(), false).expect("deserialize")
}

// --- sum_list/1 ---------------------------------------------------------

#[otter::nif]
fn sum_list<'a>(env: CallEnv<'a>, list: List<'a>) -> Integer<'a> {
    let sum: i64 = list
        .iter(env)
        .filter_map(|raw| match raw.resolve(env) {
            Some(TypedTerm::Integer(i)) => i.to_i64(env),
            _ => None,
        })
        .sum();
    Integer::from_i64(env, sum)
}

// --- test_eq/2 ----------------------------------------------------------

#[otter::nif]
fn test_eq<'a>(_env: CallEnv<'a>, a: TypedTerm<'a>, b: TypedTerm<'a>) -> Atom {
    let result = match (a, b) {
        (TypedTerm::Atom(a), TypedTerm::Atom(b)) => a == b,
        (TypedTerm::Integer(a), TypedTerm::Integer(b)) => a == b,
        (TypedTerm::Float(a), TypedTerm::Float(b)) => a == b,
        (TypedTerm::Bitstring(a), TypedTerm::Bitstring(b)) => a == b,
        (TypedTerm::List(a), TypedTerm::List(b)) => a == b,
        (TypedTerm::Tuple(a), TypedTerm::Tuple(b)) => a == b,
        (TypedTerm::Map(a), TypedTerm::Map(b)) => a == b,
        (TypedTerm::Pid(a), TypedTerm::Pid(b)) => a == b,
        (TypedTerm::Reference(a), TypedTerm::Reference(b)) => a == b,
        _ => false,
    };
    atomize_bool(result)
}

// --- test_ord/2 ---------------------------------------------------------

#[otter::nif]
fn test_ord<'a>(_env: CallEnv<'a>, a: TypedTerm<'a>, b: TypedTerm<'a>) -> Atom {
    use std::cmp::Ordering;
    let ord = match (a, b) {
        (TypedTerm::Atom(a), TypedTerm::Atom(b)) => a.cmp(&b),
        (TypedTerm::Integer(a), TypedTerm::Integer(b)) => a.cmp(&b),
        (TypedTerm::Float(a), TypedTerm::Float(b)) => a.cmp(&b),
        (TypedTerm::Bitstring(a), TypedTerm::Bitstring(b)) => a.cmp(&b),
        (TypedTerm::List(a), TypedTerm::List(b)) => a.cmp(&b),
        (TypedTerm::Tuple(a), TypedTerm::Tuple(b)) => a.cmp(&b),
        (TypedTerm::Map(a), TypedTerm::Map(b)) => a.cmp(&b),
        (TypedTerm::Pid(a), TypedTerm::Pid(b)) => a.cmp(&b),
        (TypedTerm::Reference(a), TypedTerm::Reference(b)) => a.cmp(&b),
        _ => Ordering::Equal,
    };
    match ord {
        Ordering::Less => otter::atom![less],
        Ordering::Equal => otter::atom![equal],
        Ordering::Greater => otter::atom![greater],
    }
}

// --- test_debug/1 -------------------------------------------------------

#[otter::nif]
fn test_debug<'a>(env: CallEnv<'a>, val: TypedTerm<'a>) -> Binary<'a> {
    let s = match val {
        TypedTerm::Atom(v) => format!("{:?}", v),
        TypedTerm::Integer(v) => format!("{:?}", v),
        TypedTerm::Float(v) => format!("{:?}", v),
        TypedTerm::Bitstring(v) => format!("{:?}", v),
        TypedTerm::List(v) => format!("{:?}", v),
        TypedTerm::Tuple(v) => format!("{:?}", v),
        TypedTerm::Map(v) => format!("{:?}", v),
        TypedTerm::Pid(v) => format!("{:?}", v),
        TypedTerm::Port(v) => format!("{:?}", v),
        TypedTerm::Fun(v) => format!("{:?}", v),
        TypedTerm::Reference(v) => format!("{:?}", v),
    };
    Binary::from_bytes(env, s.as_bytes())
}

// --- test_try_from/1 ----------------------------------------------------
// Now `Integer::to_i64(env)` — extraction needs the env on the branded spine.

#[otter::nif]
fn test_try_from<'a>(env: CallEnv<'a>, val: Integer<'a>) -> TypedTerm<'a> {
    match val.to_i64(env) {
        Some(v) => TypedTerm::Integer(Integer::from_i64(env, v)),
        None => TypedTerm::Atom(otter::atom![overflow]),
    }
}

// --- test_binary_traits/0 -----------------------------------------------
// Binary's byte access takes an env now (no Deref/AsRef); BinaryBuf keeps them.

#[otter::nif]
fn test_binary_traits(env: CallEnv) -> Atom {
    let bin = Binary::from_bytes(env, b"hello world");
    assert!(bin.as_bytes(env).starts_with(b"hello"));
    assert_eq!(bin.len(env), 11);

    let sub = bin.sub(env, 6, 5);
    assert_eq!(sub.as_bytes(env), b"world");

    // BinaryBuf: Extend / Deref / DerefMut / io::Write all still hold (it owns
    // its allocation — no env needed).
    let mut builder = BinaryBuf::new();
    builder.extend(b"hello".iter().copied());
    assert_eq!(builder.len(), 5);
    assert_eq!(&*builder, b"hello");

    builder[0] = b'H';
    assert_eq!(&*builder, b"Hello");

    use std::io::Write;
    write!(builder, " world").unwrap();
    assert_eq!(&*builder, b"Hello world");

    let _ = builder.into_binary(env);

    otter::atom![ok]
}

// --- test_from_str/1 ----------------------------------------------------

#[otter::nif]
fn test_from_str<'a>(env: CallEnv<'a>, bin: Binary<'a>) -> Result<List<'a>, Raised<'a>> {
    // Non-UTF-8 argument bytes are badarg, not a panic.
    let Ok(s) = bin.try_str(env) else {
        return env.badarg();
    };
    Ok(List::from_str(env, s))
}

// --- reverse_list/1 -----------------------------------------------------

#[otter::nif]
fn reverse_list<'a>(env: CallEnv<'a>, list: List<'a>) -> TypedTerm<'a> {
    match list.reverse(env) {
        Some(rev) => TypedTerm::List(rev),
        None => TypedTerm::Atom(otter::atom![error]),
    }
}

// --- list_tail/1 --------------------------------------------------------

#[otter::nif]
fn list_tail<'a>(env: CallEnv<'a>, list: List<'a>) -> AnyTerm<'a> {
    let mut iter = list.iter(env);
    while iter.next().is_some() {}
    iter.tail().unwrap()
}

// --- atom_name/1 --------------------------------------------------------

#[otter::nif]
fn atom_name<'a>(env: CallEnv<'a>, a: Atom) -> Binary<'a> {
    let name = a.name(env);
    Binary::from_bytes(env, name.as_bytes())
}

// --- hm_new/0 -----------------------------------------------------------

#[otter::nif]
fn hm_new(env: CallEnv) -> ResourceArc<HashMapResource> {
    eprintln!("[otter_demo] HashMapResource constructed");
    otter::resource::make_resource(env, HashMapResource { map: Mutex::new(HashMap::new()) })
}

// --- hm_put/3 -----------------------------------------------------------

#[otter::nif]
fn hm_put<'a>(
    env: CallEnv<'a>,
    key: Binary<'a>,
    value: Binary<'a>,
    hm: ResourceArc<HashMapResource>,
) -> Atom {
    hm.map
        .lock()
        .unwrap()
        .insert(key.as_bytes(env).to_vec(), value.as_bytes(env).to_vec());
    otter::atom![ok]
}

// --- hm_get/2 -----------------------------------------------------------

#[otter::nif]
fn hm_get<'a>(env: CallEnv<'a>, key: Binary<'a>, hm: ResourceArc<HashMapResource>) -> TypedTerm<'a> {
    match hm.map.lock().unwrap().get(key.as_bytes(env)) {
        Some(val) => {
            let ok: TypedTerm = otter::atom![ok].into();
            let bin: TypedTerm = Binary::from_bytes(env, val).into();
            TypedTerm::Tuple(Tuple::from_terms(env, [ok, bin]))
        }
        None => TypedTerm::Atom(otter::atom![error]),
    }
}

// --- test_map/0 ---------------------------------------------------------

#[otter::nif]
fn test_map(env: CallEnv) -> Atom {
    let m = Map::new(env);
    assert_eq!(m.size(env), 0);

    let k1 = Atom::intern(env, "x").unwrap();
    let v1 = Integer::from_i64(env, 1);
    let m = m.put(env, k1, v1);
    assert_eq!(m.size(env), 1);

    match m.get(env, k1).unwrap().resolve(env) {
        Some(TypedTerm::Integer(i)) => assert_eq!(i.to_i64(env).unwrap(), 1),
        _ => panic!("expected integer"),
    }
    assert!(m.get(env, Atom::intern(env, "missing").unwrap()).is_none());

    let v2 = Integer::from_i64(env, 2);
    let m = m.update(env, k1, v2).unwrap();
    match m.get(env, k1).unwrap().resolve(env) {
        Some(TypedTerm::Integer(i)) => assert_eq!(i.to_i64(env).unwrap(), 2),
        _ => panic!("expected integer"),
    }

    assert!(m.update(env, Atom::intern(env, "missing").unwrap(), v1).is_none());

    let k2 = Atom::intern(env, "y").unwrap();
    let m = m.put(env, k2, Integer::from_i64(env, 3));
    assert_eq!(m.size(env), 2);
    assert_eq!(m.iter(env).count(), 2);

    let m = m.remove(env, k1);
    assert_eq!(m.size(env), 1);
    assert!(m.get(env, k1).is_none());

    otter::atom![ok]
}

// --- test_tuple/0 -------------------------------------------------------

#[otter::nif]
fn test_tuple(env: CallEnv) -> Atom {
    let a = TypedTerm::Atom(Atom::intern(env, "hello").unwrap());
    let b = TypedTerm::Integer(Integer::from_i64(env, 42));
    let t = Tuple::from_terms(env, [a, b]).with_elements(env);

    assert_eq!(t.len(), 2);
    assert!(!t.is_empty());
    assert!(t[0].resolve(env) == Some(a));
    assert!(t[1].resolve(env) == Some(b));
    // Iteration yields the elements as unresolved terms, in order.
    let collected: Vec<_> = t.into_iter().map(|e| e.resolve(env)).collect();
    assert!(collected == vec![Some(a), Some(b)]);

    let empty = Tuple::from_terms(env, std::iter::empty::<TypedTerm>()).with_elements(env);
    assert_eq!(empty.len(), 0);
    assert!(empty.is_empty());

    otter::atom![ok]
}

// --- double_float/1 -----------------------------------------------------

#[otter::nif]
fn double_float<'a>(env: CallEnv<'a>, val: Float<'a>) -> Result<Float<'a>, Raised<'a>> {
    match Float::from_f64(env, val.to_f64(env) * 2.0) {
        Some(f) => Ok(f),
        None => env.badarg(),
    }
}

// --- nan_float/0 --------------------------------------------------------
// from_f64 rejects NaN in Rust (returns None); raise badarg ourselves so the
// BEAM raises it on return.

#[otter::nif]
fn nan_float<'a>(env: CallEnv<'a>) -> Result<Float<'a>, Raised<'a>> {
    match Float::from_f64(env, f64::NAN) {
        Some(f) => Ok(f),
        None => env.badarg(),
    }
}

// --- test_pid/0 ---------------------------------------------------------

#[otter::nif]
fn test_pid(env: CallEnv) -> LocalPid {
    let pid = LocalPid::self_(env);
    assert!(pid.is_alive(env));

    let init = LocalPid::whereis(env, Atom::intern(env, "init").unwrap());
    assert!(init.is_some());

    pid
}

// --- new_ref/0 ----------------------------------------------------------

#[otter::nif]
fn new_ref<'a>(env: CallEnv<'a>) -> Reference<'a> {
    Reference::new(env)
}

// --- divide/2 -----------------------------------------------------------

#[otter::nif]
fn divide<'a>(env: CallEnv<'a>, a: Integer<'a>, b: Integer<'a>) -> Result<Integer<'a>, Raised<'a>> {
    let (Some(a), Some(b)) = (a.to_i64(env), b.to_i64(env)) else {
        return env.badarg();
    };
    if b == 0 {
        return env.raise(otter::atom![division_by_zero]);
    }
    Ok(Integer::from_i64(env, a / b))
}

// --- dirty_cpu_thread_type/0 --------------------------------------------

#[otter::nif(schedule = "DirtyCpu")]
fn dirty_cpu_thread_type(_env: CallEnv) -> Atom {
    match otter::system::thread_type() {
        otter::system::ThreadType::DirtyCpu => otter::atom![dirty_cpu],
        _ => otter::atom![error],
    }
}

// --- send_from_thread/0 -------------------------------------------------
// Build a term in an owned arena on a spawned thread, then steal-send it.

#[otter::nif]
fn send_from_thread(env: CallEnv) -> Atom {
    let pid = LocalPid::self_(env);
    std::thread::spawn(move || {
        let mut arena = OwnedEnvArena::new();
        let msg = arena.run(|oenv| oenv.export(otter::atom![from_thread]));
        otter::types::send_move(&pid, &mut arena, msg);
    });
    otter::atom![ok]
}

// --- send_to/2 ----------------------------------------------------------

#[otter::nif]
fn send_to<'a>(env: CallEnv<'a>, to: LocalPid, msg: AnyTerm<'a>) -> Atom {
    otter::types::send_copy_from(env, &to, msg);
    otter::atom![ok]
}

// --- send_move_to/2 -----------------------------------------------------
// In-NIF attributed steal-send: copy the term into an owned arena on the
// scheduler thread, then move its heap into the recipient's mailbox
// attributed to the calling process — `enif_send` with a non-NULL caller
// AND a non-NULL msg_env, the quadrant rustler's API cannot express.

#[otter::nif]
fn send_move_to<'a>(env: CallEnv<'a>, to: LocalPid, msg: AnyTerm<'a>) -> Atom {
    let mut arena = OwnedEnvArena::new();
    let oterm = arena.copy_in(msg);
    otter::types::send_move_from(env, &to, &mut arena, oterm);
    otter::atom![ok]
}

// --- cpu_time/0 ---------------------------------------------------------

#[otter::nif]
fn cpu_time<'a>(env: CallEnv<'a>) -> Result<Tuple<'a>, Raised<'a>> {
    env.cpu_time()
}

// --- HashMap resource ---------------------------------------------------

struct HashMapResource {
    map: Mutex<HashMap<Vec<u8>, Vec<u8>>>,
}

impl Resource for HashMapResource {
    fn destructor(self, _env: CallbackEnv<'_>) {
        eprintln!(
            "[otter_demo] HashMapResource destructed ({} entries)",
            self.map.lock().unwrap().len()
        );
    }
}

// Exercises the catch_unwind wrapper in otter's resource destructor callback:
// Drop panics; the wrapper must absorb it and let the BEAM continue.
struct PanickingResource;

impl Resource for PanickingResource {}

impl Drop for PanickingResource {
    fn drop(&mut self) {
        panic!("intentional panic from PanickingResource::drop");
    }
}

#[otter::nif]
fn panicking_resource_new(env: CallEnv) -> ResourceArc<PanickingResource> {
    otter::resource::make_resource(env, PanickingResource)
}

// Exercises the catch_unwind wrapper's RETURN-ENCODING stage (audit-16 / H1):
// the return type's `Encoder::encode` panics. The generated wrapper must catch
// it and surface `nif_panicked`, not let the panic unwind across the `extern
// "C"` boundary (UB). Before the fix, encoding ran outside the catch.
struct PanickingEncode;

impl<'id> otter::codec::Encoder<'id> for PanickingEncode {
    fn encode(&self, _env: impl Env<'id>) -> Result<AnyTerm<'id>, otter::codec::CodecError> {
        panic!("intentional panic from PanickingEncode::encode");
    }
}

#[otter::nif]
fn panic_in_encoder(_env: CallEnv) -> PanickingEncode {
    PanickingEncode
}

// --- select / stop callback (audit-01 regression) -----------------------

struct FdResource {
    a: UnixStream,
    b: UnixStream,
    stop_count: AtomicUsize,
}

impl Resource for FdResource {
    fn stop(&self, _env: CallbackEnv<'_>, _event: otter::select::Event, _is_direct_call: bool) {
        self.stop_count.fetch_add(1, Ordering::Relaxed);
    }
}

#[otter::nif]
fn select_resource_new(env: CallEnv) -> ResourceArc<FdResource> {
    let (a, b) = UnixStream::pair().expect("socketpair");
    otter::resource::make_resource(env, FdResource { a, b, stop_count: AtomicUsize::new(0) })
}

#[otter::nif]
fn select_register<'a>(env: CallEnv<'a>, arc: ResourceArc<FdResource>) -> Integer<'a> {
    let pid = LocalPid::self_(env);
    let flags = otter::select::select(
        env,
        arc.a.as_raw_fd(),
        SelectFlags::READ,
        &arc,
        &pid,
        Reference::new(env),
    );
    Integer::from_i64(env, flags as i64)
}

#[otter::nif]
fn select_stop<'a>(env: CallEnv<'a>, arc: ResourceArc<FdResource>) -> Integer<'a> {
    let pid = LocalPid::self_(env);
    let flags = otter::select::select(
        env,
        arc.a.as_raw_fd(),
        SelectFlags::STOP,
        &arc,
        &pid,
        Reference::new(env),
    );
    Integer::from_i64(env, flags as i64)
}

#[otter::nif]
fn select_stop_count<'a>(env: CallEnv<'a>, arc: ResourceArc<FdResource>) -> Integer<'a> {
    Integer::from_i64(env, arc.stop_count.load(Ordering::Relaxed) as i64)
}

#[otter::nif]
fn select_x_register<'a>(
    env: CallEnv<'a>,
    arc: ResourceArc<FdResource>,
    msg: AnyTerm<'a>,
) -> Integer<'a> {
    use std::io::Write;
    let pid = LocalPid::self_(env);
    let flags = otter::select::select_x(
        env,
        arc.a.as_raw_fd(),
        SelectFlags::READ | SelectFlags::CUSTOM_MSG,
        &arc,
        &pid,
        msg,
        None::<CallEnv<'a>>,
    );
    let mut peer = &arc.b;
    let _ = peer.write_all(b"x");
    Integer::from_i64(env, flags as i64)
}

// --- port_send/2 --------------------------------------------------------

#[otter::nif]
fn port_send<'a>(env: CallEnv<'a>, port: LocalPort, data: Binary<'a>) -> Atom {
    if otter::types::port_command(env, &port, data) {
        otter::atom![ok]
    } else {
        otter::atom![error]
    }
}

// --- test_time/0 --------------------------------------------------------

#[otter::nif]
fn test_time(_env: CallEnv) -> Atom {
    use otter::time::{convert_time_unit, monotonic_time, time_offset, TimeUnit};

    let t1 = monotonic_time(TimeUnit::Nanosecond);
    let t2 = monotonic_time(TimeUnit::Nanosecond);
    assert!(t2 >= t1);

    let _ = time_offset(TimeUnit::Millisecond);

    assert_eq!(convert_time_unit(1, TimeUnit::Second, TimeUnit::Nanosecond), 1_000_000_000);
    assert_eq!(convert_time_unit(1000, TimeUnit::Millisecond, TimeUnit::Second), 1);

    otter::atom![ok]
}

// --- test_consume_timeslice/0 -------------------------------------------

#[otter::nif]
fn test_consume_timeslice(env: CallEnv) -> Atom {
    for _ in 0..100 {
        if env.consume_timeslice(100) {
            return otter::atom![ok];
        }
    }
    otter::atom![error]
}

// --- monitor / down callback --------------------------------------------

struct MonitorResource {
    down_count: AtomicUsize,
}

impl Resource for MonitorResource {
    fn down<'a>(&'a self, _env: CallbackEnv<'a>, _pid: LocalPid, _monitor: otter::resource::Monitor) {
        self.down_count.fetch_add(1, Ordering::Relaxed);
    }
}

#[otter::nif]
fn monitor_resource_new(env: CallEnv) -> ResourceArc<MonitorResource> {
    otter::resource::make_resource(env, MonitorResource { down_count: AtomicUsize::new(0) })
}

#[otter::nif]
fn monitor_pid<'a>(env: CallEnv<'a>, arc: ResourceArc<MonitorResource>, pid: LocalPid) -> Atom {
    match arc.monitor(Some(env), &pid) {
        Some(_) => otter::atom![ok],
        None => otter::atom![error],
    }
}

#[otter::nif]
fn monitor_down_count<'a>(env: CallEnv<'a>, arc: ResourceArc<MonitorResource>) -> Integer<'a> {
    Integer::from_i64(env, arc.down_count.load(Ordering::Relaxed) as i64)
}

// --- native codec round-trips: integers ---------------------------------
// Each takes the native Rust integer as an argument (decode) and returns it
// (encode). Out-of-range arguments fail to decode and surface as badarg.

#[otter::nif]
fn codec_i8(_env: CallEnv, x: i8) -> i8 {
    x
}

#[otter::nif]
fn codec_u8(_env: CallEnv, x: u8) -> u8 {
    x
}

#[otter::nif]
fn codec_i64(_env: CallEnv, x: i64) -> i64 {
    x
}

#[otter::nif]
fn codec_u64(_env: CallEnv, x: u64) -> u64 {
    x
}

#[otter::nif]
fn codec_usize(_env: CallEnv, x: usize) -> usize {
    x
}

// --- native codec round-trips: floats -----------------------------------

#[otter::nif]
fn codec_f64(_env: CallEnv, x: f64) -> f64 {
    x
}

#[otter::nif]
fn codec_f32(_env: CallEnv, x: f32) -> f32 {
    x
}

// Returns a non-finite f64: encoding it fails (NotFinite), which the nif
// macro turns into a `badret` exception — the encode-side mirror of badarg.
#[otter::nif]
fn encode_inf(_env: CallEnv) -> f64 {
    f64::INFINITY
}

#[otter::nif]
fn encode_nan(_env: CallEnv) -> f64 {
    f64::NAN
}

// --- native codec round-trips: bool -------------------------------------

#[otter::nif]
fn codec_bool(_env: CallEnv, x: bool) -> bool {
    x
}

#[otter::nif]
fn negate(_env: CallEnv, x: bool) -> bool {
    !x
}

// --- native codec round-trips: String -----------------------------------
// Decodes from a binary or a charlist; always encodes to a binary.

#[otter::nif]
fn codec_string(_env: CallEnv, s: String) -> String {
    s
}

#[otter::nif]
fn shout(_env: CallEnv, s: String) -> String {
    s.to_uppercase()
}

// --- native codec round-trips: tuples -----------------------------------

#[otter::nif]
fn codec_pair(_env: CallEnv, t: (i64, bool)) -> (i64, bool) {
    t
}

#[otter::nif]
fn codec_triple(_env: CallEnv, t: (u8, String, f64)) -> (u8, String, f64) {
    t
}

#[otter::nif]
fn swap(_env: CallEnv, t: (i64, i64)) -> (i64, i64) {
    (t.1, t.0)
}

// --- native codec round-trips: Vec/lists --------------------------------
// Decodes a proper list element-wise into a Vec and re-encodes it.

#[otter::nif]
fn codec_int_list(_env: CallEnv, v: Vec<i64>) -> Vec<i64> {
    v
}

#[otter::nif]
fn sum_i64(_env: CallEnv, v: Vec<i64>) -> i64 {
    v.iter().sum()
}

#[otter::nif]
fn codec_str_list(_env: CallEnv, v: Vec<String>) -> Vec<String> {
    v
}

// --- native codec round-trips: HashMap ----------------------------------
// Decodes an Erlang map into a HashMap<String, i64> and re-encodes it.

#[otter::nif]
fn codec_map(_env: CallEnv, m: HashMap<String, i64>) -> HashMap<String, i64> {
    m
}

#[otter::nif]
fn map_sum_values(_env: CallEnv, m: HashMap<String, i64>) -> i64 {
    m.values().sum()
}

// --- native codec round-trips: bignums (num-bigint) ---------------------
// Decodes an arbitrary-precision integer (including bignums beyond i64/u64,
// which the native i64/u64 codecs reject as badarg) into a BigInt and
// re-encodes it. Exercises the ETF read/write path in Integer::to/from_bigint.

#[otter::nif]
fn codec_bigint(_env: CallEnv, x: BigInt) -> BigInt {
    x
}

#[otter::nif]
fn bigint_add(_env: CallEnv, a: BigInt, b: BigInt) -> BigInt {
    a + b
}

// Produces 2^n as a BigInt — a clean bignum for n >= 64, proving the >64-bit
// write path.
#[otter::nif]
fn bigint_pow2(_env: CallEnv, n: u32) -> BigInt {
    BigInt::from(1u8) << (n as usize)
}

// --- Atom::intern — named recoverable error -----------------------------
// Interns `name` and returns {ok, Atom}, or {error, name_too_long} when the
// name exceeds 255 characters (AtomError::NameTooLong). The error is a plain
// Rust value, not a raised exception — the NIF maps it to an error atom itself.

#[otter::nif]
fn intern_atom(env: CallEnv, name: String) -> (Atom, Atom) {
    match Atom::intern(env, &name) {
        Ok(a) => (otter::atom![ok], a),
        Err(AtomError::NameTooLong) => (otter::atom![error], otter::atom![name_too_long]),
    }
}

fn on_load(_env: InitEnv, _load_info: AnyTerm) -> bool {
    // Atoms and resources are interned/registered by the `init!` scaffolding
    // before this runs; nothing to do here.
    true
}

// --- init ---------------------------------------------------------------

otter::init!("otter_demo__nif", [
    hello,
    add,
    echo,
    type_of,
    reverse_binary,
    etf_encode,
    etf_roundtrip,
    sum_list,
    test_eq,
    test_ord,
    test_debug,
    test_try_from,
    test_binary_traits,
    test_from_str,
    reverse_list,
    list_tail,
    atom_name,
    hm_new,
    hm_put,
    hm_get,
    test_map,
    test_tuple,
    double_float,
    nan_float,
    test_pid,
    new_ref,
    divide,
    dirty_cpu_thread_type,
    send_from_thread,
    send_to,
    send_move_to,
    cpu_time,
    codec_i8,
    codec_u8,
    codec_i64,
    codec_u64,
    codec_usize,
    codec_f64,
    codec_f32,
    encode_inf,
    encode_nan,
    codec_bool,
    negate,
    codec_string,
    shout,
    codec_pair,
    codec_triple,
    swap,
    codec_int_list,
    sum_i64,
    codec_str_list,
    codec_map,
    map_sum_values,
    codec_bigint,
    bigint_add,
    bigint_pow2,
    intern_atom,
    panicking_resource_new,
    panic_in_encoder,
    select_resource_new,
    select_register,
    select_stop,
    select_stop_count,
    select_x_register,
    monitor_resource_new,
    monitor_pid,
    monitor_down_count,
    test_time,
    test_consume_timeslice,
    port_send,
],
atoms = [
    ok, error,
    true_ = "true", false_ = "false",
    world, overflow,
    less, equal, greater,
    atom, integer, float, binary, bitstring, list,
    tuple, map, pid, port, fun, reference,
    division_by_zero, dirty_cpu, from_thread,
    name_too_long,
],
resources = [HashMapResource: "v1", PanickingResource, FdResource, MonitorResource],
load = on_load);
