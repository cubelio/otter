use std::collections::HashMap;
use std::os::unix::io::AsRawFd;
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

use otter::env::{Env, OwnedEnv};
use otter::resource::{Resource, ResourceArc, ResourceTypeHandle};
use otter::sys::NifSelectFlags;
use otter::term::{Term, TypedTerm, Raised};
use otter::types::{Atom, Binary, BinaryBuilder, Float, Integer, List, Map, Pid, Port, Reference, Tuple};

otter::declare_atoms![
    ok, error,
    true_ = "true", false_ = "false",
    world, overflow,
    less, equal, greater,
    atom, integer, float, binary, bitstring, list,
    tuple, map, pid, port, fun, reference,
    division_by_zero, dirty_cpu, from_thread,
];


fn atomize_bool(value: bool) -> Atom {
    if value { otter::atom![true_] } else { otter::atom![false_] }
}

// --- hello/0 -----------------------------------------------------------
// Simplest possible NIF: no arguments, returns an atom.

#[otter::nif]
fn hello(_env: Env) -> Atom {
    otter::atom![world]
}

// --- add/2 --------------------------------------------------------------
// Typed arguments via Decoder, typed return via Encoder.

#[otter::nif]
fn add<'a>(env: Env<'a>, a: Integer<'a>, b: Integer<'a>) -> Integer<'a> {
    let sum = i64::try_from(a).unwrap() + i64::try_from(b).unwrap();
    Integer::from_i64(env, sum)
}

// --- echo/1 -------------------------------------------------------------
// TypedTerm in, TypedTerm out — zero-cost passthrough.

#[otter::nif]
fn echo<'a>(_env: Env<'a>, val: TypedTerm<'a>) -> TypedTerm<'a> {
    val
}

// --- type_of/1 ----------------------------------------------------------
// Pattern match on TypedTerm to inspect the Erlang type.

#[otter::nif]
fn type_of(_env: Env, val: TypedTerm) -> Atom {
    match val {
        TypedTerm::Atom(_)      => otter::atom![atom],
        TypedTerm::Integer(_)   => otter::atom![integer],
        TypedTerm::Float(_)     => otter::atom![float],
        TypedTerm::Bitstring(bs) => if bs.is_binary() {
            otter::atom![binary]
        } else {
            otter::atom![bitstring]
        },
        TypedTerm::List(_)      => otter::atom![list],
        TypedTerm::Tuple(_)     => otter::atom![tuple],
        TypedTerm::Map(_)       => otter::atom![map],
        TypedTerm::Pid(_)       => otter::atom![pid],
        TypedTerm::Port(_)      => otter::atom![port],
        TypedTerm::Fun(_)       => otter::atom![fun],
        TypedTerm::Reference(_) => otter::atom![reference],
    }
}

// --- reverse_binary/1 ---------------------------------------------------
// Decode a Binary, build a new one with reversed bytes.

#[otter::nif]
fn reverse_binary<'a>(env: Env<'a>, bin: Binary<'a>) -> Binary<'a> {
    let bytes = bin.as_bytes();
    let mut builder = BinaryBuilder::with_capacity(bytes.len());
    for &b in bytes.iter().rev() {
        builder.push(b);
    }
    builder.finish(env)
}

// --- sum_list/1 ---------------------------------------------------------
// Walk a proper list of integers and return the sum, using the iterator.

#[otter::nif]
fn sum_list<'a>(env: Env<'a>, list: List<'a>) -> Integer<'a> {
    let sum: i64 = list.iter()
        .filter_map(|raw| match raw.resolve() {
            Some(TypedTerm::Integer(i)) => Some(i64::try_from(i).unwrap()),
            _ => None,
        })
        .sum();
    Integer::from_i64(env, sum)
}

// --- test_eq/2 ----------------------------------------------------------
// Test PartialEq between two terms of the same type.

#[otter::nif]
fn test_eq<'a>(_env: Env<'a>, a: TypedTerm<'a>, b: TypedTerm<'a>) -> Atom {
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
    // true/false are always pre-existing in the atom table
    atomize_bool(result)
}

