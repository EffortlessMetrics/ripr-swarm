use super::*;
use serde_json::{Value, json};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn expected() -> Expected {
    Expected {
        subject: Subject {
            repository: "EffortlessMetrics/ripr-swarm".to_owned(),
            head_sha: "a".repeat(40),
            base_sha: "b".repeat(40),
            group_sha: "c".repeat(40),
            tree_sha: "d".repeat(40),
            workflow_generation: "e".repeat(40),
            run_id: "123".to_owned(),
            run_attempt: "2".to_owned(),
            request_id: "123-2".to_owned(),
        },
        qualification_started_at_ms: 1_000,
        deadline_at_ms: 1_201_000,
    }
}

fn subject() -> Value {
    let s = expected().subject;
    json!({"repository":s.repository,"head_sha":s.head_sha,"base_sha":s.base_sha,
        "group_sha":s.group_sha,"tree_sha":s.tree_sha,"workflow_generation":s.workflow_generation,
        "run_id":s.run_id,"run_attempt":s.run_attempt,"request_id":s.request_id})
}

struct Fixture {
    os: String,
    receipt: Value,
    inventory: Vec<u8>,
    observation: Value,
}

impl Fixture {
    fn new(index: usize) -> Self {
        let (os, runner, runtime, required) = PLATFORMS[index];
        // Matches the producer's independently retained 15-case fixture exactly.
        let inventory = (0..15)
            .map(|id| {
                json!({"case_id":id,"source":"fixture","origin":"Foreign",
            "bash":"ripr check","powershell":null,"bash_only":id==3,"powershell_unavailable":id==4})
                .to_string()
            })
            .collect::<Vec<_>>()
            .join("\n")
            .into_bytes();
        let shells = required.iter().map(|shell| {
            let ps = matches!(*shell, "pwsh" | "powershell");
            json!({"shell":shell,"executed_case_ids":(0..15).filter(|id| !ps || (*id!=3 && *id!=4)).collect::<Vec<_>>(),
                "excluded":if ps {vec![json!({"case_id":3,"reason":"bash_only"}),json!({"case_id":4,"reason":"powershell_unavailable"})]} else {vec![]},
                "known_gaps":if *shell=="bash" {vec![json!({"case_id":2,"reason":"fixture known gap"})]} else {vec![]}})
        }).collect::<Vec<_>>();
        let mut receipt = subject();
        if let Some(r) = receipt.as_object_mut() {
            r.extend(json!({"schema_id":"ripr.printed-command-terminal.v1","schema_version":1,
            "claim":"main_replay_test_completed_with_disclosed_known_gaps","job_key":"paste","matrix_os":os,
            "runner_os":runner,"runtime_os":runtime,"inventory_sha256":digest(&inventory),"inventory_case_count":15,
            "required_shells":required,"shells":shells}).as_object().cloned().unwrap_or_default());
        }
        let observation = json!({"schema_id":"ripr.printed-command-observation.v1","schema_version":1,"subject":subject(),
            "workflow_path":WORKFLOW,"job_key":"paste","matrix_os":os,
            "job":{"id":index+10,"status":"completed","conclusion":"success","replay_conclusion":"success",
                "receipt_upload_conclusion":"success","started_at_ms":2000,"completed_at_ms":4000},
            "deadline":{"entry_conclusion":"success","completion_conclusion":"success","completed_at_ms":3500,"deadline_at_ms":1201000},
            "inventory_artifact":{"id":index*2+100,"name":format!("printed-commands-{os}"),"run_id":"123","run_attempt":"2","sha256":""},
            "receipt_artifact":{"id":index*2+101,"name":format!("printed-command-receipt-{os}-123-2"),"run_id":"123","run_attempt":"2","sha256":""}});
        Self {
            os: os.to_owned(),
            receipt,
            inventory,
            observation,
        }
    }
    fn encoded(&mut self) -> (Vec<u8>, Vec<u8>) {
        self.receipt["inventory_sha256"] = digest(&self.inventory).into();
        self.observation["inventory_artifact"]["sha256"] = digest(&self.inventory).into();
        let receipt = self.receipt.to_string().into_bytes();
        self.observation["receipt_artifact"]["sha256"] = digest(&receipt).into();
        (receipt, self.observation.to_string().into_bytes())
    }
}

