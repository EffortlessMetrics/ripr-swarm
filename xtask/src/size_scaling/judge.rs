//! The pass/fail decision, separate from process execution so a control can
//! feed it a synthetic quadratic cost.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Outcome {
    Completed,
    Deadline,
    Signalled,
    Exit(i32),
    /// Exited normally but reported nothing for the changed file, or named
    /// the large file in a limitation: the row measured a refusal or nothing.
    NotAnalyzed,
}

impl Outcome {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Deadline => "deadline",
            Self::Signalled => "ended_by_signal",
            Self::Exit(_) => "unexpected_exit",
            Self::NotAnalyzed => "not_analyzed",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Sample {
    pub(crate) bytes: usize,
    pub(crate) millis: u128,
    pub(crate) outcome: Outcome,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Verdict {
    Pass,
    Fail,
}

impl Verdict {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Fail => "fail",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Limits {
    /// Allowed cost multiple per doubling of size. Linear is 2.0; the slack
    /// above it absorbs timer noise and fixed per-run cost.
    pub(crate) max_doubling_ratio: f64,
    /// Below this the smallest step is timer noise, so growth is measured
    /// from the floor instead of from a few milliseconds.
    pub(crate) noise_floor_ms: u128,
    /// A largest step this fast says nothing about growth and passes.
    pub(crate) quiet_ms: u128,
    /// The largest step must finish within this, whatever its growth.
    pub(crate) ceiling_ms: u128,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_doubling_ratio: 2.45,
            noise_floor_ms: 250,
            quiet_ms: 1_000,
            ceiling_ms: 60_000,
        }
    }
}

pub(crate) struct Judged {
    pub(crate) verdict: Verdict,
    pub(crate) detail: String,
    pub(crate) growth: Option<f64>,
    pub(crate) allowed: Option<f64>,
}

fn fail(detail: String) -> Judged {
    Judged {
        verdict: Verdict::Fail,
        detail,
        growth: None,
        allowed: None,
    }
}

pub(crate) fn judge(samples: &[Sample], limits: &Limits) -> Judged {
    let (Some(first), Some(last)) = (samples.first(), samples.last()) else {
        return fail("no samples were taken".to_string());
    };
    if first.bytes == 0 || last.bytes <= first.bytes {
        return fail("sample sizes do not grow".to_string());
    }
    if let Some(bad) = samples
        .iter()
        .find(|sample| sample.outcome != Outcome::Completed)
    {
        return fail(format!(
            "{} bytes ended {}{}",
            bad.bytes,
            bad.outcome.as_str(),
            match bad.outcome {
                Outcome::Exit(code) => format!(" (exit {code})"),
                _ => String::new(),
            }
        ));
    }
    if last.millis > limits.ceiling_ms {
        return fail(format!(
            "{} bytes took {} ms, over the {} ms ceiling",
            last.bytes, last.millis, limits.ceiling_ms
        ));
    }
    let doublings = (last.bytes as f64 / first.bytes as f64).log2();
    let allowed = limits.max_doubling_ratio.powf(doublings);
    let base = first.millis.max(limits.noise_floor_ms);
    let growth = last.millis as f64 / base as f64;
    // A run this short carries no signal about growth.
    if last.millis <= limits.quiet_ms {
        return Judged {
            verdict: Verdict::Pass,
            detail: format!(
                "largest step {} ms, under the {} ms quiet limit",
                last.millis, limits.quiet_ms
            ),
            growth: Some(growth),
            allowed: Some(allowed),
        };
    }
    let (verdict, word) = if growth > allowed {
        (Verdict::Fail, "super-linear")
    } else {
        (Verdict::Pass, "within the allowance")
    };
    Judged {
        verdict,
        detail: format!(
            "{} ms to {} ms over {:.0}x the size: {word}",
            first.millis,
            last.millis,
            last.bytes as f64 / first.bytes as f64
        ),
        growth: Some(growth),
        allowed: Some(allowed),
    }
}