// --- test_ord/2 ---------------------------------------------------------
// Test Ord between two terms of the same type.
// Returns less, equal, or greater.

#[otter::nif]
fn test_ord<'a>(_env: Env<'a>, a: TypedTerm<'a>, b: TypedTerm<'a>) -> Atom {
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
// Test Debug formatting — returns the Debug string as a binary.

#[otter::nif]
fn test_debug<'a>(env: Env<'a>, val: TypedTerm<'a>) -> Binary<'a> {
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
// Test TryFrom<Integer> for i64. Returns the value or the atom 'overflow'.

#[otter::nif]
fn test_try_from<'a>(env: Env<'a>, val: Integer<'a>) -> TypedTerm<'a> {
    match i64::try_from(val) {
        Ok(v) => TypedTerm::Integer(Integer::from_i64(env, v)),
        Err(_) => TypedTerm::Atom(otter::atom![overflow]),
    }
}

// --- test_binary_traits/0 -----------------------------------------------
// Exercise Binary Deref, AsRef, sub, and BinaryBuilder Extend/Deref.

#[otter::nif]
fn test_binary_traits(env: Env) -> Atom {
    // Binary: Deref gives us slice methods
    let bin = Binary::from_bytes(env, b"hello world");
    assert!(bin.starts_with(b"hello"));  // via Deref<Target=[u8]>
    assert_eq!(bin.len(), 11);

    // Binary: sub with bounds check
    let sub = bin.sub(6, 5);
    assert_eq!(sub.as_bytes(), b"world");

    // Binary: AsRef
    fn takes_asref(b: &impl AsRef<[u8]>) -> usize { b.as_ref().len() }
    assert_eq!(takes_asref(&bin), 11);

    // BinaryBuilder: Extend
    let mut builder = BinaryBuilder::new();
    builder.extend(b"hello".iter().copied());
    assert_eq!(builder.len(), 5);
    assert_eq!(&*builder, b"hello");  // via Deref

    // BinaryBuilder: DerefMut
    builder[0] = b'H';
    assert_eq!(&*builder, b"Hello");

    // BinaryBuilder: io::Write
    use std::io::Write;
    write!(builder, " world").unwrap();
    assert_eq!(&*builder, b"Hello world");

    let _ = builder.finish(env);

    otter::atom![ok]
}

// --- test_from_str/1 ----------------------------------------------------
// Test List::from_str — returns the Erlang string (list of codepoints).

#[otter::nif]
fn test_from_str<'a>(env: Env<'a>, bin: Binary<'a>) -> List<'a> {
    let s = bin.try_str().unwrap();
    List::from_str(env, s)
}

// --- reverse_list/1 -----------------------------------------------------
// Test List::reverse.

#[otter::nif]
fn reverse_list<'a>(_env: Env<'a>, list: List<'a>) -> TypedTerm<'a> {
    match list.reverse() {
        Some(rev) => TypedTerm::List(rev),
        None => TypedTerm::Atom(otter::atom![error]),
    }
}

// --- list_tail/1 --------------------------------------------------------
// Return the tail of an iterated list (tests ListIterator::tail).

#[otter::nif]
fn list_tail<'a>(_env: Env<'a>, list: List<'a>) -> Term<'a> {
    let mut iter = list.iter();
    while iter.next().is_some() {}
    iter.tail().unwrap()
}

// --- atom_name/1 --------------------------------------------------------
// Return an atom's name as a binary, exposing the raw bytes from Atom::name().

#[otter::nif]
fn atom_name<'a>(env: Env<'a>, a: Atom) -> Binary<'a> {
    let name = a.name(env);
    Binary::from_bytes(env, name.as_bytes())
}

// --- hm_new/0 -----------------------------------------------------------

#[otter::nif]
fn hm_new(_env: Env) -> ResourceArc<HashMapResource> {
    eprintln!("[otter_demo] HashMapResource constructed");
    ResourceArc::from(HashMapResource {
        map: Mutex::new(HashMap::new()),
    })
}

