use crate::sys::{NifTime, NifTimeUnit};
use crate::enif::funcs;

/// Return the current BEAM monotonic time.
pub(crate) fn monotonic_time(unit: NifTimeUnit) -> NifTime {
    unsafe { (funcs().monotonic_time)(unit) }
}

/// Return the current BEAM time offset.
pub(crate) fn time_offset(unit: NifTimeUnit) -> NifTime {
    unsafe { (funcs().time_offset)(unit) }
}

/// Convert a time value from one unit to another.
pub(crate) fn convert_time_unit(val: NifTime, from: NifTimeUnit, to: NifTimeUnit) -> NifTime {
    unsafe { (funcs().convert_time_unit)(val, from, to) }
}
