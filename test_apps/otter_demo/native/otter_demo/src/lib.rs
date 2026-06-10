use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use otter::env::{Env, OwnedEnv};
use otter::resource::{Resource, ResourceArc, ResourceTypeHandle};
use otter::term::Term;
use otter::types::{Atom, Binary, BinaryBuilder, Float, Integer, List, Map, Pid, Reference, Tuple};

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
// Term in, Term out — zero-cost passthrough.

#[otter::nif]
fn echo<'a>(_env: Env<'a>, val: Term<'a>) -> Term<'a> {
    val
}

// --- type_of/1 ----------------------------------------------------------
// Pattern match on Term to inspect the Erlang type.

#[otter::nif]
fn type_of(_env: Env, val: Term) -> Atom {
    match val {
        Term::Atom(_)      => otter::atom![atom],
        Term::Integer(_)   => otter::atom![integer],
        Term::Float(_)     => otter::atom![float],
        Term::Binary(_)    => otter::atom![binary],
        Term::Bitstring(_) => otter::atom![bitstring],
        Term::List(_)      => otter::atom![list],
        Term::Tuple(_)     => otter::atom![tuple],
        Term::Map(_)       => otter::atom![map],
        Term::Pid(_)       => otter::atom![pid],
        Term::Port(_)      => otter::atom![port],
        Term::Fun(_)       => otter::atom![fun],
        Term::Reference(_) => otter::atom![reference],
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
            Term::Integer(i) => Some(i64::try_from(i).unwrap()),
            _ => None,
        })
        .sum();
    Integer::from_i64(env, sum)
}

// --- test_eq/2 ----------------------------------------------------------
// Test PartialEq between two terms of the same type.

#[otter::nif]
fn test_eq<'a>(_env: Env<'a>, a: Term<'a>, b: Term<'a>) -> Atom {
    let result = match (a, b) {
        (Term::Atom(a), Term::Atom(b)) => a == b,
        (Term::Integer(a), Term::Integer(b)) => a == b,
        (Term::Float(a), Term::Float(b)) => a == b,
        (Term::Binary(a), Term::Binary(b)) => a == b,
        (Term::List(a), Term::List(b)) => a == b,
        (Term::Tuple(a), Term::Tuple(b)) => a == b,
        (Term::Map(a), Term::Map(b)) => a == b,
        (Term::Pid(a), Term::Pid(b)) => a == b,
        (Term::Reference(a), Term::Reference(b)) => a == b,
        _ => false,
    };
    // true/false are always pre-existing in the atom table
    atomize_bool(result)
}

// --- test_ord/2 ---------------------------------------------------------
// Test Ord between two terms of the same type.
// Returns less, equal, or greater.