// --- hm_put/3 -----------------------------------------------------------

#[otter::nif]
fn hm_put<'a>(_env: Env<'a>, key: Binary<'a>, value: Binary<'a>, hm: ResourceArc<HashMapResource>) -> Atom {
    hm.map.lock().unwrap().insert(key.as_bytes().to_vec(), value.as_bytes().to_vec());
    otter::atom![ok]
}

// --- hm_get/2 -----------------------------------------------------------

#[otter::nif]
fn hm_get<'a>(env: Env<'a>, key: Binary<'a>, hm: ResourceArc<HashMapResource>) -> TypedTerm<'a> {
    match hm.map.lock().unwrap().get(key.as_bytes()) {
        Some(val) => {
            let ok: TypedTerm = otter::atom![ok].into();
            let bin: TypedTerm = Binary::from_bytes(env, val).into();
            TypedTerm::Tuple(Tuple::from_terms(env, [ok, bin]))
        }
        None => TypedTerm::Atom(otter::atom![error]),
    }
}

// --- test_map/0 ---------------------------------------------------------
// Exercise Map::new, put, get, update, remove, size, iter.

#[otter::nif]
fn test_map(env: Env) -> Atom {
    let m = Map::new(env);
    assert_eq!(m.size(), 0);

    let k1 = Atom::intern(env, "x").unwrap();
    let v1 = Integer::from_i64(env, 1);
    let m = m.put(k1, v1);
    assert_eq!(m.size(), 1);

    // get
    match m.get(k1).unwrap().resolve() {
        Some(TypedTerm::Integer(i)) => assert_eq!(i64::try_from(i).unwrap(), 1),
        _ => panic!("expected integer"),
    }
    assert!(m.get(Atom::intern(env, "missing").unwrap()).is_none());

    // update existing key
    let v2 = Integer::from_i64(env, 2);
    let m = m.update(k1, v2).unwrap();
    match m.get(k1).unwrap().resolve() {
        Some(TypedTerm::Integer(i)) => assert_eq!(i64::try_from(i).unwrap(), 2),
        _ => panic!("expected integer"),
    }

    // update missing key returns None
    assert!(m.update(Atom::intern(env, "missing").unwrap(), v1).is_none());

    // put second key, iterate
    let k2 = Atom::intern(env, "y").unwrap();
    let m = m.put(k2, Integer::from_i64(env, 3));
    assert_eq!(m.size(), 2);
    assert_eq!(m.iter().count(), 2);

    // remove
    let m = m.remove(k1).unwrap();
    assert_eq!(m.size(), 1);
    assert!(m.get(k1).is_none());

    otter::atom![ok]
}

// --- test_tuple/0 -------------------------------------------------------
// Exercise Tuple::from_terms, element, len, is_empty.

#[otter::nif]
fn test_tuple(env: Env) -> Atom {
    let a = TypedTerm::Atom(Atom::intern(env, "hello").unwrap());
    let b = TypedTerm::Integer(Integer::from_i64(env, 42));
    let t = Tuple::from_terms(env, [a, b]);

    assert_eq!(t.len(), 2);
    assert!(!t.is_empty());
    assert!(t.element(0).resolve() == Some(a));
    assert!(t.element(1).resolve() == Some(b));

    let empty = Tuple::from_terms(env, std::iter::empty::<TypedTerm>());
    assert_eq!(empty.len(), 0);
    assert!(empty.is_empty());

    otter::atom![ok]
}

// --- double_float/1 -----------------------------------------------------
// Float decode → f64 → Float encode roundtrip.

#[otter::nif]
fn double_float<'a>(env: Env<'a>, val: Float<'a>) -> Result<Float<'a>, Raised<'a>> {
    Float::from_f64(env, f64::from(val) * 2.0)
}

// --- nan_float/0 --------------------------------------------------------
// make_double on a non-finite value raises badarg on the env; the Raised
// witness propagates out and the BEAM raises it on return.

