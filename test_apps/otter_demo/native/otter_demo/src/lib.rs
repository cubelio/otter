use std::collections::HashMap;
use std::os::unix::io::AsRawFd;
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use otter::enif_ffi::SelectFlags;
use otter::resource::{Resource, ResourceArc};
use otter::types::{
    AnyTerm, Atom, Binary, BinaryBuf, CallEnv, CallbackEnv, Env, Float, InitEnv, Integer, List,
    LocalPid, LocalPort, Map, OwnedEnvArena, Raised, Reference, Tuple, TypedTerm,
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
fn add<'a>(env: CallEnv<'a>, a: Integer<'a>, b: Integer<'a>) -> Integer<'a> {
    let sum = a.to_i64(env).unwrap() + b.to_i64(env).unwrap();
    Integer::from_i64(env, sum)
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
fn test_from_str<'a>(env: CallEnv<'a>, bin: Binary<'a>) -> List<'a> {
    let s = bin.try_str(env).unwrap();
    List::from_str(env, s)
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

    let m = m.remove(env, k1).unwrap();
    assert_eq!(m.size(env), 1);
    assert!(m.get(env, k1).is_none());

    otter::atom![ok]
}

// --- test_tuple/0 -------------------------------------------------------

#[otter::nif]
fn test_tuple(env: CallEnv) -> Atom {
    let a = TypedTerm::Atom(Atom::intern(env, "hello").unwrap());
    let b = TypedTerm::Integer(Integer::from_i64(env, 42));
    let t = Tuple::from_terms(env, [a, b]);

    assert_eq!(t.len(env), 2);
    assert!(!t.is_empty(env));
    assert!(t.element(env, 0).resolve(env) == Some(a));
    assert!(t.element(env, 1).resolve(env) == Some(b));

    let empty = Tuple::from_terms(env, std::iter::empty::<TypedTerm>());
    assert_eq!(empty.len(env), 0);
    assert!(empty.is_empty(env));

    otter::atom![ok]
}

// --- double_float/1 -----------------------------------------------------

#[otter::nif]
fn double_float<'a>(env: CallEnv<'a>, val: Float<'a>) -> Result<Float<'a>, Raised<'a>> {
    match Float::from_f64(env, val.to_f64(env).unwrap() * 2.0) {
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
    let b_val = b.to_i64(env).unwrap();
    if b_val == 0 {
        return env.raise(otter::atom![division_by_zero]);
    }
    Ok(Integer::from_i64(env, a.to_i64(env).unwrap() / b_val))
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
        otter::types::send(&pid, &mut arena, msg);
    });
    otter::atom![ok]
}

// --- send_to/2 ----------------------------------------------------------

#[otter::nif]
fn send_to<'a>(env: CallEnv<'a>, to: LocalPid, msg: AnyTerm<'a>) -> Atom {
    otter::types::send_from(env, &to, msg);
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

// --- select / stop callback (audit-01 regression) -----------------------

struct FdResource {
    a: UnixStream,
    b: UnixStream,
    stop_count: AtomicUsize,
}

impl Resource for FdResource {
    fn stop(&self, _env: CallbackEnv<'_>, _event: otter::enif_ffi::Event, _is_direct_call: bool) {
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
    cpu_time,
    panicking_resource_new,
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
],
resources = [HashMapResource: "v1", PanickingResource, FdResource, MonitorResource],
load = on_load);