fn fixtures() -> Vec<Fixture> {
    (0..3).map(Fixture::new).collect()
}

fn run(fixtures: &mut [Fixture]) -> Report {
    let encoded: Vec<_> = fixtures.iter_mut().map(Fixture::encoded).collect();
    let inputs: Vec<_> = fixtures
        .iter()
        .zip(&encoded)
        .map(|(f, (r, o))| CellInput {
            matrix_os: &f.os,
            receipt: r,
            inventory: &f.inventory,
            observation: o,
        })
        .collect();
    inspect(&expected(), &inputs)
}

fn refused(mut change: impl FnMut(&mut Fixture)) {
    let mut fs = fixtures();
    change(&mut fs[0]);
    let report = run(&mut fs);
    assert_eq!(report.status, Status::NotEstablished);
    assert!(!report.findings.is_empty(), "malformed fixture accepted");
}

#[test]
fn complete_matrix_retains_gaps_and_exclusions_without_authority() {
    let report = run(&mut fixtures());
    assert!(report.findings.is_empty(), "{:?}", report.findings);
    assert_eq!(report.status, Status::NotEstablished);
    assert_eq!(report.cells.len(), 3);
    for cell in report.cells {
        assert_eq!(cell.inventory_case_count, 15);
        assert_eq!(
            cell.replay_observation,
            ReplayObservation::CompletedWithDisclosedGaps
        );
        assert_eq!(
            cell.inventory_sha256,
            "f19682d76d330a2b2cabdd6c9daffe968756cbd63d10e60cc80d2f43f736f562"
        );
        let bash = cell.shells.iter().find(|s| s.shell == "bash");
        assert!(bash.is_some_and(|s| s.known_gaps
            == vec![CaseDisclosure {
                case_id: 2,
                reason: "fixture known gap".to_owned()
            }]));
        let pwsh = cell.shells.iter().find(|s| s.shell == "pwsh");
        assert!(pwsh.is_some_and(|s| s.excluded.len() == 2 && s.executed_case_ids.len() == 13));
    }
}