#[otter::nif]
fn nan_float<'a>(env: Env<'a>) -> Result<Float<'a>, Raised<'a>> {
    env.make_double(f64::NAN)
}

// --- test_pid/0 ---------------------------------------------------------
// Exercise Pid::self_, is_alive, as_nif_pid, whereis.

#[otter::nif]
fn test_pid(env: Env) -> Pid {
    let pid = Pid::self_(env);
    assert!(pid.is_alive(env));
    assert!(pid.as_nif_pid(env).is_some());

    // whereis — 'init' is always registered
    let init = Pid::whereis(env, Atom::intern(env, "init").unwrap());
    assert!(init.is_some());

    pid
}

// --- new_ref/0 ----------------------------------------------------------
// Exercise Reference::new and Reference encode.

#[otter::nif]
fn new_ref<'a>(env: Env<'a>) -> Reference<'a> {
    Reference::new(env)
}

// --- divide/2 -----------------------------------------------------------
// Result<T, Raised> return type — raising goes through env.raise_exception,
// which yields the Raised that `?` propagates out as the exception.

#[otter::nif]
fn divide<'a>(env: Env<'a>, a: Integer<'a>, b: Integer<'a>) -> Result<Integer<'a>, Raised<'a>> {
    let b_val = i64::try_from(b).unwrap();
    if b_val == 0 {
        return env.raise_exception(otter::atom![division_by_zero]);
    }
    Ok(Integer::from_i64(env, i64::try_from(a).unwrap() / b_val))
}

// --- dirty_cpu_thread_type/0 --------------------------------------------
// Dirty CPU scheduler — verifies scheduling via thread_type().

#[otter::nif(schedule = "DirtyCpu")]
fn dirty_cpu_thread_type(_env: Env) -> Atom {
    match otter::system::thread_type() {
        otter::system::ThreadType::DirtyCpu => otter::atom![dirty_cpu],
        _ => otter::atom![error],
    }
}

// --- send_from_thread/0 -------------------------------------------------
// OwnedEnv: spawn a thread, build a term, send to calling process.

#[otter::nif]
fn send_from_thread(env: Env) -> Atom {
    let pid = Pid::self_(env);
    std::thread::spawn(move || {
        let mut owned = OwnedEnv::new();
        owned.send(&pid, |_env| {
            TypedTerm::Atom(otter::atom![from_thread])
        });
    });
    otter::atom![ok]
}

// --- send_to/2 ----------------------------------------------------------
// In-NIF send: copy a term from the caller env into a pid's mailbox.

#[otter::nif]
fn send_to<'a>(env: Env<'a>, to: Pid, msg: TypedTerm<'a>) -> Atom {
    env.send(&to, msg);
    otter::atom![ok]
}

// --- cpu_time/0 ---------------------------------------------------------
// enif_cpu_time returns an erlang:timestamp()-format tuple, or raises badarg
// if the OS cannot provide it.

#[otter::nif]
fn cpu_time<'a>(env: Env<'a>) -> Result<Term<'a>, Raised<'a>> {
    env.cpu_time()
}

// --- HashMap resource ---------------------------------------------------

struct HashMapResource {
    map: Mutex<HashMap<Vec<u8>, Vec<u8>>>,
}

static HASH_MAP_RESOURCE_TYPE: OnceLock<ResourceTypeHandle> = OnceLock::new();

impl Resource for HashMapResource {
    fn resource_type_handle() -> &'static OnceLock<ResourceTypeHandle> {
        &HASH_MAP_RESOURCE_TYPE
    }

    fn destructor(self, _env: Env<'_>) {
        eprintln!("[otter_demo] HashMapResource destructed ({} entries)", self.map.lock().unwrap().len());
    }
}

// Exists solely to exercise the S1 catch_unwind wrapper in otter's resource
// destructor callback. Drop panics; the wrapper must absorb it and let the
// BEAM continue. See the panicking_destructor test in otter_demo__nif_test.
struct PanickingResource;

static PANICKING_RESOURCE_TYPE: OnceLock<ResourceTypeHandle> = OnceLock::new();

