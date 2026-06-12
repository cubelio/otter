//! BEAM time functions.
//!
//! Wraps `enif_monotonic_time`, `enif_time_offset`, and
//! `enif_convert_time_unit`.

pub use crate::sys::{NifTime as Time, NifTimeUnit as TimeUnit};

/// Return the current BEAM monotonic time in the given unit.
///
/// Wraps `enif_monotonic_time`.
pub fn monotonic_time(unit: TimeUnit) -> Time {
    unsafe { crate::enif::monotonic_time(unit) }
}

/// Return the current BEAM time offset in the given unit.
///
/// `monotonic_time + time_offset = system_time` (Erlang system time).
///
/// Wraps `enif_time_offset`.
pub fn time_offset(unit: TimeUnit) -> Time {
    unsafe { crate::enif::time_offset(unit) }
}

/// Convert a time value from one unit to another.
///
/// Wraps `enif_convert_time_unit`.
pub fn convert_time_unit(val: Time, from: TimeUnit, to: TimeUnit) -> Time {
    unsafe { crate::enif::convert_time_unit(val, from, to) }
}
