//! Growth-ratio assertion for complexity regression tests.
//!
//! An absolute wall-clock bound (`elapsed < 10s`) fails on a slow or contended
//! runner without any code change, and passes a slow algorithm on a fast one.
//! Comparing the same work at two sizes on the same machine cancels runner
//! speed: a linear pass roughly doubles when the input doubles, a quadratic
//! one roughly quadruples, an exponential one explodes.

use std::time::{Duration, Instant};

/// Slack that absorbs timer and scheduler noise when both runs are fast.
const NOISE_FLOOR: Duration = Duration::from_millis(50);
const RUNS: usize = 3;

fn best_of(size: usize, work: &mut impl FnMut(usize)) -> Duration {
    (0..RUNS)
        .map(|_| {
            let started = Instant::now();
            work(size);
            started.elapsed()
        })
        .min()
        .unwrap_or_default()
}

/// Runs `work(small)` and `work(large)` three times each and asserts the best
/// large time stays within `factor` times the best small time plus a noise
/// floor. `work` should assert its own results so a wrong answer still fails.
pub(crate) fn assert_growth_within(
    label: &str,
    small: usize,
    large: usize,
    factor: u32,
    mut work: impl FnMut(usize),
) {
    let small_time = best_of(small, &mut work);
    let large_time = best_of(large, &mut work);
    assert!(
        large_time <= small_time * factor + NOISE_FLOOR,
        "{label}: size {small} took {small_time:?} but size {large} took {large_time:?}, \
         more than {factor}x plus {NOISE_FLOOR:?}"
    );
}

#[cfg(test)]
mod tests {
    use super::assert_growth_within;

    fn spin(units: usize) {
        let mut sink = 0u64;
        for i in 0..units as u64 {
            sink = std::hint::black_box(sink.wrapping_add(i));
        }
        std::hint::black_box(sink);
    }

    #[test]
    fn linear_work_passes() {
        assert_growth_within("linear", 5_000_000, 10_000_000, 3, spin);
    }

    #[test]
    #[should_panic(expected = "quadratic")]
    fn quadratic_work_fails() {
        // 1k vs 2k squared: 1M vs 4M units, well beyond 3x plus the floor
        // only when the work is large enough; scale it so the gap is seconds.
        assert_growth_within("quadratic", 4_000, 16_000, 3, |n| spin(n * n));
    }
}
