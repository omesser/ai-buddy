//! A sleep that macOS does not coalesce, for the moving tick.
//!
//! `std::thread::sleep` gets timer slack, so a 16 ms moving tick lands up to
//! 4 ms late. A one-shot kqueue timer with `NOTE_CRITICAL` fires on time and
//! leaves the thread as it was. `THREAD_TIME_CONSTRAINT_POLICY` also fires on
//! time, but `THREAD_STANDARD_POLICY` never gives the thread its slack back,
//! so every still tick after the first move runs fast and breaks #183.

use std::time::Duration;

/// Sleep `duration` with no timer slack. Falls back to `thread::sleep` if
/// the kqueue calls fail.
pub(crate) fn sleep_precisely(duration: Duration) {
    // SAFETY: plain syscalls on a descriptor this function owns and closes.
    let fired = unsafe {
        let kq = libc::kqueue();
        if kq < 0 {
            false
        } else {
            let timer = libc::kevent {
                ident: 0,
                filter: libc::EVFILT_TIMER,
                flags: libc::EV_ADD | libc::EV_ONESHOT,
                fflags: libc::NOTE_NSECONDS | libc::NOTE_CRITICAL,
                data: isize::try_from(duration.as_nanos())
                    .unwrap_or(isize::MAX)
                    .max(1),
                udata: std::ptr::null_mut(),
            };
            let mut event: libc::kevent = std::mem::zeroed();
            let n = libc::kevent(kq, &raw const timer, 1, &raw mut event, 1, std::ptr::null());
            libc::close(kq);
            n == 1 && event.flags & libc::EV_ERROR == 0
        }
    };
    if !fired {
        std::thread::sleep(duration);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Instant;

    const TICK: Duration = Duration::from_millis(16);

    fn percentile(mut gaps: Vec<f64>, p: f64) -> f64 {
        gaps.sort_by(f64::total_cmp);
        gaps[((gaps.len() - 1) as f64 * p).round() as usize]
    }

    /// The frame loop's still tick, a tick from each wake. Gap p50 in ms.
    fn still_p50(ticks: usize) -> f64 {
        let mut last = Instant::now();
        let gaps = (0..ticks)
            .map(|_| {
                thread::sleep(TICK);
                let gap = last.elapsed().as_secs_f64() * 1e3;
                last = Instant::now();
                gap
            })
            .collect();
        percentile(gaps, 0.5)
    }

    /// The frame loop's moving tick, paced from the last deadline. Gap p90 in ms.
    fn moving_p90(ticks: usize, wait: impl Fn(Duration)) -> f64 {
        let mut deadline = Instant::now();
        let mut last = Instant::now();
        let gaps = (0..=ticks)
            .map(|_| {
                let now = Instant::now();
                deadline = (deadline + TICK).max(now);
                wait(deadline - now);
                let gap = last.elapsed().as_secs_f64() * 1e3;
                last = Instant::now();
                gap
            })
            .skip(1)
            .collect();
        percentile(gaps, 0.9)
    }

    /// Still p50 before, moving p90, still p50 after, on a fresh thread.
    fn stretch(wait: impl Fn(Duration) + Send + 'static) -> (f64, f64, f64) {
        thread::spawn(move || (still_p50(60), moving_p90(200, wait), still_p50(60)))
            .join()
            .unwrap()
    }

    /// Moving ticks land on the deadline, and the still ticks after them keep
    /// the coalesced ~20 ms a fresh thread gets, not the realtime ~16 ms.
    #[test]
    fn a_moving_stretch_is_on_time_and_the_still_ticks_after_it_stay_coalesced() {
        let (before, moving, after) = stretch(sleep_precisely);
        assert!(moving <= 16.1, "moving p90 {moving:.2} ms");
        assert!(
            after >= 18.5,
            "still p50 after {after:.2} ms, before {before:.2} ms"
        );
    }

    // libc points at `mach2` for the timebase, a crate pulled in for no test.
    #[allow(deprecated)]
    fn set_time_constraint_policy() {
        // Nanoseconds per tick, as a ratio of two u32s.
        let mut timebase = [0u32; 2];
        // SAFETY: out-pointer, then this thread's port with the struct the
        // flavor reads.
        let status = unsafe {
            libc::mach_timebase_info(timebase.as_mut_ptr().cast());
            let ticks = |ns: u64| (ns * u64::from(timebase[1]) / u64::from(timebase[0])) as u32;
            let mut policy = libc::thread_time_constraint_policy {
                period: 0,
                computation: ticks(1_000_000),
                constraint: ticks(4_000_000),
                preemptible: 1,
            };
            libc::thread_policy_set(
                libc::pthread_mach_thread_np(libc::pthread_self()),
                libc::THREAD_TIME_CONSTRAINT_POLICY as _,
                (&raw mut policy).cast(),
                libc::THREAD_TIME_CONSTRAINT_POLICY_COUNT,
            )
        };
        assert_eq!(status, 0);
    }

    fn set_standard_policy() {
        // SAFETY: flavor 1 with a count of zero reads no struct.
        let status = unsafe {
            libc::thread_policy_set(
                libc::pthread_mach_thread_np(libc::pthread_self()),
                libc::THREAD_STANDARD_POLICY as _,
                std::ptr::null_mut(),
                0,
            )
        };
        assert_eq!(status, 0);
    }

    /// The comparison behind `sleep_precisely`. Run with
    /// `cargo test -p fidget --bin fidget probe_moving_waits -- --ignored --nocapture`.
    #[test]
    #[ignore = "a timing probe that prints numbers, not a pass/fail check"]
    fn probe_moving_waits() {
        let policy_toggle = |d| {
            set_time_constraint_policy();
            thread::sleep(d);
            set_standard_policy();
        };
        let waits = [
            ("thread::sleep", thread::sleep as fn(Duration)),
            ("policy toggle", policy_toggle),
            ("kqueue critical", sleep_precisely),
        ];
        for _ in 0..3 {
            for (name, wait) in waits {
                let (before, moving, after) = stretch(wait);
                println!(
                    "{name:16} still p50 before {before:5.2} | moving p90 {moving:5.2} | still p50 after {after:5.2}"
                );
            }
        }
    }
}
