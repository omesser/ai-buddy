//! Scheduling policy for the frame-loop thread.
//!
//! A moving tick waits on `std::thread::sleep`, which macOS coalesces.
//! `THREAD_TIME_CONSTRAINT_POLICY` puts that thread on the realtime clock for
//! the moving stretch. `THREAD_STANDARD_POLICY` puts it back, so a perched
//! sprite keeps the coalesced wake.

use std::ffi::{c_int, c_uint, c_void};

use super::super::thread_policy::absolute_ticks;

/// Flavor 1. A count of zero is the standard policy; the same flavor with a
/// count reads the extended timeshare hint, and zero keeps the fair share.
const THREAD_STANDARD_POLICY: c_uint = 1;
const THREAD_TIME_CONSTRAINT_POLICY: c_uint = 2;

/// About a millisecond of tick work.
const COMPUTATION_NS: u64 = 1_000_000;
/// How late that work may finish. The kernel stores a computation under half
/// of this as that half, so one millisecond beside four is kept as two.
const CONSTRAINT_NS: u64 = 4_000_000;

#[link(name = "System", kind = "dylib")]
unsafe extern "C" {
    fn mach_timebase_info(info: *mut MachTimebase) -> c_int;
    fn thread_policy_set(
        thread: c_uint,
        flavor: c_uint,
        policy_info: *mut c_int,
        count: c_uint,
    ) -> c_int;
    fn pthread_self() -> *mut c_void;
    fn pthread_mach_thread_np(thread: *mut c_void) -> c_uint;
}

#[repr(C)]
struct MachTimebase {
    numerator: u32,
    denominator: u32,
}

#[repr(C)]
struct TimeConstraintPolicy {
    period: u32,
    computation: u32,
    constraint: u32,
    /// `boolean_t`, an `unsigned int`.
    preemptible: c_uint,
}

const TIME_CONSTRAINT_COUNT: c_uint = {
    let bytes = std::mem::size_of::<TimeConstraintPolicy>();
    let unit = std::mem::size_of::<c_int>();
    assert!(bytes.is_multiple_of(unit));
    (bytes / unit) as c_uint
};

/// Put this thread on the realtime clock. `0` is success.
#[must_use]
pub(super) fn set_thread_time_constraint_policy() -> i32 {
    let mut timebase = MachTimebase {
        numerator: 0,
        denominator: 0,
    };
    // SAFETY: `timebase` is a live `mach_timebase_info` out-pointer.
    let status = unsafe { mach_timebase_info(&raw mut timebase) };
    if status != 0 {
        return status;
    }
    let mut policy = TimeConstraintPolicy {
        // No separate CPU reservation. The constraint is the deadline for
        // the tick's own work; the wait stays `thread::sleep`.
        period: 0,
        computation: absolute_ticks(COMPUTATION_NS, timebase.numerator, timebase.denominator),
        constraint: absolute_ticks(CONSTRAINT_NS, timebase.numerator, timebase.denominator),
        preemptible: 1,
    };
    // SAFETY: the port is this thread, and the struct is the four integers
    // `THREAD_TIME_CONSTRAINT_POLICY` reads. The port needs no release.
    unsafe {
        thread_policy_set(
            pthread_mach_thread_np(pthread_self()),
            THREAD_TIME_CONSTRAINT_POLICY,
            (&raw mut policy).cast(),
            TIME_CONSTRAINT_COUNT,
        )
    }
}

/// Return this thread to the fair scheduler. `0` is success.
#[must_use]
pub(super) fn set_thread_standard_policy() -> i32 {
    // SAFETY: flavor 1 with a count of zero is `THREAD_STANDARD_POLICY`.
    // The kernel does not read the pointer, and the port needs no release.
    unsafe {
        thread_policy_set(
            pthread_mach_thread_np(pthread_self()),
            THREAD_STANDARD_POLICY,
            std::ptr::null_mut(),
            0,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Both directions are accepted, including a second moving stretch,
    /// so clearing the policy is not a one-way trip.
    #[test]
    fn moving_takes_the_time_constraint_and_still_returns_the_standard_policy() {
        let took = set_thread_time_constraint_policy();
        let cleared = set_thread_standard_policy();
        let took_again = set_thread_time_constraint_policy();
        let cleared_again = set_thread_standard_policy();
        assert_eq!(took, 0);
        assert_eq!(cleared, 0);
        assert_eq!(took_again, 0);
        assert_eq!(cleared_again, 0);
    }
}