#[test]
fn reviewed_workflow_matrix_is_the_independent_required_shell_source() {
    let workflow = include_str!("../../../../.github/workflows/printed-command-paste.yml");
    let mut observed = Vec::new();
    let mut os = None;
    for line in workflow.lines() {
        if let Some(value) = line.trim().strip_prefix("- os: ") {
            os = Some(value);
        }
        if let Some(shells) = line.trim().strip_prefix("shells: ") {
            observed.push((os.take(), shells.split(',').collect::<Vec<_>>()));
        }
    }
    assert_eq!(
        observed,
        PLATFORMS
            .iter()
            .map(|p| (Some(p.0), p.3.to_vec()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn missing_duplicate_unknown_or_extra_os_cells_refuse() {
    let mut fs = fixtures();
    fs.pop();
    assert!(!run(&mut fs).findings.is_empty());
    let mut fs = fixtures();
    fs.push(Fixture::new(0));
    assert!(!run(&mut fs).findings.is_empty());
    refused(|f| f.os = "macOS-latest".to_owned());
    let mut fs = fixtures();
    fs[2] = Fixture::new(0);
    assert!(!run(&mut fs).findings.is_empty());
}

#[test]
fn receipt_subject_and_platform_fields_are_exact() {
    for key in [
        "repository",
        "head_sha",
        "base_sha",
        "group_sha",
        "tree_sha",
        "workflow_generation",
        "run_id",
        "run_attempt",
        "request_id",
        "job_key",
        "matrix_os",
        "runner_os",
        "runtime_os",
        "claim",
        "schema_id",
    ] {
        refused(|f| f.receipt[key] = "wrong".into());
    }
    for value in [json!(2), json!(true), json!(1.0)] {
        refused(|f| f.receipt["schema_version"] = value.clone());
    }
}

#[test]
fn external_observation_subject_cannot_be_replaced_by_receipt_context() {
    for key in [
        "repository",
        "head_sha",
        "base_sha",
        "group_sha",
        "tree_sha",
        "workflow_generation",
        "run_id",
        "run_attempt",
        "request_id",
    ] {
        refused(|f| f.observation["subject"][key] = "wrong".into());
    }
    for key in ["workflow_path", "matrix_os", "job_key", "schema_id"] {
        refused(|f| f.observation[key] = "wrong".into());
    }
}

#[test]
fn producer_required_shells_cannot_redefine_the_canonical_denominator() {
    refused(|f| {
        f.receipt["required_shells"] = json!(["bash"]);
        f.receipt["shells"] = json!([f.receipt["shells"][0].clone()]);
    });
    refused(|f| f.receipt["required_shells"] = json!(["bash", "sh", "zsh", "pwsh", "pwsh"]));
    refused(|f| {
        f.receipt["shells"].as_array_mut().map(Vec::pop);
    });
    refused(|f| {
        let row = f.receipt["shells"][0].clone();
        if let Some(v) = f.receipt["shells"].as_array_mut() {
            v.push(row);
        }
    });
    refused(|f| f.receipt["shells"][0]["shell"] = "unknown".into());
}

#[test]
fn missing_duplicate_reordered_noninteger_or_out_of_range_case_ids_refuse() {
    for ids in [
        json!([]),
        json!([0, 0]),
        json!([1, 0]),
        json!([0.0]),
        json!([true]),
        json!([100000]),
        json!([-1]),
    ] {
        refused(|f| f.receipt["shells"][0]["executed_case_ids"] = ids.clone());
    }
    refused(|f| {
        f.receipt["shells"][0]["executed_case_ids"]
            .as_array_mut()
            .map(Vec::pop);
    });
}

#[test]
fn exclusions_must_be_exact_and_known_gaps_must_remain_on_executed_cases() {
    refused(|f| f.receipt["shells"][3]["excluded"][0]["reason"] = "powershell_unavailable".into());
    refused(|f| f.receipt["shells"][3]["excluded"] = json!([]));
    refused(|f| {
        f.receipt["shells"][3]["known_gaps"] = json!([{"case_id":3,"reason":"excluded case"}])
    });
    refused(|f| {
        f.receipt["shells"][0]["known_gaps"] =
            json!([{"case_id":2,"reason":"x"},{"case_id":2,"reason":"x"}])
    });
    refused(|f| f.receipt["shells"][0]["known_gaps"][0]["reason"] = " \n".into());
    refused(|f| f.receipt["shells"][0]["known_gaps"][0]["case_id"] = json!(2.0));
}

#[test]
fn strict_inventory_identity_and_required_fields() -> TestResult {
    let original = String::from_utf8(Fixture::new(0).inventory)?;
    for (from, to) in [
        ("\"case_id\":0", "\"case_id\":1"),
        ("\"case_id\":0", "\"case_id\":0,\"case_id\":0"),
        ("\"case_id\":0", "\"case_id\":0.0"),
        ("\"case_id\":0", "\"case_id\":true"),
        ("\"powershell\":null,", ""),
        ("\"bash_only\":false", "\"bash_only\":0"),
        ("\"origin\":\"Foreign\"", "\"origin\":\"other\""),
        ("\"source\":\"fixture\"", "\"source\":\"\""),
        ("\"case_id\":0", "\"extra\":0,\"case_id\":0"),
    ] {
        let altered = original.replacen(from, to, 1);
        assert_ne!(altered, original);
        refused(|f| f.inventory = altered.clone().into_bytes());
    }
    refused(|f| f.inventory = b"\xff".to_vec());
    refused(|f| f.inventory = vec![]);
    refused(|f| f.receipt["inventory_case_count"] = json!(16));
    refused(|f| f.receipt["inventory_case_count"] = json!(14));
    Ok(())
}

#[test]
fn job_and_deadline_outcomes_must_be_explicit_success_observations() {
    for key in [
        "conclusion",
        "replay_conclusion",
        "receipt_upload_conclusion",
    ] {
        for status in ["failure", "cancelled", "skipped", "neutral", ""] {
            refused(|f| f.observation["job"][key] = status.into());
        }
    }
    refused(|f| f.observation["job"]["status"] = "in_progress".into());
    for key in ["entry_conclusion", "completion_conclusion"] {
        for status in ["failure", "cancelled", "skipped", ""] {
            refused(|f| f.observation["deadline"][key] = status.into());
        }
    }
}

#[test]
fn job_and_deadline_timing_cannot_reset_the_whole_route_budget() {
    refused(|f| f.observation["job"]["started_at_ms"] = json!(999));
    refused(|f| f.observation["job"]["completed_at_ms"] = json!(1_201_001));
    refused(|f| f.observation["deadline"]["deadline_at_ms"] = json!(1_202_000));
    refused(|f| f.observation["deadline"]["completed_at_ms"] = json!(1_999));
    refused(|f| f.observation["deadline"]["completed_at_ms"] = json!(4_001));
}

#[test]
fn same_run_artifact_labels_and_distinct_object_ids_are_required() {
    for artifact in ["inventory_artifact", "receipt_artifact"] {
        for key in ["run_id", "run_attempt", "name"] {
            refused(|f| f.observation[artifact][key] = "wrong".into());
        }
        refused(|f| f.observation[artifact]["id"] = json!(0));
        refused(|f| f.observation[artifact]["id"] = json!(true));
    }
    refused(|f| {
        f.observation["receipt_artifact"]["id"] = f.observation["inventory_artifact"]["id"].clone()
    });
    let mut fs = fixtures();
    fs[1].observation["job"]["id"] = fs[0].observation["job"]["id"].clone();
    assert!(!run(&mut fs).findings.is_empty());
    let mut fs = fixtures();
    fs[1].observation["inventory_artifact"]["id"] =
        fs[0].observation["inventory_artifact"]["id"].clone();
    assert!(!run(&mut fs).findings.is_empty());
}

fn raw_case(mut change: impl FnMut(&mut Vec<u8>, &mut Vec<u8>, &mut Vec<u8>)) {
    let mut fs = fixtures();
    let mut encoded: Vec<_> = fs.iter_mut().map(Fixture::encoded).collect();
    let (receipt, observation) = &mut encoded[0];
    change(receipt, &mut fs[0].inventory, observation);
    let inputs: Vec<_> = fs
        .iter()
        .zip(&encoded)
        .map(|(f, (r, o))| CellInput {
            matrix_os: &f.os,
            receipt: r,
            inventory: &f.inventory,
            observation: o,
        })
        .collect();
    assert!(!inspect(&expected(), &inputs).findings.is_empty());
}

#[test]
fn exact_inventory_and_receipt_bytes_must_match_separate_observations() {
    raw_case(|_, inventory, _| inventory.extend_from_slice(b"\n"));
    raw_case(|receipt, _, _| receipt.extend_from_slice(b" "));
    raw_case(|_, _, observation| {
        let text = String::from_utf8_lossy(observation);
        *observation = text.replacen("f19682d7", "019682d7", 1).into_bytes();
    });
}

#[test]
fn strict_receipt_and_observation_json_reject_ambiguity_and_missing_fields() {
    // Reach schema decoding before hash comparison. Identical duplicate values
    // and nested duplicates are still rejected, not normalized away.
    for raw in [
        b"{}".to_vec(),
        b"\xff".to_vec(),
        b"{\"x\":NaN}".to_vec(),
        b"{\"x\":1e999}".to_vec(),
    ] {
        raw_case(|receipt, _, _| *receipt = raw.clone());
        raw_case(|_, _, observation| *observation = raw.clone());
    }
    for field in ["schema_id", "shells", "required_shells"] {
        refused(|f| {
            f.receipt.as_object_mut().map(|r| r.remove(field));
        });
    }
    for field in ["deadline", "job", "inventory_artifact"] {
        refused(|f| {
            f.observation.as_object_mut().map(|r| r.remove(field));
        });
    }
    refused(|f| f.receipt["authenticated"] = json!(true));
    refused(|f| f.observation["job"]["authenticated"] = json!(true));
    raw_case(|r, _, _| {
        let text = String::from_utf8_lossy(r);
        *r = text
            .replacen(
                "\"schema_version\":1",
                "\"schema_version\":1,\"schema_version\":1",
                1,
            )
            .into_bytes();
    });
    raw_case(|_, _, o| {
        let text = String::from_utf8_lossy(o);
        *o = text
            .replacen(
                "\"status\":\"completed\"",
                "\"status\":\"completed\",\"status\":\"completed\"",
                1,
            )
            .into_bytes();
    });
}

#[test]
fn byte_and_count_bounds_are_fail_closed() {
    raw_case(|r, _, _| *r = vec![b' '; MAX_DOCUMENT + 1]);
    raw_case(|_, i, _| *i = vec![b' '; MAX_DOCUMENT + 1]);
    raw_case(|_, _, o| *o = vec![b' '; MAX_OBSERVATION + 1]);
    refused(|f| f.receipt["inventory_case_count"] = json!(MAX_CASES + 1));
}

#[test]
fn expected_subject_and_deadline_are_not_supplied_by_candidate_records() {
    let fs = fixtures();
    let mut bad = expected();
    bad.subject.repository = "other/repo".to_owned();
    assert!(!inspect(&bad, &[]).findings.is_empty());
    bad = expected();
    bad.deadline_at_ms += 1;
    assert!(!inspect(&bad, &[]).findings.is_empty());
    bad = expected();
    bad.qualification_started_at_ms = u64::MAX;
    assert!(!inspect(&bad, &[]).findings.is_empty());
    assert_eq!(fs.len(), 3);
}

#[test]
fn closed_decoders_reject_duplicate_fields_before_any_hash_comparison() {
    let mut f = Fixture::new(0);
    let (receipt, observation) = f.encoded();
    let raw = String::from_utf8_lossy(&receipt).replacen(
        "\"schema_version\":1",
        "\"schema_version\":1,\"schema_version\":1",
        1,
    );
    assert_ne!(raw.as_bytes(), receipt);
    assert!(decode::<Receipt>(raw.as_bytes(), MAX_DOCUMENT).is_err());
    let raw = String::from_utf8_lossy(&observation).replacen(
        "\"status\":\"completed\"",
        "\"status\":\"completed\",\"status\":\"completed\"",
        1,
    );
    assert_ne!(raw.as_bytes(), observation);
    assert!(decode::<Observation>(raw.as_bytes(), MAX_OBSERVATION).is_err());
}

#[test]
fn no_disclosed_gap_is_still_not_an_unconditional_pass() {
    let mut fs = fixtures();
    for f in &mut fs {
        f.receipt["shells"][0]["known_gaps"] = json!([]);
    }
    let r = run(&mut fs);
    assert!(r.findings.is_empty(), "{:?}", r.findings);
    assert_eq!(r.status, Status::NotEstablished);
    assert!(
        r.cells
            .iter()
            .all(|c| c.replay_observation == ReplayObservation::CompletedWithoutDisclosedGaps)
    );
}

#[test]
fn extra_required_shell_is_rejected_but_additional_completed_shell_is_disclosed() {
    refused(|f| f.receipt["required_shells"] = json!(["bash", "sh", "zsh", "pwsh", "powershell"]));
    let mut fs = fixtures();
    let mut extra = fs[0].receipt["shells"][3].clone();
    extra["shell"] = "powershell".into();
    if let Some(shells) = fs[0].receipt["shells"].as_array_mut() {
        shells.push(extra);
    }
    let r = run(&mut fs);
    assert!(r.findings.is_empty(), "{:?}", r.findings);
    assert_eq!(r.cells[0].shells.len(), 5);
    assert_eq!(r.status, Status::NotEstablished);
}
