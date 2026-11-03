//! Bounded-noise growth measurements for complexity regression tests.
//!
//! Both sizes run the same number of repetitions. The batch grows until the
//! fastest small sample reaches 20 ms, so the 2 ms allowance is at most 10%
//! of that measurement. Samples are short and numerous (the minimum of 15
//! alternating pairs) rather than few and long: hosted runners showed the
//! larger size consistently 3.1-3.4x slower at 2x input (#11238). The cause
//! is not established; this design keeps each slice short so a sustained
//! slowdown within one long batch has less room to accumulate, and takes the
//! minimum over many alternating slices. An unresolved measurement fails instead of passing
//! below the floor. Interleaved samples reduce order effects; they cannot
//! eliminate asymmetric scheduling delays or establish a universal bound.
//! Nontermination is handled by the test runner, not by this assertion.

use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

#[cfg(test)]
static MEASUREMENT_PASS: AtomicU32 = AtomicU32::new(0);

const NOISE_ALLOWANCE: Duration = Duration::from_millis(2);
const MIN_SAMPLE: Duration = Duration::from_millis(20);
const RUNS: usize = 15;
const LONG_SLICE: Duration = Duration::from_millis(100);
const LONG_RUNS: usize = 3;
const MAX_REPETITIONS: usize = 65_536;

/// Compare work at two sizes, preserving the caller's semantic assertions on
/// every repetition. At 2x input, factor 3 separates linear from quadratic
/// growth; factor 6 permits quadratic work but rejects cubic growth once the
/// measured ratio exceeds the factor plus the bounded noise allowance.
pub(crate) fn assert_growth_within(
    label: &str,
    small: usize,
    large: usize,
    factor: u32,
    work: impl FnMut(usize),
) {
    let epoch = Instant::now();
    assert_growth_with_clock(label, small, large, factor, work, || epoch.elapsed());
}

fn assert_growth_with_clock(
    label: &str,
    small: usize,
    large: usize,
    factor: u32,
    mut work: impl FnMut(usize),
    mut now: impl FnMut() -> Duration,
) {
    assert_measured_growth(label, small, large, factor, |size, repetitions| {
        let started = now();
        for _ in 0..repetitions {
            work(size);
        }
        now().saturating_sub(started)
    });
}

fn measure_batch(
    small: usize,
    large: usize,
    repetitions: usize,
    runs: usize,
    measure: &mut impl FnMut(usize, usize) -> Duration,
) -> (Duration, Duration) {
    let mut small_time = Duration::MAX;
    let mut large_time = Duration::MAX;
    let mut sample = 0;
    let mut effective_runs = runs;
    while sample < effective_runs {
        // Alternate order without dividing by independently calibrated
        // repetition counts: both samples must represent identical work.
        let (s, l) = if sample % 2 == 0 {
            (measure(small, repetitions), measure(large, repetitions))
        } else {
            let l = measure(large, repetitions);
            (measure(small, repetitions), l)
        };
        small_time = small_time.min(s);
        large_time = large_time.min(l);
        // A first slice already far above the floor needs no more samples
        // than the original long-batch design took.
        if sample == 0 && s >= LONG_SLICE {
            effective_runs = LONG_RUNS;
        }
        sample += 1;
    }
    (small_time, large_time)
}

fn growth_within_factor(small_time: Duration, large_time: Duration, factor: u32) -> bool {
    large_time <= small_time * factor + NOISE_ALLOWANCE
}

fn assert_measured_growth(
    label: &str,
    small: usize,
    large: usize,
    factor: u32,
    mut measure: impl FnMut(usize, usize) -> Duration,
) {
    assert!(small > 0 && large > small && factor > 0);
    #[cfg(test)]
    MEASUREMENT_PASS.store(0, Ordering::Relaxed);
    let mut repetitions = 1;
    loop {
        let (small_time, large_time) = measure_batch(small, large, repetitions, RUNS, &mut measure);
        if small_time >= MIN_SAMPLE {
            eprintln!(
                "{label}: {repetitions} repetitions, size {small} {small_time:?}, \
                 size {large} {large_time:?}, factor {factor}, allowance {NOISE_ALLOWANCE:?}"
            );
            if growth_within_factor(small_time, large_time, factor) {
                return;
            }
            let retry_repetitions = repetitions * 2;
            #[cfg(test)]
            MEASUREMENT_PASS.store(1, Ordering::Relaxed);
            eprintln!(
                "{label}: ratio check failed; remeasuring once at {retry_repetitions} repetitions"
            );
            let (small_retry, large_retry) =
                measure_batch(small, large, retry_repetitions, RUNS, &mut measure);
            eprintln!(
                "{label}: remeasure size {small} {small_retry:?}, size {large} {large_retry:?}"
            );
            assert!(
                growth_within_factor(small_retry, large_retry, factor),
                "{label}: growth exceeded {factor}x plus {NOISE_ALLOWANCE:?} after remeasure: \
                 {small_retry:?} -> {large_retry:?}"
            );
            return;
        }
        assert!(
            repetitions < MAX_REPETITIONS,
            "{label}: measurement unresolved: small sample {small_time:?} remains below \
             {MIN_SAMPLE:?} after {repetitions} repetitions"
        );
        repetitions *= 2;
    }
}

