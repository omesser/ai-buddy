//! Mach absolute time for the frame-loop thread policy.
//!
//! `thread_policy_set` counts in ticks, not nanoseconds. A raw millisecond is
//! under the kernel's minimum computation on Apple Silicon, so the call is
//! rejected and the moving tick stays coalesced.

/// Ticks for `nanoseconds`. `numerator`/`denominator` is nanoseconds per tick.
pub(in crate::platform) fn absolute_ticks(
    nanoseconds: u64,
    numerator: u32,
    denominator: u32,
) -> u32 {
    let numerator = u64::from(numerator.max(1));
    let ticks = nanoseconds.saturating_mul(u64::from(denominator)) / numerator;
    u32::try_from(ticks).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_millisecond_is_a_million_ticks_when_a_tick_is_a_nanosecond() {
        assert_eq!(absolute_ticks(1_000_000, 1, 1), 1_000_000);
        assert_eq!(absolute_ticks(4_000_000, 1, 1), 4_000_000);
    }

    /// 125/3 nanoseconds per tick, the timebase Apple Silicon reports.
    #[test]
    fn one_millisecond_is_24000_ticks_on_the_apple_silicon_timebase() {
        assert_eq!(absolute_ticks(1_000_000, 125, 3), 24_000);
        assert_eq!(absolute_ticks(4_000_000, 125, 3), 96_000);
    }
}
