//! Raw job occupancy, deliberately separate from legacy lane attribution.
//! Input inventories are observations, not proof of a complete workflow portfolio.
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct Job {
    pub id: Option<u64>,
    pub status: String,
    pub conclusion: String,
    pub started_at: String,
    pub completed_at: String,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Snapshot {
    pub run_id: u64,
    pub attempt: Option<u64>,
    pub inventory_present: bool,
    pub attempt_bound: bool,
    pub completed: bool,
    pub jobs: Vec<Job>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Occupancy {
    pub observed_runner_seconds: Option<u64>,
    pub counted_jobs: usize,
    pub duplicate_observations: usize,
    pub complete: bool,
    pub reasons: Vec<String>,
}

/// Strict whole-second UTC timestamps. Offsets/fractions are not silently
/// discarded: unsupported precision/zone is explicitly incomplete accounting.
fn timestamp(text: &str) -> Option<u64> {
    let b = text.as_bytes();
    if b.len() != 20
        || b[4] != b'-'
        || b[7] != b'-'
        || b[10] != b'T'
        || b[13] != b':'
        || b[16] != b':'
        || b[19] != b'Z'
    {
        return None;
    }
    let number = |start: usize, end: usize| -> Option<u64> {
        b[start..end].iter().try_fold(0, |n, c| {
            c.is_ascii_digit()
                .then_some(n * 10 + u64::from(c.wrapping_sub(b'0')))
        })
    };
    let (y, m, d, h, min, s) = (
        number(0, 4)?,
        number(5, 7)?,
        number(8, 10)?,
        number(11, 13)?,
        number(14, 16)?,
        number(17, 19)?,
    );
    if y < 1970 || !(1..=12).contains(&m) || h > 23 || min > 59 || s > 59 {
        return None;
    }
    let leap = |year: u64| {
        year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400))
    };
    let months = [
        31,
        if leap(y) { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    if d == 0 || d > months[(m - 1) as usize] {
        return None;
    }
    let prev = y - 1;
    let days = prev * 365 + prev / 4 - prev / 100
        + prev / 400
        + months[..(m - 1) as usize].iter().sum::<u64>()
        + d
        - 1;
    Some(days * 86400 + h * 3600 + min * 60 + s)
}

fn checked_add(total: Option<u64>, value: u64) -> Option<u64> {
    total.and_then(|n| n.checked_add(value))
}

pub(super) fn calculate(snapshots: &[Snapshot]) -> Occupancy {
    let mut result = Occupancy {
        observed_runner_seconds: Some(0),
        ..Occupancy::default()
    };
    let mut reasons = BTreeSet::new();
    let mut attempts: BTreeMap<u64, BTreeSet<u64>> = BTreeMap::new();
    let mut inventories: BTreeMap<(u64, Option<u64>), &Snapshot> = BTreeMap::new();
    let mut job_attempts: BTreeMap<u64, BTreeSet<(u64, u64)>> = BTreeMap::new();
    let mut jobs: BTreeMap<u64, (u64, Job, bool)> = BTreeMap::new();
    if snapshots.is_empty() {
        reasons.insert("empty_sample".to_string());
    }
    for snapshot in snapshots {
        let label = format!("run {}", snapshot.run_id);
        if let Some(previous) = inventories.insert((snapshot.run_id, snapshot.attempt), snapshot)
            && previous != snapshot
        {
            reasons.insert(format!("{label}: conflicting_attempt_inventory"));
        }
        if snapshot.run_id == 0 {
            reasons.insert(format!("{label}: invalid_run_id"));
        }
        match snapshot.attempt.filter(|a| *a > 0) {
            Some(a) => {
                attempts.entry(snapshot.run_id).or_default().insert(a);
            }
            None => {
                reasons.insert(format!("{label}: missing_attempt_identity"));
            }
        }
        if !snapshot.attempt_bound {
            reasons.insert(format!("{label}: latest_snapshot_attempt_unverified"));
        }
        if !snapshot.completed {
            reasons.insert(format!("{label}: unfinished_run"));
        }
        if !snapshot.inventory_present || snapshot.jobs.is_empty() {
            reasons.insert(format!("{label}: missing_or_empty_job_inventory"));
        }
        for job in &snapshot.jobs {
            let Some(id) = job.id.filter(|id| *id > 0) else {
                reasons.insert(format!("{label}: missing_job_identity"));
                continue;
            };
            if snapshot.attempt_bound
                && let Some(attempt) = snapshot.attempt.filter(|a| *a > 0)
            {
                let bindings = job_attempts.entry(id).or_default();
                bindings.insert((snapshot.run_id, attempt));
                if bindings.len() > 1 {
                    reasons.insert(format!("job {id}: ambiguous_attempt_binding"));
                }
            }
            match jobs.get_mut(&id) {
                Some((run_id, previous, conflict)) => {
                    if *run_id == snapshot.run_id && previous == job {
                        result.duplicate_observations += 1;
                    } else {
                        *conflict = true;
                        reasons.insert(format!("job {id}: conflicting_identity"));
                    }
                }
                None => {
                    jobs.insert(id, (snapshot.run_id, job.clone(), false));
                }
            }
        }
    }
    for (run, seen) in attempts {
        // Comparing count to maximum avoids enumerating attacker-sized ranges.
        if seen.last().copied() != Some(seen.len() as u64) {
            reasons.insert(format!("run {run}: missing_prior_attempts"));
        }
    }
    for (id, (_, job, conflict)) in jobs {
        if conflict {
            continue;
        }
        if job.status != "completed" {
            reasons.insert(format!("job {id}: unfinished_job"));
            continue;
        }
        // GitHub can attach scheduler timestamps to skipped, unallocated jobs.
        if job.conclusion == "skipped" {
            continue;
        }
        if job.conclusion.is_empty() {
            reasons.insert(format!("job {id}: missing_conclusion"));
            continue;
        }
        let duration = timestamp(&job.started_at)
            .zip(timestamp(&job.completed_at))
            .and_then(|(s, e)| e.checked_sub(s));
        match duration {
            Some(seconds) => {
                result.observed_runner_seconds =
                    checked_add(result.observed_runner_seconds, seconds);
                result.counted_jobs += 1;
            }
            None => {
                reasons.insert(format!("job {id}: invalid_or_missing_timestamps"));
            }
        }
    }
    if result.observed_runner_seconds.is_none() {
        reasons.insert("runner_seconds_overflow".to_string());
    }
    result.complete = reasons.is_empty();
    result.reasons = reasons.into_iter().collect();
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    fn job(id: u64) -> Job {
        Job {
            id: Some(id),
            status: "completed".into(),
            conclusion: "success".into(),
            started_at: "2026-11-12T00:00:00Z".into(),
            completed_at: "2026-11-12T00:01:00Z".into(),
        }
    }
    fn snapshot(jobs: Vec<Job>) -> Snapshot {
        Snapshot {
            run_id: 1,
            attempt: Some(1),
            inventory_present: true,
            attempt_bound: true,
            completed: true,
            jobs,
        }
    }
    #[test]
    fn parallel_and_failed_work_sum() {
        let mut failed = job(2);
        failed.conclusion = "failure".into();
        let mut cancelled = job(3);
        cancelled.conclusion = "cancelled".into();
        let r = calculate(&[snapshot(vec![job(1), failed, cancelled])]);
        assert_eq!(r.observed_runner_seconds, Some(180));
        assert!(r.complete);
    }
    #[test]
    fn skipped_work_does_not_fabricate_occupancy() {
        let mut j = job(1);
        j.conclusion = "skipped".into();
        assert_eq!(
            calculate(&[snapshot(vec![j])]).observed_runner_seconds,
            Some(0)
        );
    }
    #[test]
    fn duplicate_is_counted_once_and_conflict_excluded() {
        let r = calculate(&[snapshot(vec![job(1), job(1)])]);
        assert_eq!(r.observed_runner_seconds, Some(60));
        assert_eq!(r.duplicate_observations, 1);
        let mut j = job(1);
        j.completed_at = "2026-11-12T00:02:00Z".into();
        let r = calculate(&[snapshot(vec![job(1), j])]);
        assert!(!r.complete);
        assert_eq!(r.observed_runner_seconds, Some(0));
    }
    #[test]
    fn earlier_attempts_retained_not_multiplied() {
        let mut second = snapshot(vec![job(2)]);
        second.attempt = Some(2);
        assert!(!calculate(&[second.clone()]).complete);
        let r = calculate(&[snapshot(vec![job(1)]), second]);
        assert!(r.complete);
        assert_eq!(r.observed_runner_seconds, Some(120));
    }
    #[test]
    fn repeated_success_across_attempts_is_not_new_work() {
        let mut second = snapshot(vec![job(1), job(2)]);
        second.attempt = Some(2);
        let r = calculate(&[snapshot(vec![job(1)]), second]);
        assert_eq!(r.observed_runner_seconds, Some(120));
        assert!(!r.complete);
        assert!(
            r.reasons
                .iter()
                .any(|r| r.contains("ambiguous_attempt_binding"))
        );
    }
    #[test]
    fn incomplete_inventory_and_identity_never_complete() {
        assert!(!calculate(&[]).complete);
        assert!(!calculate(&[snapshot(vec![])]).complete);
        for mode in 0..5 {
            let mut s = snapshot(vec![job(1)]);
            match mode {
                0 => s.inventory_present = false,
                1 => s.attempt = None,
                2 => s.attempt_bound = false,
                3 => s.completed = false,
                _ => s.jobs[0].id = None,
            };
            assert!(!calculate(&[s]).complete);
        }
    }
    #[test]
    fn conflicting_attempt_inventory_is_incomplete() {
        let r = calculate(&[snapshot(vec![job(1)]), snapshot(vec![job(2)])]);
        assert!(!r.complete);
        assert!(
            r.reasons
                .iter()
                .any(|r| r.contains("conflicting_attempt_inventory"))
        );
    }

    #[test]
    fn timestamps_are_validated_not_truncated() {
        for bad in [
            "",
            "2026-11-12T00:01:00+01:00",
            "0001-01-01T00:00:00Z",
            "2026-11-12T00:01:00.1Z",
            "2026-02-30T00:01:00Z",
            "2026-11-12T24:00:00Z",
            "2026-11-11T00:01:00Z",
            "éééééééééé",
        ] {
            let mut j = job(1);
            j.completed_at = bad.into();
            assert!(!calculate(&[snapshot(vec![j])]).complete, "{bad}");
        }
        assert!(timestamp("2024-02-29T00:00:00Z").is_some());
        assert!(timestamp("2100-02-29T00:00:00Z").is_none());
    }
    #[test]
    fn running_job_cannot_supply_final_occupancy() {
        let mut j = job(1);
        j.status = "in_progress".into();
        let r = calculate(&[snapshot(vec![j])]);
        assert!(!r.complete);
        assert_eq!(r.observed_runner_seconds, Some(0));
    }
    #[test]
    fn arithmetic_overflow_stays_unknown() {
        assert_eq!(checked_add(Some(u64::MAX), 1), None);
        assert_eq!(checked_add(None, 1), None);
    }
}
