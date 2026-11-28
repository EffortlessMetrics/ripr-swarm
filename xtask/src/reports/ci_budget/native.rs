//! Opt-in bounded native attempt observation. No retries or effect endpoints.
use super::{RawJob, RawRun, json_str};
use serde_json::{Value, json};
use std::collections::BTreeSet;

const MAX_RUNS: usize = 100;
const MAX_ATTEMPTS: u64 = 20;
const MAX_PAGES: u64 = 20;
const MAX_READS: usize = 2000;

struct Reader<F> {
    read: F,
    remaining: usize,
}
impl<F: FnMut(&[String]) -> Result<String, String>> Reader<F> {
    fn get(&mut self, args: &[String]) -> Result<Value, String> {
        if self.remaining == 0 {
            return Err("native_read_bound".into());
        }
        self.remaining -= 1;
        let text = (self.read)(args).map_err(|_| "native_read_unavailable".to_string())?;
        if text.len() > 4 * 1024 * 1024 {
            return Err("native_response_size_bound".into());
        }
        serde_json::from_str(&text).map_err(|_| "native_json_invalid".into())
    }
    fn api(&mut self, path: &str) -> Result<Value, String> {
        self.get(&[
            "api".into(),
            "--hostname".into(),
            "github.com".into(),
            "--method".into(),
            "GET".into(),
            path.into(),
        ])
    }
}
fn positive(v: &Value, key: &str) -> Result<u64, String> {
    v.get(key)
        .and_then(Value::as_u64)
        .filter(|n| *n > 0)
        .ok_or_else(|| format!("invalid_native_{key}"))
}
fn repository_name(name: &str) -> bool {
    let parts: Vec<_> = name.split('/').collect();
    parts.len() == 2
        && parts.iter().all(|s| {
            !s.is_empty()
                && *s != "."
                && *s != ".."
                && s.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
        })
}
fn valid_state(v: &Value) -> bool {
    match v["status"].as_str() {
        Some("completed") => matches!(
            v["conclusion"].as_str(),
            Some(
                "success"
                    | "failure"
                    | "neutral"
                    | "cancelled"
                    | "skipped"
                    | "timed_out"
                    | "action_required"
                    | "stale"
                    | "startup_failure"
            )
        ),
        Some("queued" | "in_progress" | "waiting" | "pending" | "requested") => {
            v.get("conclusion").is_some_and(Value::is_null)
        }
        _ => false,
    }
}
fn run_identity(v: &Value, repo: &Value, id: u64) -> Result<Value, String> {
    let sha = json_str(v, "head_sha");
    if positive(v, "id")? != id
        || v["repository"]["id"] != repo["id"]
        || v["repository"]["full_name"] != repo["full_name"]
        || sha.len() != 40
        || !sha.bytes().all(|b| b.is_ascii_hexdigit())
        || json_str(v, "event").is_empty()
    {
        return Err("native_run_identity_mismatch".into());
    }
    positive(v, "run_attempt")?;
    positive(v, "workflow_id")?;
    if !valid_state(v) {
        return Err("native_run_state_invalid".into());
    }
    Ok(json!([
        v["id"],
        v["repository"]["id"],
        v["repository"]["full_name"],
        v["head_sha"],
        v["workflow_id"],
        v["event"]
    ]))
}
fn state(v: &Value) -> Value {
    json!([
        v["run_attempt"],
        v["status"],
        v["conclusion"],
        v["updated_at"]
    ])
}
fn raw(v: &Value) -> Result<RawRun, String> {
    let a = positive(v, "run_attempt")?;
    Ok(RawRun {
        database_id: positive(v, "id")?,
        display_title: json_str(v, "display_title"),
        conclusion: json_str(v, "conclusion"),
        status: json_str(v, "status"),
        event: json_str(v, "event"),
        head_branch: json_str(v, "head_branch"),
        created_at: json_str(v, "created_at"),
        started_at: json_str(v, "run_started_at"),
        updated_at: json_str(v, "updated_at"),
        attempt: a,
        native_attempt: Some(a),
        attempt_bound: true,
        native_subject: Some(
            json!({"repository_id":v["repository"]["id"],"repository":v["repository"]["full_name"],"head_sha":v["head_sha"],"workflow_id":v["workflow_id"],"event":v["event"]}),
        ),
        ..RawRun::default()
    })
}

