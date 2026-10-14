//! Bounded projection of the pinned UB Review packet. Does not execute proof.

use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Read,
    path::Path,
};

const MAX_BYTES: u64 = 8 * 1024 * 1024;
const MAX_ROWS: usize = 200;

fn text(value: &str) -> String {
    let mut output = String::new();
    for ch in value.chars().take(400) {
        match ch {
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '&' => output.push_str("&amp;"),
            '\\' | '`' | '*' | '_' | '[' | ']' | '#' | '|' => {
                output.push('\\');
                output.push(ch);
            }
            ch if ch.is_control() => output.push(' '),
            ch => output.push(ch),
        }
    }
    output
}

fn read_json(root: &Path, relative: &str) -> Result<Value, String> {
    let mut path = root.to_path_buf();
    for part in relative.split('/') {
        path.push(part);
        if fs::symlink_metadata(&path)
            .map_err(|e| e.to_string())?
            .file_type()
            .is_symlink()
        {
            return Err("symlink refused".into());
        }
    }
    let file = fs::File::open(&path).map_err(|e| e.to_string())?;
    let metadata = file.metadata().map_err(|e| e.to_string())?;
    if !metadata.is_file() || metadata.len() > MAX_BYTES {
        return Err("not a bounded regular file".into());
    }
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("artifact exceeds read bound".into());
    }
    serde_json::from_slice(&bytes).map_err(|e| e.to_string())
}

fn field<'a>(row: &'a Value, key: &str) -> Result<&'a str, String> {
    row.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("invalid {key}"))
}

fn array(value: &Value) -> Result<&Vec<Value>, String> {
    value.as_array().ok_or_else(|| "expected array".into())
}

#[derive(Default)]
struct Section {
    rows: Vec<String>,
    missing: usize,
}
impl Section {
    fn gap(&mut self, detail: impl AsRef<str>) {
        self.missing += 1;
        self.rows
            .push(format!("MISSING EVIDENCE: {}", detail.as_ref()));
    }
}

fn section(
    root: &Path,
    title: &str,
    relative: &str,
    produce: impl FnOnce(&Value, &mut Section) -> Result<(), String>,
) -> (String, usize) {
    let mut section = Section::default();
    if let Err(error) = read_json(root, relative).and_then(|value| produce(&value, &mut section)) {
        section.gap(format!("{relative}: unavailable or invalid ({error})"));
    }
    let mut report = format!("### {title}\n\nArtifact: {relative}\n\n");
    if section.rows.is_empty() {
        report.push_str("- No entries recorded. This is not executed proof.\n");
    }
    for row in section.rows.iter().take(MAX_ROWS) {
        report.push_str(&format!("- {}\n", text(row)));
    }
    if section.rows.len() > MAX_ROWS {
        report.push_str(&format!(
            "- {} additional rows withheld by the display bound; inspect the artifact.\n",
            section.rows.len() - MAX_ROWS
        ));
    }
    (report, section.missing)
}