impl Resource for PanickingResource {
    fn resource_type_handle() -> &'static OnceLock<ResourceTypeHandle> {
        &PANICKING_RESOURCE_TYPE
    }
}

impl Drop for PanickingResource {
    fn drop(&mut self) {
        panic!("intentional panic from PanickingResource::drop");
    }
}

#[otter::nif]
fn panicking_resource_new(_env: Env) -> ResourceArc<PanickingResource> {
    ResourceArc::from(PanickingResource)
}

// --- select / stop callback (audit-01 regression) -----------------------
// A resource owning a connected socket pair. select() registers READ
// interest on one end; select() with STOP drives the select-stop path,
// which the BEAM dispatches to Resource::stop. Before audit-01 the stop
// slot was NULL and this call segfaulted the VM. `stop` bumps a counter the
// Erlang side polls, proving the (non-NULL) callback ran and the VM lived.
//
// Both ends are held alive so neither becomes readable: the only event is
// the explicit STOP. The streams close on Drop, after STOP has already
// deregistered the fd from the pollset.
struct FdResource {
    a: UnixStream,
    b: UnixStream,
    stop_count: AtomicUsize,
}

static FD_RESOURCE_TYPE: OnceLock<ResourceTypeHandle> = OnceLock::new();

impl Resource for FdResource {
    fn resource_type_handle() -> &'static OnceLock<ResourceTypeHandle> {
        &FD_RESOURCE_TYPE
    }

    fn stop(&self, _env: Env<'_>, _event: otter::sys::NifEvent, _is_direct_call: bool) {
        self.stop_count.fetch_add(1, Ordering::Relaxed);
    }
}

#[otter::nif]
fn select_resource_new(_env: Env) -> ResourceArc<FdResource> {
    let (a, b) = UnixStream::pair().expect("socketpair");
    ResourceArc::from(FdResource { a, b, stop_count: AtomicUsize::new(0) })
}

#[otter::nif]
fn select_register<'a>(env: Env<'a>, arc: ResourceArc<FdResource>) -> Integer<'a> {
    let pid = Pid::self_(env);
    let ref_term = TypedTerm::Reference(Reference::new(env));
    let flags = otter::select::select(
        env, arc.a.as_raw_fd(), NifSelectFlags::READ, &arc, &pid, ref_term,
    );
    Integer::from_i64(env, flags as i64)
}

#[otter::nif]
fn select_stop<'a>(env: Env<'a>, arc: ResourceArc<FdResource>) -> Integer<'a> {
    let pid = Pid::self_(env);
    let ref_term = TypedTerm::Reference(Reference::new(env));
    let flags = otter::select::select(
        env, arc.a.as_raw_fd(), NifSelectFlags::STOP, &arc, &pid, ref_term,
    );
    Integer::from_i64(env, flags as i64)
}

#[otter::nif]
fn select_stop_count<'a>(env: Env<'a>, arc: ResourceArc<FdResource>) -> Integer<'a> {
    Integer::from_i64(env, arc.stop_count.load(Ordering::Relaxed) as i64)
}

// select_x with a custom notification message. Selects READ on the fd, then
// writes to its peer so the fd becomes readable — the BEAM then delivers
// `msg` (not the default {select,...} tuple) to the calling process.
#[otter::nif]
fn select_x_register<'a>(env: Env<'a>, arc: ResourceArc<FdResource>, msg: TypedTerm<'a>) -> Integer<'a> {
    use std::io::Write;
    let pid = Pid::self_(env);
    // CUSTOM_MSG is required for select_x to deliver `msg` itself; without it
    // the BEAM sends the default {select,...} tuple with msg nested as the ref.
    let flags = otter::select::select_x(
        env, arc.a.as_raw_fd(), NifSelectFlags::READ | NifSelectFlags::CUSTOM_MSG,
        &arc, &pid, msg, None,
    );
    let mut peer = &arc.b;
    let _ = peer.write_all(b"x");
    Integer::from_i64(env, flags as i64)
}