#[otter::nif]
fn test_ord<'a>(_env: Env<'a>, a: Term<'a>, b: Term<'a>) -> Atom {
    use std::cmp::Ordering;
    let ord = match (a, b) {
        (Term::Atom(a), Term::Atom(b)) => a.cmp(&b),
        (Term::Integer(a), Term::Integer(b)) => a.cmp(&b),
        (Term::Float(a), Term::Float(b)) => a.cmp(&b),
        (Term::Binary(a), Term::Binary(b)) => a.cmp(&b),
        (Term::List(a), Term::List(b)) => a.cmp(&b),
        (Term::Tuple(a), Term::Tuple(b)) => a.cmp(&b),
        (Term::Map(a), Term::Map(b)) => a.cmp(&b),
        (Term::Pid(a), Term::Pid(b)) => a.cmp(&b),
        (Term::Reference(a), Term::Reference(b)) => a.cmp(&b),
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
fn test_debug<'a>(env: Env<'a>, val: Term<'a>) -> Binary<'a> {
    let s = match val {
        Term::Atom(v) => format!("{:?}", v),
        Term::Integer(v) => format!("{:?}", v),
        Term::Float(v) => format!("{:?}", v),
        Term::Binary(v) => format!("{:?}", v),
        Term::Bitstring(v) => format!("{:?}", v),
        Term::List(v) => format!("{:?}", v),
        Term::Tuple(v) => format!("{:?}", v),
        Term::Map(v) => format!("{:?}", v),
        Term::Pid(v) => format!("{:?}", v),
        Term::Port(v) => format!("{:?}", v),
        Term::Fun(v) => format!("{:?}", v),
        Term::Reference(v) => format!("{:?}", v),
    };
    Binary::from_bytes(env, s.as_bytes())
}

// --- test_try_from/1 ----------------------------------------------------
// Test TryFrom<Integer> for i64. Returns the value or the atom 'overflow'.

#[otter::nif]
fn test_try_from<'a>(env: Env<'a>, val: Integer<'a>) -> Term<'a> {
    match i64::try_from(val) {
        Ok(v) => Term::Integer(Integer::from_i64(env, v)),
        Err(_) => Term::Atom(otter::atom![overflow]),
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
fn reverse_list<'a>(_env: Env<'a>, list: List<'a>) -> Term<'a> {
    match list.reverse() {
        Some(rev) => Term::List(rev),
        None => Term::Atom(otter::atom![error]),
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
fn hm_get<'a>(env: Env<'a>, key: Binary<'a>, hm: ResourceArc<HashMapResource>) -> Term<'a> {
    match hm.map.lock().unwrap().get(key.as_bytes()) {
        Some(val) => {
            let ok: Term = otter::atom![ok].into();
            let bin: Term = Binary::from_bytes(env, val).into();
            Term::Tuple(Tuple::from_terms(env, [ok, bin]))
        }
        None => Term::Atom(otter::atom![error]),
    }
}

// --- test_map/0 ---------------------------------------------------------
// Exercise Map::new, put, get, update, remove, size, iter.

#[otter::nif]
fn test_map(env: Env) -> Atom {
    let m = Map::new(env);
    assert_eq!(m.size(), 0);

    let k1 = Atom::new(env, "x").unwrap();
    let v1 = Integer::from_i64(env, 1);
    let m = m.put(k1, v1);
    assert_eq!(m.size(), 1);

    // get
    match m.get(k1).unwrap() {
        Term::Integer(i) => assert_eq!(i64::try_from(i).unwrap(), 1),
        _ => panic!("expected integer"),
    }
    assert!(m.get(Atom::new(env, "missing").unwrap()).is_none());

    // update existing key
    let v2 = Integer::from_i64(env, 2);
    let m = m.update(k1, v2).unwrap();
    match m.get(k1).unwrap() {
        Term::Integer(i) => assert_eq!(i64::try_from(i).unwrap(), 2),
        _ => panic!("expected integer"),
    }

    // update missing key returns None
    assert!(m.update(Atom::new(env, "missing").unwrap(), v1).is_none());

    // put second key, iterate
    let k2 = Atom::new(env, "y").unwrap();
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
    let a = Term::Atom(Atom::new(env, "hello").unwrap());
    let b = Term::Integer(Integer::from_i64(env, 42));
    let t = Tuple::from_terms(env, [a, b]);

    assert_eq!(t.len(), 2);
    assert!(!t.is_empty());
    assert!(t.element(0) == a);
    assert!(t.element(1) == b);

    let empty = Tuple::from_terms(env, std::iter::empty::<Term>());
    assert_eq!(empty.len(), 0);
    assert!(empty.is_empty());

    otter::atom![ok]
}

// --- double_float/1 -----------------------------------------------------
// Float decode → f64 → Float encode roundtrip.

#[otter::nif]
fn double_float<'a>(env: Env<'a>, val: Float<'a>) -> Float<'a> {
    Float::from_f64(env, f64::from(val) * 2.0)
}

// --- test_pid/0 ---------------------------------------------------------
// Exercise Pid::self_, is_alive, as_nif_pid, whereis.

#[otter::nif]
fn test_pid(env: Env) -> Pid {
    let pid = Pid::self_(env);
    assert!(pid.is_alive(env));
    assert!(pid.as_nif_pid(env).is_some());

    // whereis — 'init' is always registered
    let init = Pid::whereis(env, Atom::new(env, "init").unwrap());
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
// Result<T, E> return type — Err is raised as an exception.

#[otter::nif]
fn divide<'a>(env: Env<'a>, a: Integer<'a>, b: Integer<'a>) -> Result<Integer<'a>, Atom> {
    let b_val = i64::try_from(b).unwrap();
    if b_val == 0 {
        Err(otter::atom![division_by_zero])
    } else {
        Ok(Integer::from_i64(env, i64::try_from(a).unwrap() / b_val))
    }
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
            Term::Atom(otter::atom![from_thread])
        });
    });
    otter::atom![ok]
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

fn on_load(env: Env, _load_info: Term) -> bool {
    otter::init_atoms!(env);
    otter::resource::register_resource_type::<HashMapResource>(env, "hashmap");
    otter::resource::register_resource_type::<PanickingResource>(env, "panicking");
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
    test_pid,
    new_ref,
    divide,
    dirty_cpu_thread_type,
    send_from_thread,
    panicking_resource_new,
], load = on_load);
