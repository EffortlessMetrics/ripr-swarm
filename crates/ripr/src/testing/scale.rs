//! Growth-ratio assertion for complexity regression tests.
//!
//! An absolute wall-clock bound (`elapsed < 10s`) fails on a slow or contended
//! runner without any code change. Comparing the same work at two sizes in
//! one process removes the runner's absolute speed from the decision: a
//! linear pass roughly doubles when the input doubles and a quadratic one
//! roughly quadruples.
//!
//! What this does and does not establish: it rejects growth beyond `factor`
//! once the small run is longer than [`NOISE_FLOOR`] (a quadratic regression
//! at 2x input passes `factor = 3` only while the small run stays at or under
//! the floor), so pick sizes whose small run is clearly above it. The two
//! sizes are timed in separate groups, so a scheduler stall that hits only
//! one group still moves the ratio. A regression that does not terminate
//! (exponential backtracking) is not an assertion failure here; it ends at
//! the harness timeout.

use std::time::{Duration, Instant};

/// Slack that absorbs timer noise; the sizes should keep the small run above it.
const NOISE_FLOOR: Duration = Duration::from_millis(10);
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
    use std::time::{Duration, Instant};

    fn spin(units: usize) {
        let mut sink = 0u64;
        for i in 0..units as u64 {
            sink = std::hint::black_box(sink.wrapping_add(i));
        }
        std::hint::black_box(sink);
    }

    /// A unit count whose spin takes at least 40 ms on this machine, so the
    /// small run sits well above the noise floor whatever the runner speed.
    fn calibrated_units() -> usize {
        let mut units = 1_000_000;
        loop {
            let started = Instant::now();
            spin(units);
            if started.elapsed() >= Duration::from_millis(40) || units >= 1 << 33 {
                return units;
            }
            units *= 2;
        }
    }

    // The migrated tests double their input, so both controls use the same
    // 2x step: linear work doubles, quadratic work quadruples.
    #[test]
    fn matched_2x_linear_work_passes() {
        let unit = calibrated_units();
        assert_growth_within("linear", 1, 2, 3, |step| spin(unit * step));
    }

    #[test]
    #[should_panic(expected = "quadratic")]
    fn matched_2x_quadratic_work_fails() {
        let unit = calibrated_units();
        assert_growth_within("quadratic", 1, 2, 3, |step| spin(unit * step * step));
    }
}