fn render(root: &Path) -> String {
    let mut sections = Vec::new();
    sections.push(section(
        root,
        "Planned sensors",
        "plan.json",
        |plan, section| {
            let mut ids = BTreeSet::new();
            for sensor in array(&plan["sensors"])? {
                let id = field(sensor, "id")?;
                if !id
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
                    || !ids.insert(id)
                {
                    return Err("invalid or duplicate sensor ID".into());
                }
                match sensor["run"].as_bool() {
                    Some(false) => section
                        .rows
                        .push(format!("{id}: not planned ({})", field(sensor, "reason")?)),
                    Some(true) => {
                        let relative = format!("sensors/{id}/ub-review-sensor-status.json");
                        let receipt = read_json(root, &relative).and_then(|r| {
                            if field(&r, "sensor")? != id {
                                return Err("sensor identity mismatch".into());
                            }
                            field(&r, "status")?;
                            field(&r, "reason")?;
                            Ok(r)
                        });
                        match receipt {
                            Ok(r) => {
                                let detail = format!(
                                    "{id}: status={}, exit={}, timed_out={}; {}; {relative}",
                                    field(&r, "status")?,
                                    r["exit_code"],
                                    r["timed_out"],
                                    field(&r, "reason")?
                                );
                                if r["status"] != "ok"
                                    || r["exit_code"].as_i64() != Some(0)
                                    || r["timed_out"].as_bool() != Some(false)
                                {
                                    section.gap(detail);
                                } else {
                                    section.rows.push(detail);
                                }
                            }
                            Err(error) => section
                                .gap(format!("{id}: {relative} unavailable or invalid ({error})")),
                        }
                    }
                    None => return Err("invalid sensor run flag".into()),
                }
            }
            Ok(())
        },
    ));
    sections.push(section(
        root,
        "Model request validation",
        "review/observations.json",
        |values, section| {
            for row in array(values)? {
                if row["schema"] != "ub-review.observation.v1" {
                    return Err("unknown observation schema".into());
                }
                let source = field(row, "source")?;
                field(row, "id")?;
                if source == "proof-intent-validation" {
                    let evidence = array(&row["evidence"])?
                        .iter()
                        .map(|v| {
                            v.as_str()
                                .ok_or_else(|| "invalid observation evidence".to_owned())
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    section.gap(format!(
                        "{}: rejected before planning; {}",
                        field(row, "id")?,
                        evidence.join("; ")
                    ));
                }
            }
            Ok(())
        },
    ));
    sections.push(section(
        root,
        "Proof intent resolution",
        "review/proof_intents.json",
        |values, section| {
            for row in array(values)? {
                let status = field(row, "status")?;
                let detail = format!(
                    "{}: status={status}; {}; target={}",
                    field(row, "id")?,
                    field(row, "resolution_reason")?,
                    row["target"].as_str().unwrap_or("not recorded")
                );
                if matches!(status, "resolved" | "deduplicated" | "requested") {
                    section
                        .rows
                        .push(format!("{detail}; resolution is not execution"));
                } else {
                    section.gap(detail);
                }
            }
            Ok(())
        },
    ));
    let mut receipts = BTreeMap::new();
    sections.push(section(
        root,
        "Recorded proof results",
        "review/proof_receipts.json",
        |values, section| {
            for row in array(values)? {
                if row["schema"] != "ub-review.proof_receipt.v1" {
                    return Err("unknown proof receipt schema".into());
                }
                let id = field(row, "id")?;
                if receipts.contains_key(id) {
                    return Err("duplicate proof receipt ID".into());
                }
                let result = field(row, "result")?;
                let commands = array(&row["commands"])?;
                let mut outcomes = Vec::new();
                for cmd in commands {
                    outcomes.push(format!(
                        "{}: {}, exit={}, timed_out={}",
                        field(cmd, "side")?,
                        field(cmd, "status")?,
                        cmd["exit_code"],
                        cmd["timed_out"]
                    ));
                }
                let terminal = !commands.is_empty()
                    && commands.iter().all(|c| {
                        c["exit_code"].as_i64().is_some()
                            && c["timed_out"].as_bool() == Some(false)
                            && matches!(c["status"].as_str(), Some("passed" | "failed"))
                    });
                let head: Vec<_> = commands.iter().filter(|c| c["side"] == "head").collect();
                let base: Vec<_> = commands
                    .iter()
                    .filter(|c| c["side"] == "base-plus-tests")
                    .collect();
                let head_passed = head.len() == 1
                    && head[0]["status"] == "passed"
                    && head[0]["exit_code"].as_i64() == Some(0);
                let satisfied = terminal
                    && head_passed
                    && ((result == "head_passed" && commands.len() == 1)
                        || (result == "discriminating"
                            && commands.len() == 2
                            && base.len() == 1
                            && base[0]["status"] == "failed"
                            && base[0]["exit_code"].as_i64() != Some(0)));
                let kind = field(row, "kind")?;
                receipts.insert(
                    id.to_owned(),
                    (satisfied, kind.to_owned(), result.to_owned()),
                );
                let detail = format!(
                    "{id}: producer result={result}; kind={kind}; {}; {}",
                    outcomes.join("; "),
                    field(row, "reason")?
                );
                if !satisfied {
                    section.gap(detail);
                } else {
                    section.rows.push(
                        detail
                            + if result == "head_passed" {
                                "; HEAD-only success does not establish red/green discrimination"
                            } else {
                                ""
                            },
                    );
                }
            }
            Ok(())
        },
    ));
    sections.push(section(
        root,
        "Portfolio task disposition",
        "review/proof_portfolio.json",
        |portfolio, section| {
            if portfolio["schema"] != "ub-review.proof_portfolio.v1" {
                return Err("unknown portfolio schema".into());
            }
            section.rows.push(format!(
                "Packet subject head={}; phase={}; not current-head qualification",
                field(portfolio, "head")?,
                field(portfolio, "phase")?
            ));
            let tasks = array(&portfolio["candidate_tasks"])?;
            let decisions = array(&portfolio["decisions"])?;
            if portfolio["candidate_count"].as_u64() != Some(tasks.len() as u64) {
                return Err("candidate count mismatch".into());
            }
            let mut ids = BTreeSet::new();
            for task in tasks {
                let id = field(task, "id")?;
                let kind = field(task, "kind")?;
                if !ids.insert(id) {
                    return Err("duplicate task ID".into());
                }
                let matches: Vec<_> = decisions
                    .iter()
                    .filter(|d| d["task_id"].as_str() == Some(id))
                    .collect();
                if matches.len() != 1 {
                    section.gap(format!("{id}: missing or ambiguous portfolio decision"));
                    continue;
                }
                let decision = matches[0];
                if field(decision, "kind")? != kind {
                    return Err("decision and task kinds differ".into());
                }
                let status = field(decision, "status")?;
                let linked = array(&decision["receipt_ids"])?
                    .iter()
                    .map(|v| v.as_str().ok_or_else(|| "invalid receipt ID".to_owned()))
                    .collect::<Result<BTreeSet<_>, _>>()?;
                let mut recorded = linked.clone();
                recorded.insert(id);
                recorded.retain(|id| receipts.contains_key(*id));
                let detail = format!(
                    "{id}: {status}; kind={kind}; {}; matching recorded receipts={}",
                    field(decision, "reason")?,
                    recorded.len()
                );
                if !matches!(
                    status,
                    "selected_with_lease" | "answered_by_existing_receipt"
                ) || recorded.is_empty()
                    || recorded.iter().any(|id| {
                        receipts
                            .get(*id)
                            .is_none_or(|(satisfied, receipt_kind, result)| {
                                !satisfied
                                    || receipt_kind != kind
                                    || !matches!(
                                        kind,
                                        "focused-head" | "focused-build" | "focused-red-green"
                                    )
                                    || (kind == "focused-red-green" && result != "discriminating")
                            })
                    })
                    || linked.iter().any(|id| !receipts.contains_key(*id))
                {
                    section.gap(detail);
                } else {
                    section.rows.push(detail);
                }
            }
            if decisions
                .iter()
                .any(|d| d["task_id"].as_str().is_none_or(|id| !ids.contains(id)))
            {
                return Err("decision outside task catalog".into());
            }
            Ok(())
        },
    ));
    let missing: usize = sections.iter().map(|(_, count)| count).sum();
    let mut report = format!(
        "## UB Review advisory evidence\n\n**{}**\n\n",
        if missing > 0 {
            "INCOMPLETE: missing evidence is recorded below."
        } else {
            "No missing evidence detected in the supported receipt dimensions."
        }
    );
    report.push_str("A green advisory wrapper is not a complete analysis or proof pass. These are execution and receipt limitations, not source findings. This report does not execute proof, validate test adequacy, or change required checks. Results describe this packet only; inspect revision admission and the original receipts before using them as candidate evidence.\n\n");
    for (section, _) in sections {
        report.push_str(&section);
        report.push('\n');
    }
    report
}

fn run(root: &Path, summary: Option<&Path>) -> Result<String, String> {
    let report = render(root);
    fs::create_dir_all(root).map_err(|e| format!("create packet directory: {e}"))?;
    let output = root.join("consumer-evidence.md");
    if fs::symlink_metadata(&output).is_ok_and(|m| !m.is_file() || m.file_type().is_symlink()) {
        return Err("report output is not a regular file".into());
    }
    fs::write(output, &report).map_err(|e| format!("write consumer report: {e}"))?;
    if let Some(summary) = summary {
        use std::io::Write;
        fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(summary)
            .and_then(|mut f| f.write_all(report.as_bytes()))
            .map_err(|e| format!("append job summary: {e}"))?;
    }
    Ok(report)
}

fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let summary = std::env::var_os("GITHUB_STEP_SUMMARY");
    let result = if args.len() == 1 {
        run(Path::new(&args[0]), summary.as_deref().map(Path::new))
    } else {
        Err("usage: ub_review_evidence_report <packet-directory>".into())
    };
    match result {
        Ok(report) => print!("{report}"),
        Err(error) => {
            let diagnostic = format!(
                "## UB Review advisory evidence\n\nMISSING EVIDENCE: consumer reporter failed ({}). No report pass is established.\n",
                text(&error)
            );
            eprintln!("{diagnostic}");
            if let Some(summary) = summary {
                use std::io::Write;
                if let Err(error) = fs::OpenOptions::new()
                    .append(true)
                    .create(true)
                    .open(summary)
                    .and_then(|mut f| f.write_all(diagnostic.as_bytes()))
                {
                    eprintln!("append missing-evidence diagnostic failed: {error}");
                }
            }
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);

    struct Packet(std::path::PathBuf);
    impl Drop for Packet {
        fn drop(&mut self) {
            if let Err(error) = fs::remove_dir_all(&self.0) {
                eprintln!("fixture cleanup: {error}");
            }
        }
    }
    impl Packet {
        fn new() -> Result<Self, String> {
            let root = std::env::temp_dir().join(format!(
                "ub-evidence-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(std::env::temp_dir()).map_err(|e| e.to_string())?;
            fs::create_dir(&root).map_err(|e| e.to_string())?;
            let p = Self(root);
            p.write("plan.json", json!({"sensors":[]}))?;
            for name in ["observations", "proof_intents", "proof_receipts"] {
                p.write(&format!("review/{name}.json"), json!([]))?;
            }
            p.write(
                "review/proof_portfolio.json",
                portfolio(&[], "deferred_by_safe_wind_down", &[]),
            )?;
            Ok(p)
        }
        fn write(&self, relative: &str, value: Value) -> Result<(), String> {
            let path = self.0.join(relative);
            let parent = path
                .parent()
                .ok_or_else(|| "fixture parent missing".to_owned())?;
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            fs::write(path, serde_json::to_vec(&value).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())
        }
    }

    fn receipt(id: &str, result: &str) -> Value {
        json!({"schema":"ub-review.proof_receipt.v1", "id":id, "kind":"focused-head", "base":"origin/main", "head":"HEAD", "test_patch_mode":"head-only", "requested_by":["impact-planner"], "request_ids":[],
            "commands":[{"side":"head", "command":"cargo test --locked --package ripr --test agent_command_journey", "status":"passed", "exit_code":0, "timed_out":false, "timeout_sec":300, "duration_ms":1200, "stdout":"proof/journey/head/stdout.txt", "stderr":"proof/journey/head/stderr.txt", "reason":"completed"}],
            "result":result, "reason":"HEAD proof passed: completed"})
    }

    fn portfolio(ids: &[&str], status: &str, receipts: &[&str]) -> Value {
        json!({"schema":"ub-review.proof_portfolio.v1", "head":"HEAD", "phase":"broker-final", "candidate_count":ids.len(), "selected_task_ids":[],
            "candidate_tasks":ids.iter().map(|id| json!({"id":id,"kind":"focused-red-green","required":true,"estimated_cost_sec":300,"request_ids":[]})).collect::<Vec<_>>(),
            "decisions":ids.iter().map(|id| json!({"task_id":id,"kind":"focused-red-green","required":true,"status":status,"reason":"hard deadline has expired; proof was safely wound down","receipt_ids":receipts,"request_ids":[],"estimated_cost_sec":300})).collect::<Vec<_>>()})
    }

    // Reconstructed from the pinned producer schemas and #7893's documented
    // receipt facts, not an archive copy or a replay of the model run.
    #[test]
    fn refused_analysis_rejected_requests_and_three_deferred_tasks_survive_unrelated_green_proof()
    -> Result<(), String> {
        let p = Packet::new()?;
        p.write(
            "plan.json",
            json!({"sensors":[{"id":"ripr","run":true,"reason":"planned"}]}),
        )?;
        p.write("sensors/ripr/ub-review-sensor-status.json", json!({"sensor":"ripr","status":"failed","exit_code":2,"timed_out":false,"duration_ms":40,"reason":"installed ripr 0.10.0 refused 1192 Rust files over 800 cap"}))?;
        p.write("review/observations.json", json!([{"schema":"ub-review.observation.v1","id":"proof-intent-validation-tests-oracle-1","source":"proof-intent-validation","kind":"missing-evidence","evidence":["target is not a safe repository symbol, test, package, or route label"]}]))?;
        p.write("review/proof_intents.json", json!([
            {"id":"target","status":"unsupported_target","resolution_reason":"unsupported target syntax"},
            {"id":"kind","status":"unsupported_kind","resolution_reason":"unsupported proof kind"},
            {"id":"budget","status":"invalid_timeout","resolution_reason":"requested timeout 600s is outside approved per-command budget of 300s"}]))?;
        p.write(
            "review/proof_receipts.json",
            json!([receipt("agent-command-journey", "head_passed")]),
        )?;
        p.write(
            "review/proof_portfolio.json",
            portfolio(
                &["snippet-test-gap", "snippet-owner-gap", "snippet-boundary"],
                "deferred_by_safe_wind_down",
                &[],
            ),
        )?;
        let report = render(&p.0);
        assert!(report.contains("INCOMPLETE"));
        assert!(report.contains("MISSING EVIDENCE: ripr: status=failed, exit=2"));
        assert!(report.contains("1192 Rust files over 800 cap"));
        assert!(report.contains("rejected before planning"));
        for status in ["unsupported_target", "unsupported_kind", "invalid_timeout"] {
            assert!(report.contains(&format!("status={}", text(status))));
        }
        assert!(report.contains("600s is outside approved per-command budget of 300s"));
        for id in ["snippet-test-gap", "snippet-owner-gap", "snippet-boundary"] {
            assert!(report.contains(&format!(
                "MISSING EVIDENCE: {id}: deferred\\_by\\_safe\\_wind\\_down"
            )));
        }
        assert_eq!(report.matches("matching recorded receipts=0").count(), 3);
        assert!(report.contains("agent-command-journey: producer result=head\\_passed"));
        assert!(report.contains("green advisory wrapper is not a complete analysis or proof pass"));
        assert!(report.contains("not source findings"));
        Ok(())
    }

    #[test]
    fn unplanned_and_empty_packet_is_not_executed_proof() -> Result<(), String> {
        let p = Packet::new()?;
        p.write("plan.json", json!({"sensors":[{"id":"artifact-verifier","run":false,"reason":"except-tools: artifact-verifier"}]}))?;
        let report = render(&p.0);
        assert!(!report.contains("MISSING EVIDENCE:"));
        assert!(report.contains("not planned"));
        assert!(report.contains("No entries recorded. This is not executed proof"));
        Ok(())
    }

    #[test]
    fn selected_and_answered_tasks_need_canonical_receipts() -> Result<(), String> {
        let p = Packet::new()?;
        for status in ["selected_with_lease", "answered_by_existing_receipt"] {
            p.write(
                "review/proof_portfolio.json",
                portfolio(&["snippet"], status, &["nonexistent"]),
            )?;
            assert!(render(&p.0).contains("MISSING EVIDENCE: snippet:"));
        }
        Ok(())
    }

    #[test]
    fn matching_head_result_does_not_grant_red_green() -> Result<(), String> {
        let p = Packet::new()?;
        p.write(
            "review/proof_receipts.json",
            json!([receipt("snippet", "head_passed")]),
        )?;
        let mut head_task = portfolio(&["snippet"], "answered_by_existing_receipt", &["snippet"]);
        head_task["candidate_tasks"][0]["kind"] = json!("focused-head");
        head_task["decisions"][0]["kind"] = json!("focused-head");
        p.write("review/proof_portfolio.json", head_task)?;
        let report = render(&p.0);
        assert!(!report.contains("MISSING EVIDENCE:"));
        assert!(report.contains("matching recorded receipts=1"));
        assert!(report.contains("HEAD-only success does not establish red/green discrimination"));
        Ok(())
    }

    #[test]
    fn head_only_or_wrong_kind_receipt_cannot_answer_a_red_green_task() -> Result<(), String> {
        let p = Packet::new()?;
        p.write(
            "review/proof_receipts.json",
            json!([receipt("snippet", "head_passed")]),
        )?;
        p.write(
            "review/proof_portfolio.json",
            portfolio(&["snippet"], "answered_by_existing_receipt", &["snippet"]),
        )?;
        assert!(render(&p.0).contains("MISSING EVIDENCE: snippet:"));
        Ok(())
    }

    #[test]
    fn non_discriminating_refused_timeout_and_unknown_results_do_not_satisfy_tasks()
    -> Result<(), String> {
        let p = Packet::new()?;
        for result in [
            "non_discriminating",
            "skipped_budget",
            "skipped_profile",
            "skipped_unresolved",
            "timed_out",
            "future_result",
        ] {
            let mut row = receipt("snippet", result);
            if result.starts_with("skipped") {
                row["commands"] = json!([]);
            }
            if result == "timed_out" {
                row["commands"][0]["timed_out"] = json!(true);
            }
            p.write("review/proof_receipts.json", json!([row]))?;
            p.write(
                "review/proof_portfolio.json",
                portfolio(&["snippet"], "answered_by_existing_receipt", &["snippet"]),
            )?;
            assert!(render(&p.0).contains("MISSING EVIDENCE: snippet:"));
        }
        Ok(())
    }

    #[test]
    fn discriminating_receipt_needs_consistent_head_and_base_outcomes() -> Result<(), String> {
        let p = Packet::new()?;
        let mut row = receipt("snippet", "discriminating");
        row["kind"] = json!("focused-red-green");
        let mut base = row["commands"][0].clone();
        base["side"] = json!("base-plus-tests");
        base["status"] = json!("failed");
        base["exit_code"] = json!(101);
        row["commands"] = json!([row["commands"][0], base]);
        p.write("review/proof_receipts.json", json!([row]))?;
        p.write(
            "review/proof_portfolio.json",
            portfolio(&["snippet"], "answered_by_existing_receipt", &["snippet"]),
        )?;
        assert!(!render(&p.0).contains("MISSING EVIDENCE:"));
        row["commands"][1]["status"] = json!("passed");
        row["commands"][1]["exit_code"] = json!(0);
        p.write("review/proof_receipts.json", json!([row]))?;
        assert!(render(&p.0).contains("MISSING EVIDENCE: snippet:"));
        Ok(())
    }

    #[test]
    fn sensor_identity_and_native_completion_must_agree() -> Result<(), String> {
        let p = Packet::new()?;
        p.write(
            "plan.json",
            json!({"sensors":[{"id":"ripr","run":true,"reason":"planned"}]}),
        )?;
        let row = json!({"sensor":"ripr","status":"ok","exit_code":0,"timed_out":false,"reason":"completed"});
        p.write("sensors/ripr/ub-review-sensor-status.json", row.clone())?;
        assert!(!render(&p.0).contains("MISSING EVIDENCE:"));
        for (key, value) in [
            ("sensor", json!("other")),
            ("exit_code", json!(2)),
            ("timed_out", json!(true)),
            ("status", json!("future")),
        ] {
            let mut changed = row.clone();
            changed[key] = value;
            p.write("sensors/ripr/ub-review-sensor-status.json", changed)?;
            assert!(render(&p.0).contains("MISSING EVIDENCE: ripr:"));
        }
        Ok(())
    }

    #[test]
    fn missing_malformed_and_unknown_artifacts_cannot_look_clean() -> Result<(), String> {
        let p = Packet::new()?;
        fs::remove_file(p.0.join("review/proof_intents.json")).map_err(|e| e.to_string())?;
        fs::write(p.0.join("review/proof_receipts.json"), "{").map_err(|e| e.to_string())?;
        p.write("review/proof_portfolio.json", json!({"schema":"future"}))?;
        let report = render(&p.0);
        assert!(report.contains("INCOMPLETE"));
        for name in ["proof_intents", "proof_receipts", "proof_portfolio"] {
            assert!(report.contains(&format!("MISSING EVIDENCE: review/{}", text(name))));
        }
        Ok(())
    }

    #[test]
    fn catalog_count_decisions_duplicates_and_unsafe_sensor_ids_stay_missing() -> Result<(), String>
    {
        let p = Packet::new()?;
        let base = portfolio(&["snippet"], "deferred_by_safe_wind_down", &[]);
        let mut count = base.clone();
        count["candidate_count"] = json!(9);
        let mut no_decision = base.clone();
        no_decision["decisions"] = json!([]);
        let mut duplicate = base.clone();
        duplicate["decisions"] = json!([base["decisions"][0], base["decisions"][0]]);
        for changed in [count, no_decision, duplicate] {
            p.write("review/proof_portfolio.json", changed)?;
            assert!(render(&p.0).contains("MISSING EVIDENCE:"));
        }
        p.write(
            "plan.json",
            json!({"sensors":[{"id":"../outside","run":true,"reason":"planned"}]}),
        )?;
        assert!(render(&p.0).contains("invalid or duplicate sensor ID"));
        Ok(())
    }

    #[test]
    fn display_and_read_bounds_are_explicit_and_prose_is_escaped() -> Result<(), String> {
        let p = Packet::new()?;
        p.write("review/proof_intents.json",json!((0..205).map(|id| json!({"id":format!("intent-{id}"),"status":"unsupported_target","resolution_reason":"<script>\n[forged](https://invalid) | text"})).collect::<Vec<_>>()))?;
        let report = render(&p.0);
        assert!(report.contains("5 additional rows withheld"));
        assert!(report.contains("&lt;script&gt;"));
        assert!(!report.contains("<script>"));
        let file =
            fs::File::create(p.0.join("review/proof_receipts.json")).map_err(|e| e.to_string())?;
        file.set_len(MAX_BYTES + 1).map_err(|e| e.to_string())?;
        assert!(render(&p.0).contains("not a bounded regular file"));
        Ok(())
    }

    #[test]
    fn reporter_writes_identical_packet_and_summary_without_changing_receipts() -> Result<(), String>
    {
        let p = Packet::new()?;
        let source = p.0.join("review/proof_portfolio.json");
        let before = fs::read(&source).map_err(|e| e.to_string())?;
        let summary = p.0.join("summary.md");
        let report = run(&p.0, Some(&summary))?;
        for output in [p.0.join("consumer-evidence.md"), summary] {
            assert_eq!(
                fs::read_to_string(output).map_err(|e| e.to_string())?,
                report
            );
        }
        assert_eq!(fs::read(source).map_err(|e| e.to_string())?, before);
        Ok(())
    }

    #[test]
    fn absent_packet_gets_five_missing_dimensions_and_green_gate_metadata_cannot_clear_tasks()
    -> Result<(), String> {
        let p = Packet::new()?;
        let report = run(&p.0.join("absent"), None)?;
        assert_eq!(report.matches("MISSING EVIDENCE:").count(), 5);
        p.write(
            "review/gate_outcome.json",
            json!({"gate_result":"pass","fail_on_gate":false}),
        )?;
        p.write(
            "review/proof_portfolio.json",
            portfolio(&["snippet"], "deferred_by_safe_wind_down", &[]),
        )?;
        assert!(run(&p.0, None)?.contains("MISSING EVIDENCE: snippet:"));
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn symlink_receipt_components_are_refused_without_reading_outside_file() -> Result<(), String> {
        let p = Packet::new()?;
        p.write(
            "plan.json",
            json!({"sensors":[{"id":"ripr","run":true,"reason":"planned"}]}),
        )?;
        fs::create_dir(p.0.join("outside")).map_err(|e| e.to_string())?;
        fs::write(
            p.0.join("outside/ub-review-sensor-status.json"),
            "PRIVATE_MARKER",
        )
        .map_err(|e| e.to_string())?;
        fs::create_dir(p.0.join("sensors")).map_err(|e| e.to_string())?;
        std::os::unix::fs::symlink(p.0.join("outside"), p.0.join("sensors/ripr"))
            .map_err(|e| e.to_string())?;
        let report = render(&p.0);
        assert!(report.contains("symlink refused"));
        assert!(!report.contains("PRIVATE_MARKER"));
        Ok(())
    }
}
