//! The pass/fail decision for a same-run A/B, separate from process execution
//! so a control can feed it a synthetic slowdown.

#[derive(Clone, Copy, Debug)]
pub(crate) struct Limits {
    /// The candidate's median may be at most this multiple of the baseline's.
    pub(crate) max_ratio: f64,
    /// A difference under this is timer and scheduler noise whatever the ratio.
    pub(crate) noise_floor_ms: u128,
    /// The candidate's median must finish within this, whatever the baseline did.
    pub(crate) ceiling_ms: u128,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_ratio: 1.5,
            noise_floor_ms: 2_000,
            ceiling_ms: 300_000,
        }
    }
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

pub(crate) struct Judged {
    pub(crate) verdict: Verdict,
    pub(crate) detail: String,
    pub(crate) baseline_ms: Option<u128>,
    pub(crate) candidate_ms: Option<u128>,
    pub(crate) ratio: Option<f64>,
}

pub(crate) fn median(samples: &[u128]) -> Option<u128> {
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    let middle = sorted.len() / 2;
    match sorted.len() {
        0 => None,
        len if len % 2 == 1 => sorted.get(middle).copied(),
        _ => {
            let low = sorted.get(middle.checked_sub(1)?)?;
            let high = sorted.get(middle)?;
            Some((low + high) / 2)
        }
    }
}

fn fail(detail: String) -> Judged {
    Judged {
        verdict: Verdict::Fail,
        detail,
        baseline_ms: None,
        candidate_ms: None,
        ratio: None,
    }
}

/// Positive analyzed evidence must hold independently on each measured
/// side: every measured check (cold and warm) must report at least one
/// finding. A one-sided zero — a fast empty candidate against a nonempty
/// baseline, or the reciprocal — means that side analyzed nothing, so the
/// timing comparison measured nothing. Returns the failure detail, or
/// `None` when both sides carry positive evidence in every check.
pub(crate) fn evidence_failure(
    baseline_counts: &[usize],
    candidate_counts: &[usize],
) -> Option<String> {
    if baseline_counts.is_empty() {
        return Some("baseline has no measured checks; nothing was analyzed".to_string());
    }
    if candidate_counts.is_empty() {
        return Some("candidate has no measured checks; nothing was analyzed".to_string());
    }
    if baseline_counts.contains(&0) {
        return Some(
            "baseline reported no findings in a measured check; nothing was analyzed on the baseline side"
                .to_string(),
        );
    }
    if candidate_counts.contains(&0) {
        return Some(
            "candidate reported no findings in a measured check; nothing was analyzed on the candidate side"
                .to_string(),
        );
    }
    None
}

/// Compares warm wall times taken on one runner in one job, so runner class
/// and load cancel. Fails on a ratio over the limit that also exceeds the
/// noise floor, or on a candidate over the absolute ceiling.
pub(crate) fn judge(baseline: &[u128], candidate: &[u128], limits: &Limits) -> Judged {
    let (Some(base), Some(cand)) = (median(baseline), median(candidate)) else {
        return fail("a side has no warm samples".to_string());
    };
    let ratio = cand as f64 / base.max(1) as f64;
    let (verdict, detail) = if cand > limits.ceiling_ms {
        (
            Verdict::Fail,
            format!(
                "candidate median {cand} ms is over the {} ms ceiling",
                limits.ceiling_ms
            ),
        )
    } else if ratio > limits.max_ratio && cand.saturating_sub(base) > limits.noise_floor_ms {
        (
            Verdict::Fail,
            format!(
                "candidate median {cand} ms is {ratio:.2}x the baseline's {base} ms (limit {:.2}x)",
                limits.max_ratio
            ),
        )
    } else {
        (
            Verdict::Pass,
            format!(
                "candidate median {cand} ms against baseline {base} ms ({ratio:.2}x, limit {:.2}x)",
                limits.max_ratio
            ),
        )
    };
    Judged {
        verdict,
        detail,
        baseline_ms: Some(base),
        candidate_ms: Some(cand),
        ratio: Some(ratio),
    }
}