#[cfg(test)]
mod tests {
    use super::{
        MEASUREMENT_PASS, assert_growth_with_clock, assert_growth_within, assert_measured_growth,
    };
    use std::cell::{Cell, RefCell};
    use std::sync::atomic::Ordering;
    use std::time::Duration;

    #[test]
    fn dispatcher_runs_both_sizes_with_the_same_calibrated_repetitions() {
        let elapsed = Cell::new(Duration::ZERO);
        let sizes = RefCell::new(Vec::new());
        assert_growth_with_clock(
            "dispatch",
            1,
            2,
            3,
            |size| {
                sizes.borrow_mut().push(size);
                elapsed.set(elapsed.get() + Duration::from_millis(2 * size as u64));
            },
            || elapsed.get(),
        );
        // The first qualifying small batch is 16 * 2 ms. Each size runs
        // fifteen samples at every doubling: 15 * (1+2+4+8+16) = 465.
        let sizes = sizes.into_inner();
        assert_eq!(sizes.iter().filter(|&&size| size == 1).count(), 465);
        assert_eq!(sizes.iter().filter(|&&size| size == 2).count(), 465);
        assert_eq!(sizes.len(), 930);
        assert_eq!(&sizes[..6], &[1, 2, 2, 1, 1, 2]);
    }

    #[test]
    fn matched_2x_linear_work_passes_even_below_the_initial_floor() {
        assert_measured_growth("linear", 1, 2, 3, |size, repetitions| {
            Duration::from_micros(2 * size as u64 * repetitions as u64)
        });
    }

    #[test]
    #[should_panic(expected = "quadratic: growth exceeded")]
    fn matched_2x_quadratic_work_fails_even_below_the_initial_floor() {
        assert_measured_growth("quadratic", 1, 2, 3, |size, repetitions| {
            Duration::from_micros(2 * (size * size * repetitions) as u64)
        });
    }

    #[test]
    fn quadratic_glob_growth_passes() {
        assert_measured_growth("quadratic glob", 1, 2, 6, |size, repetitions| {
            Duration::from_micros(2 * (size * size * repetitions) as u64)
        });
    }

    #[test]
    #[should_panic(expected = "cubic glob: growth exceeded")]
    fn cubic_glob_growth_fails() {
        assert_measured_growth("cubic glob", 1, 2, 6, |size, repetitions| {
            Duration::from_micros(2 * (size * size * size * repetitions) as u64)
        });
    }

    #[test]
    #[should_panic(expected = "measurement unresolved")]
    fn zero_measurements_cannot_pass() {
        assert_measured_growth("zero clock", 1, 2, 3, |_, _| Duration::ZERO);
    }

    #[test]
    #[should_panic(expected = "measurement unresolved")]
    fn positive_measurements_stuck_below_floor_cannot_pass() {
        assert_measured_growth("unresolved clock", 1, 2, 3, |_, _| {
            Duration::from_millis(19)
        });
    }

    #[test]
    #[should_panic(expected = "stalled quadratic: growth exceeded")]
    fn one_stalled_small_sample_cannot_satisfy_calibration() {
        let mut first = true;
        assert_measured_growth("stalled quadratic", 1, 2, 3, |size, repetitions| {
            if first {
                first = false;
                Duration::from_secs(1)
            } else {
                Duration::from_micros(2 * (size * size * repetitions) as u64)
            }
        });
    }

    #[test]
    fn one_stalled_large_sample_is_remeasured_before_failing() {
        assert_measured_growth("stalled large remeasure", 1, 2, 3, |size, repetitions| {
            let base = Duration::from_millis(20 * size as u64 * repetitions as u64);
            if MEASUREMENT_PASS.load(Ordering::Relaxed) == 0 && size == 2 {
                base * 4
            } else {
                base
            }
        });
    }

    #[test]
    fn isolated_large_stall_does_not_reject_linear_work() {
        let mut stalled = false;
        assert_measured_growth("linear stall", 1, 2, 3, |size, repetitions| {
            if size == 2 && !stalled {
                stalled = true;
                Duration::from_secs(10)
            } else {
                Duration::from_millis(100 * (size * repetitions) as u64)
            }
        });
    }

    #[test]
    fn allowance_boundary_is_inclusive() {
        assert_measured_growth("boundary", 1, 2, 3, |size, _| {
            Duration::from_millis(if size == 1 { 20 } else { 62 })
        });
    }

    #[test]
    #[should_panic(expected = "over boundary: growth exceeded")]
    fn measurement_just_outside_allowance_fails() {
        assert_measured_growth("over boundary", 1, 2, 3, |size, _| {
            Duration::from_micros(if size == 1 { 20_000 } else { 62_001 })
        });
    }

    fn spin(units: usize) {
        let mut acc = 0usize;
        for i in 0..units {
            acc = std::hint::black_box(acc.wrapping_add(i));
        }
        std::hint::black_box(acc);
    }

    #[test]
    #[should_panic(expected = "quadratic stand-in: growth exceeded")]
    fn real_clock_rejects_quadratic_work_at_the_guard_sizes() {
        // Deliberately quadratic work through the real wall clock: doubling
        // the size quadruples the cost, so the 3x guard must still fail.
        assert_growth_within("quadratic stand-in", 1_000, 2_000, 3, |n| spin(n * n / 4));
    }
}