// --- port_send/2 --------------------------------------------------------
// Send a command to a port via enif_port_command. The caller process owns
// the port (opened by the test), so the command is permitted; the binary is
// copied into the port's input. Returns ok if accepted.

#[otter::nif]
fn port_send<'a>(env: Env<'a>, port: Port, data: Binary<'a>) -> Atom {
    if env.port_command(&port, env, data) {
        otter::atom![ok]
    } else {
        otter::atom![error]
    }
}

// --- test_time/0 --------------------------------------------------------
// Exercise the time module: monotonic_time, time_offset, convert_time_unit
// across the TimeUnit variants.

#[otter::nif]
fn test_time(_env: Env) -> Atom {
    use otter::time::{convert_time_unit, monotonic_time, time_offset, TimeUnit};

    // Monotonic time does not go backwards.
    let t1 = monotonic_time(TimeUnit::Nanosecond);
    let t2 = monotonic_time(TimeUnit::Nanosecond);
    assert!(t2 >= t1);

    // time_offset is callable (monotonic + offset = system time).
    let _ = time_offset(TimeUnit::Millisecond);

    // Unit conversion is exact for these ratios.
    assert_eq!(convert_time_unit(1, TimeUnit::Second, TimeUnit::Nanosecond), 1_000_000_000);
    assert_eq!(convert_time_unit(1000, TimeUnit::Millisecond, TimeUnit::Second), 1);

    otter::atom![ok]
}

// --- test_consume_timeslice/0 -------------------------------------------
// Drive enif_consume_timeslice to exhaustion. Consuming 100% repeatedly
// must eventually report the timeslice used up (returns true).

#[otter::nif]
fn test_consume_timeslice(env: Env) -> Atom {
    for _ in 0..100 {
        if env.consume_timeslice(100) {
            return otter::atom![ok];
        }
    }
    otter::atom![error]
}

// --- monitor / down callback --------------------------------------------
// A resource that monitors a process via ResourceArc::monitor. When the
// monitored process exits, the BEAM dispatches to Resource::down on a
// scheduler thread; down() bumps a counter the Erlang side polls. Mirrors
// the select-stop test for the other resource extern "C" callback.
struct MonitorResource {
    down_count: AtomicUsize,
}

static MONITOR_RESOURCE_TYPE: OnceLock<ResourceTypeHandle> = OnceLock::new();

impl Resource for MonitorResource {
    fn resource_type_handle() -> &'static OnceLock<ResourceTypeHandle> {
        &MONITOR_RESOURCE_TYPE
    }

    fn down<'a>(&'a self, _env: Env<'a>, _pid: Pid, _monitor: otter::resource::Monitor) {
        self.down_count.fetch_add(1, Ordering::Relaxed);
    }
}

#[otter::nif]
fn monitor_resource_new(_env: Env) -> ResourceArc<MonitorResource> {
    ResourceArc::from(MonitorResource { down_count: AtomicUsize::new(0) })
}

#[otter::nif]
fn monitor_pid<'a>(env: Env<'a>, arc: ResourceArc<MonitorResource>, pid: Pid) -> Atom {
    match arc.monitor(Some(env), &pid) {
        Some(_) => otter::atom![ok],
        None => otter::atom![error],
    }
}

#[otter::nif]
fn monitor_down_count<'a>(env: Env<'a>, arc: ResourceArc<MonitorResource>) -> Integer<'a> {
    Integer::from_i64(env, arc.down_count.load(Ordering::Relaxed) as i64)
}

fn on_load(env: Env, _load_info: Term) -> bool {
    otter::init_atoms!(env);
    otter::resource::register_resource_type::<HashMapResource>(env);
    otter::resource::register_resource_type::<PanickingResource>(env);
    otter::resource::register_resource_type::<FdResource>(env);
    otter::resource::register_resource_type::<MonitorResource>(env);
    true
}

// --- init ---------------------------------------------------------------

otter::init!("otter_demo__nif", [
    hello,
    add,
    echo,
    type_of,
    reverse_binary,
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
], load = on_load);