pub(super) fn collect<F: FnMut(&[String]) -> Result<String, String>>(
    workflow: &str,
    limit: usize,
    read: F,
) -> Result<Vec<RawRun>, String> {
    if limit > MAX_RUNS {
        return Err("--all-attempts --limit exceeds 100 runs".into());
    }
    let mut reader = Reader {
        read,
        remaining: MAX_READS,
    };
    // gh resolves repository placeholders natively; the destination is fixed even
    // when GH_HOST or the current checkout's remote names another host.
    let resolved = reader.api("repos/{owner}/{repo}")?;
    positive(&resolved, "id")?;
    let name = json_str(&resolved, "full_name");
    if !repository_name(&name) {
        return Err("native_repository_name_invalid".into());
    }
    let root = format!("repos/{name}");
    let repo = reader.api(&root)?;
    positive(&repo, "id")?;
    if repo["full_name"] != name || repo["id"] != resolved["id"] {
        return Err("native_repository_identity_mismatch".into());
    }
    let list = reader.get(&[
        "run".into(),
        "list".into(),
        "--repo".into(),
        format!("github.com/{name}"),
        "--workflow".into(),
        workflow.into(),
        "--limit".into(),
        limit.to_string(),
        "--json".into(),
        "databaseId".into(),
    ])?;
    let items = list.as_array().ok_or("native_run_list_invalid")?;
    if items.len() > limit {
        return Err("native_run_list_over_bound".into());
    }
    let mut seen = BTreeSet::new();
    let mut ids = Vec::new();
    for item in items {
        let id = positive(item, "databaseId")?;
        if !seen.insert(id) {
            return Err("duplicate_native_run_id".into());
        }
        ids.push(id);
    }
    let mut runs = Vec::new();
    for id in ids {
        let start = runs.len();
        if let Err(reason) = collect_run(&mut reader, &root, &repo, id, &mut runs) {
            if start == runs.len() {
                runs.push(RawRun {
                    database_id: id,
                    ..RawRun::default()
                });
            }
            for run in &mut runs[start..] {
                run.collection_reasons.push(reason.clone());
            }
        }
    }
    Ok(runs)
}
fn collect_run<F: FnMut(&[String]) -> Result<String, String>>(
    reader: &mut Reader<F>,
    root: &str,
    repo: &Value,
    id: u64,
    output: &mut Vec<RawRun>,
) -> Result<(), String> {
    let path = format!("{root}/actions/runs/{id}");
    let current = reader.api(&path)?;
    let identity = run_identity(&current, repo, id)?;
    let latest = positive(&current, "run_attempt")?;
    if latest > MAX_ATTEMPTS {
        return Err("native_attempt_bound".into());
    }
    for attempt in 1..=latest {
        let start = output.len();
        let attempt_path = format!("{path}/attempts/{attempt}");
        let result = collect_attempt(reader, &attempt_path, repo, id, attempt, &current, output);
        if let Err(reason) = result {
            if output.len() == start {
                output.push(RawRun {
                    database_id: id,
                    attempt,
                    native_attempt: Some(attempt),
                    ..RawRun::default()
                });
            }
            for run in &mut output[start..] {
                run.collection_reasons.push(reason.clone());
            }
        }
    }
    let closing = reader.api(&path)?;
    if run_identity(&closing, repo, id)? != identity || state(&current) != state(&closing) {
        return Err("native_current_run_changed".into());
    }
    Ok(())
}
fn collect_attempt<F: FnMut(&[String]) -> Result<String, String>>(
    reader: &mut Reader<F>,
    attempt_path: &str,
    repo: &Value,
    id: u64,
    attempt: u64,
    current: &Value,
    output: &mut Vec<RawRun>,
) -> Result<(), String> {
    let identity = run_identity(current, repo, id)?;
    let before = reader.api(attempt_path)?;
    if run_identity(&before, repo, id)? != identity
        || positive(&before, "run_attempt")? != attempt
        || (Some(attempt) == current["run_attempt"].as_u64() && state(&before) != state(current))
    {
        return Err("native_attempt_identity_mismatch".into());
    }
    let mut run = raw(&before)?;
    let jobs_result = collect_jobs(
        reader,
        attempt_path,
        id,
        attempt,
        &current["head_sha"],
        &mut run.jobs,
    );
    run.inventory_present = jobs_result.is_ok();
    output.push(run);
    jobs_result?;
    let after = reader.api(attempt_path)?;
    if run_identity(&after, repo, id)? != identity || state(&before) != state(&after) {
        return Err("native_attempt_changed".into());
    }
    Ok(())
}
fn collect_jobs<F: FnMut(&[String]) -> Result<String, String>>(
    reader: &mut Reader<F>,
    path: &str,
    id: u64,
    attempt: u64,
    sha: &Value,
    jobs: &mut Vec<RawJob>,
) -> Result<(), String> {
    let mut total = None;
    let mut ids = BTreeSet::new();
    for page in 1..=MAX_PAGES {
        let value = reader.api(&format!("{path}/jobs?per_page=100&page={page}"))?;
        let count = positive(&value, "total_count")?;
        if count > MAX_PAGES * 100 {
            return Err("native_job_bound".into());
        }
        if total.is_some_and(|t| t != count) {
            return Err("native_job_total_changed".into());
        }
        total = Some(count);
        let rows = value["jobs"].as_array().ok_or("native_jobs_missing")?;
        let expected = (count - (page - 1) * 100).min(100) as usize;
        if rows.len() != expected {
            return Err("native_job_denominator_mismatch".into());
        }
        for v in rows {
            let job_id = positive(v, "id")?;
            if positive(v, "run_id")? != id
                || positive(v, "run_attempt")? != attempt
                || &v["head_sha"] != sha
            {
                return Err("native_job_identity_mismatch".into());
            }
            let duplicate = !ids.insert(job_id);
            if !valid_state(v) {
                return Err("native_job_state_invalid".into());
            }
            jobs.push(RawJob {
                database_id: Some(job_id),
                name: json_str(v, "name"),
                status: json_str(v, "status"),
                conclusion: json_str(v, "conclusion"),
                started_at: json_str(v, "started_at"),
                completed_at: json_str(v, "completed_at"),
            });
            if duplicate {
                return Err("duplicate_native_job_id".into());
            }
        }
        if ids.len() as u64 == count {
            return Ok(());
        }
    }
    Err("native_job_page_bound".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    fn run(attempt: u64) -> Value {
        json!({"id":42,"repository":{"id":7,"full_name":"owner/repo"},"head_sha":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","workflow_id":9,"run_attempt":attempt,"event":"merge_group","status":"completed","conclusion":"failure","updated_at":"2026-11-12T00:02:00Z"})
    }
    fn job(attempt: u64, id: u64) -> Value {
        json!({"id":id,"run_id":42,"run_attempt":attempt,"head_sha":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","status":"completed","conclusion":"failure","name":"new hosted worker","started_at":"2026-11-12T00:00:00Z","completed_at":"2026-11-12T00:01:00Z"})
    }
    fn fixture() -> Vec<(String, Value)> {
        let root = "repos/owner/repo/actions/runs/42";
        let mut r = vec![
            (
                "api --hostname github.com --method GET repos/{owner}/{repo}".into(),
                json!({"id":7,"full_name":"owner/repo"}),
            ),
            (
                "api --hostname github.com --method GET repos/owner/repo".into(),
                json!({"id":7,"full_name":"owner/repo"}),
            ),
            (
                "run list --repo github.com/owner/repo --workflow routed --limit 40 --json databaseId".into(),
                json!([{"databaseId":42}]),
            ),
            (format!("api --hostname github.com --method GET {root}"), run(2)),
        ];
        for a in 1..=2 {
            r.push((
                format!("api --hostname github.com --method GET {root}/attempts/{a}"),
                run(a),
            ));
            r.push((
                format!("api --hostname github.com --method GET {root}/attempts/{a}/jobs?per_page=100&page=1"),
                json!({"total_count":1,"jobs":[job(a,100+a)]}),
            ));
            r.push((
                format!("api --hostname github.com --method GET {root}/attempts/{a}"),
                run(a),
            ));
        }
        r.push((
            format!("api --hostname github.com --method GET {root}"),
            run(2),
        ));
        r
    }
    fn observe(rows: Vec<(String, Value)>) -> Result<Vec<RawRun>, String> {
        let mut queue: VecDeque<_> = rows.into();
        collect("routed", 40, |args| {
            let (expected, value) = queue.pop_front().ok_or("unexpected read")?;
            if args.join(" ") != expected {
                return Err("unexpected fixture route".into());
            }
            Ok(value.to_string())
        })
    }
    #[test]
    fn native_two_attempts_sum_actual_work() -> Result<(), String> {
        let runs = observe(fixture())?;
        assert_eq!(runs.len(), 2);
        let report = super::super::build_report("routed", 40, &runs);
        assert_eq!(report.occupancy.observed_runner_seconds, Some(120));
        assert!(report.occupancy.complete);
        let json = super::super::report_json(&report);
        assert_eq!(json["runner_occupancy"]["portfolio_complete"], false);
        Ok(())
    }
    #[test]
    fn unavailable_early_attempt_retains_later_observed_work() -> Result<(), String> {
        let mut f = fixture();
        f[4].1 = Value::Null;
        f.drain(5..7);
        let report = super::super::build_report("routed", 40, &observe(f)?);
        assert_eq!(report.occupancy.observed_runner_seconds, Some(60));
        assert!(!report.occupancy.complete);
        Ok(())
    }
    #[test]
    fn native_identity_and_inventory_fail_closed() -> Result<(), String> {
        for variant in 0..15 {
            let mut f = fixture();
            match variant {
                0 => f[3].1["repository"]["id"] = json!(8),
                1 => f[4].1["run_attempt"] = json!(2),
                2 => f[5].1["jobs"][0]["run_id"] = json!(43),
                3 => f[5].1["jobs"][0]["run_attempt"] = json!(2),
                4 => f[5].1["jobs"][0]["head_sha"] = json!("wrong"),
                5 => f[5].1["total_count"] = json!(2),
                6 => f[5].1["total_count"] = json!(0),
                7 => f[5].1["total_count"] = json!(2001),
                8 => f[6].1["conclusion"] = json!("success"),
                9 => f[10].1["run_attempt"] = json!(3),
                10 => f[3].1["run_attempt"] = json!(21),
                12 => f[5].1["jobs"][0]["status"] = json!("invented"),
                13 => f[4].1["conclusion"] = Value::Null,
                14 => f[3].1["status"] = json!("invented"),
                _ => {
                    f[5].1["jobs"][0]
                        .as_object_mut()
                        .ok_or("fixture")?
                        .remove("conclusion");
                }
            }
            let runs = observe(f)?;
            let report = super::super::build_report("routed", 40, &runs);
            assert!(!report.occupancy.complete, "variant {variant}");
            assert!(!report.occupancy.reasons.is_empty());
        }
        Ok(())
    }
    #[test]
    fn otherwise_valid_closing_state_changes_require_the_native_brackets() -> Result<(), String> {
        for (index, reason) in [
            (6, "native_attempt_changed"),
            (10, "native_current_run_changed"),
        ] {
            for field in ["conclusion", "updated_at"] {
                let mut f = fixture();
                f[index].1[field] = if field == "conclusion" {
                    json!("success")
                } else {
                    json!("2026-01-01T00:02:00Z")
                };
                assert!(
                    valid_state(&f[index].1),
                    "control must retain a valid native state"
                );
                let runs = observe(f)?;
                assert!(
                    runs[0].collection_reasons.iter().any(|r| r == reason),
                    "{index}/{field}: {runs:?}"
                );
                let report = super::super::build_report("routed", 40, &runs);
                assert!(!report.occupancy.complete, "{index}/{field}");
            }
        }
        Ok(())
    }
    #[test]
    fn paginated_jobs_are_complete_and_duplicates_refuse() -> Result<(), String> {
        for duplicate in [false, true] {
            let mut f = fixture();
            f[5].1 = json!({"total_count":101,"jobs":(1..=100).map(|i|job(1,1000+i)).collect::<Vec<_>>()});
            f.insert(6,("api --hostname github.com --method GET repos/owner/repo/actions/runs/42/attempts/1/jobs?per_page=100&page=2".into(),json!({"total_count":101,"jobs":[job(1,if duplicate {1001}else{1101})]})));
            let report = super::super::build_report("routed", 40, &observe(f)?);
            assert_eq!(report.occupancy.complete, !duplicate);
            if !duplicate {
                assert_eq!(report.occupancy.observed_runner_seconds, Some(6120));
            }
        }
        Ok(())
    }
    #[test]
    fn unavailable_pages_retain_observed_work_but_never_complete() -> Result<(), String> {
        let mut f = fixture();
        f.truncate(8);
        let report = super::super::build_report("routed", 40, &observe(f)?);
        assert_eq!(report.occupancy.observed_runner_seconds, Some(60));
        assert!(!report.occupancy.complete);
        assert!(
            report
                .occupancy
                .reasons
                .iter()
                .any(|s| s.contains("unavailable"))
        );
        Ok(())
    }
    #[test]
    fn repository_and_run_list_are_not_inferred() {
        let mut f = fixture();
        f[0].1["full_name"] = json!("../repo");
        assert!(observe(f).is_err());
        let mut f = fixture();
        f[2].1 = json!([{"databaseId":42},{"databaseId":42}]);
        assert!(observe(f).is_err());
    }
    #[test]
    fn changed_page_denominator_and_conflicting_job_are_incomplete() -> Result<(), String> {
        for conflict in [false, true] {
            let mut f = fixture();
            if conflict {
                let mut second = job(1, 101);
                second["completed_at"] = json!("2026-11-12T00:02:00Z");
                f[5].1 = json!({"total_count":2,"jobs":[job(1,101),second]});
            } else {
                f[5].1 = json!({"total_count":101,"jobs":(1..=100).map(|i|job(1,1000+i)).collect::<Vec<_>>()});
                f.insert(6,("api --hostname github.com --method GET repos/owner/repo/actions/runs/42/attempts/1/jobs?per_page=100&page=2".into(),json!({"total_count":100,"jobs":[]})));
            }
            let report = super::super::build_report("routed", 40, &observe(f)?);
            assert!(!report.occupancy.complete);
            if conflict {
                assert_eq!(report.occupancy.observed_runner_seconds, Some(0));
            }
        }
        Ok(())
    }
    #[test]
    fn transport_read_and_sample_bounds_stop_before_more_reads() -> Result<(), String> {
        let mut calls = 0;
        let result = collect("routed", 101, |_| {
            calls += 1;
            Ok("null".into())
        });
        assert!(result.is_err());
        assert_eq!(calls, 0);
        let mut reader = Reader {
            read: |_: &[String]| Ok("{}".into()),
            remaining: 1,
        };
        assert!(reader.api("repos/owner/repo").is_ok());
        assert_eq!(
            reader.api("repos/owner/repo"),
            Err("native_read_bound".into())
        );
        let mut f = fixture();
        f[2].1 = json!([]);
        let report = super::super::build_report("routed", 40, &observe(f)?);
        assert!(!report.occupancy.complete);
        assert!(
            super::super::parse_options(&[
                "--all-attempts".into(),
                "--input".into(),
                "file".into()
            ])
            .is_err()
        );
        Ok(())
    }
}
