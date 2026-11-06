//! Literal Rust continuation preparation. Project execution, static movement
//! and advisory receipt provenance remain separate observations.
use super::super::CommandResult;
use super::{CandidateExecution, archive::InstalledCandidate, rust_pre_edit as prefix};
use serde_json::{Value, json};
use std::{fs, path::Path};

pub(crate) fn observe(candidate: &InstalledCandidate, parent: &Path) -> Result<Value, String> {
    let prepared = prefix::observe(candidate, parent)?;
    let root = Path::new(
        prepared["fixture_root"]
            .as_str()
            .ok_or("Rust fixture root absent")?,
    );
    let foreign = Path::new(
        prepared["public_route"]["foreign_cwd"]
            .as_str()
            .ok_or("Rust foreign root absent")?,
    );
    let before_decoy = prefix::inventory(foreign)?;
    let execution = CandidateExecution::Qualified(candidate);
    let result = follow(
        root,
        parent,
        &prepared["public_route"],
        &candidate.rust_journey_manifest()?,
        Behavior::positive(),
        |args| execution.run(args, foreign, "qualified Rust continuation"),
        |args| execution.rust_project_cargo(root, args),
    );
    let result = result.map_err(|e| {
        format!(
            "{e}; Rust repair intended evidence destination: {}",
            parent.display()
        )
    })?;
    if prefix::inventory(foreign)? != before_decoy {
        return Err("Rust continuation changed foreign decoy inventory".into());
    }
    candidate.revalidate()?;
    if execution.fixture_head(root)? != "18a45fa5cd718f85dc60fcb27495b24dbd9715e7" {
        return Err("Rust fixture HEAD moved during literal preparation".into());
    }
    fs::write(
        parent.join("repair-observation.json"),
        serde_json::to_vec_pretty(&result).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    if result["preparation_ready"] != true {
        return Err(format!(
            "Rust literal repair preparation blocked; observations retained at {}",
            parent.display()
        ));
    }
    Ok(
        json!({"schema_version":"installed_rust_repair_preparation.v1","scope":"preparation_only",
        "pre_edit":prepared,"repair":result,"decoy_final_inventory_unchanged":true,
        "installed_acceptance":"not_established","blind_acceptance":"not_established"}),
    )
}

// Rust-specific adapter over the existing bounded process owner. The opaque
// installed handle is revalidated around project execution; Legacy cannot run.
impl CandidateExecution<'_> {
    pub(crate) fn rust_project_cargo(
        self,
        root: &Path,
        args: &[String],
    ) -> Result<CommandResult, String> {
        let Self::Qualified(candidate) = self else {
            return Err("Rust project verification requires installed custody".into());
        };
        if args.len() != 2
            || args[0] != "test"
            || args[1].is_empty()
            || !args[1]
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_')
        {
            return Err("unsupported Rust project test command".into());
        }
        candidate.revalidate()?;
        let output = crate::run::capture_bytes_in_dir_with_timeout(
            Path::new("cargo"),
            args,
            root,
            &[
                ("CARGO_BUILD_JOBS", "1"),
                ("CARGO_INCREMENTAL", "0"),
                ("CARGO_NET_OFFLINE", "true"),
                ("CARGO_TERM_COLOR", "never"),
            ],
            &[
                "CARGO_TARGET_DIR",
                "GIT_DIR",
                "GIT_WORK_TREE",
                "GIT_INDEX_FILE",
            ],
            std::time::Duration::from_mins(2),
            "qualified Rust focused Cargo",
        )?;
        candidate.revalidate()?;
        if output.timed_out {
            return Err("qualified Rust focused Cargo exceeded owned deadline".into());
        }
        let status = output.status.ok_or("Rust focused Cargo status missing")?;
        Ok(CommandResult {
            status: status.code(),
            success: status.success(),
            stdout: String::from_utf8(output.stdout).map_err(|e| e.to_string())?,
            stderr: String::from_utf8(output.stderr).map_err(|e| e.to_string())?,
        })
    }
}

struct Behavior {
    use_public_name: bool,
    execute: bool,
    expected_value: &'static str,
    continue_failed_control: bool,
}
impl Behavior {
    fn positive() -> Self {
        Self {
            use_public_name: true,
            execute: true,
            expected_value: "gold",
            continue_failed_control: false,
        }
    }
}

fn follow(
    root: &Path,
    evidence: &Path,
    prepared: &Value,
    manifest: &[u8],
    behavior: Behavior,
    mut product: impl FnMut(&[String]) -> Result<CommandResult, String>,
    mut cargo: impl FnMut(&[String]) -> Result<CommandResult, String>,
) -> Result<Value, String> {
    let manifest: Value = serde_json::from_slice(manifest).map_err(|e| e.to_string())?;
    let independent = &manifest["journey"]["selected_repair"];
    if independent["edit_target"] != "tests/tier_boundary.rs"
        || manifest["journey"]["expected_edit_cage"] != json!(["tests/tier_boundary.rs"])
        || manifest["journey"]["discriminator_family"] != "boundary-constant"
    {
        return Err("unsupported independent Rust repair contract".into());
    }
    let packet = prefix::json_file(root, &root.join("target/ripr/workflow/agent-packet.json"))?;
    let row = &packet["packets"][0];
    let public_name = row["recommended_test"]["name"]
        .as_str()
        .ok_or("Rust public test name absent")?;
    if row["allowed_edit_surface"] != json!(["tests/tier_boundary.rs"])
        || row["recommended_test"]["file"] != "tests/tier_boundary.rs"
        || row["seam_id"] != prepared["seam_id"]
        || row["changed_expression"] != "quantity >= 20"
        || public_name != "tier_boundary_discriminator"
    {
        return Err("Rust public repair differs from supported independent equality repair".into());
    }
    let declared_name = independent["added_test"]
        .as_str()
        .ok_or("Rust fixture test identifier absent")?;
    let edit_name = if behavior.use_public_name {
        public_name
    } else {
        declared_name
    };
    if !edit_name
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        return Err("unsupported Rust test identifier".into());
    }
    let hint = row["suggested_test_command"]
        .as_str()
        .ok_or("Rust focused command absent")?;
    let words = prefix::printed_words::printed_words(hint)?;
    if words != ["cargo", "test", public_name] {
        return Err("unsupported Rust product-emitted focused command".into());
    }
    let target = root.join("tests/tier_boundary.rs");
    let original = prefix::plain_bytes(root, &target)?;
    if manifest["repository"]["snapshots"]["head"]["files"]["tests/tier_boundary.rs"]
        != prefix::digest(&original)
    {
        return Err(
            "Rust selected test differs from admitted fixture before the bounded edit".into(),
        );
    }
    let mut edited = original.clone();
    edited.extend_from_slice(
        format!(
            "\n#[test]\nfn {edit_name}() {{\n    assert_eq!(tier(20), \"{}\");\n}}\n",
            behavior.expected_value
        )
        .as_bytes(),
    );
    fs::write(&target, &edited).map_err(|e| e.to_string())?;
    let edit = json!({"path":"tests/tier_boundary.rs","before_sha256":prefix::digest(&original),
        "after_sha256":prefix::digest(&edited),"public_prescribed_test_name":public_name,
        "fixture_declared_test_identifier":declared_name,"actual_test_name":edit_name,
        "independent_assertion":"assert_eq!(tier(20), \"gold\")",
        "control_expected_value":behavior.expected_value,"command_rewritten":false});
    let runtime = if behavior.execute {
        let args = words[1..].to_vec();
        let observed = cargo(&args)?;
        let counts = runtime_counts(&observed.stdout, edit_name);
        let raw = json!({"program":"cargo","argv":args,"working_directory":root,
            "command_origin":"producer packet suggested_test_command","printed_command":hint,
            "operator_intervention":"harness binds the project test hint to the selected project cwd; no command rewriting",
            "exit":observed.status,"stdout":observed.stdout,"stderr":observed.stderr,"counts":counts});
        fs::write(
            evidence.join("focused-cargo.json"),
            serde_json::to_vec_pretty(&raw).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        raw
    } else {
        json!({"status":"not_run","counts":{"selected":0,"executed":0,"passed":0,"failed":0}})
    };
    let runtime_ready = runtime["exit"] == 0
        && runtime["counts"]["selected"] == 1
        && runtime["counts"]["executed"] == 1
        && runtime["counts"]["passed"] == 1
        && runtime["counts"]["failed"] == 0
        && runtime["counts"]["named_subjects"] == 1
        && runtime["counts"]["ignored"] == 0
        && runtime["counts"]["measured"] == 0
        && runtime["counts"]["malformed_summaries"] == 0;
    if !runtime_ready && !behavior.continue_failed_control {
        return Ok(
            json!({"preparation_ready":false,"edit":edit,"project_verification":runtime,
            "static_movement":"not_run","receipt":"not_run","authoritative_closure":false}),
        );
    }
    let literal = prepared["emitted_next"]
        .as_str()
        .ok_or("Rust printed continuation absent")?;
    let next = prefix::printed_words::printed_words(literal)?;
    let root_text = root.to_str().ok_or("non-UTF8 Rust root")?;
    if next.len() != 6 || next[..5] != ["ripr", "continue", "--root", root_text, "--attempt"] {
        return Err("misbound Rust literal continuation".into());
    }
    let continued = product(&next[1..])?;
    record(evidence, "continue", &next[1..], &continued)?;
    if !continued.success {
        return Err(format!(
            "Rust literal continuation refused: {}",
            continued.stderr
        ));
    }
    let receipt_path = continued
        .stdout
        .lines()
        .find_map(|line| line.strip_prefix("  receipt: "))
        .ok_or("Rust continuation did not emit receipt path")?;
    if Path::new(receipt_path) != root.join("target/ripr/reports/agent-receipt.json") {
        return Err("Rust emitted receipt path escaped its selected root".into());
    }
    let attempt_path = Path::new(
        prepared["attempt_manifest"]["path"]
            .as_str()
            .ok_or("Rust manifest path absent")?,
    );
    let attempt = prefix::json_file(root, attempt_path)?;
    if attempt["after"]["verdict"]["status"] != "compliant" || attempt["after"]["current"] != true {
        return Err("Rust after did not retain a current compliant cage verdict".into());
    }
    let changed = attempt["after"]["verdict"]["changed_paths"]
        .as_array()
        .ok_or("Rust after changed paths absent")?;
    if !changed.iter().any(|p| p == "tests/tier_boundary.rs")
        || changed.iter().any(|p| {
            p.as_str()
                .is_none_or(|p| p != "tests/tier_boundary.rs" && !p.starts_with("target/"))
        })
    {
        return Err("Rust after admitted an unexpected authored edit".into());
    }
    let workflow = prefix::json_file(root, &root.join("target/ripr/workflow/workflow.json"))?;
    let receipt_command = workflow["commands"]
        .as_array()
        .ok_or("Rust workflow commands absent")?
        .iter()
        .find(|row| row["step"] == "agent_receipt")
        .and_then(|row| row["command"].as_str())
        .ok_or("Rust workflow receipt producer command absent")?;
    let argv = receipt_argv(
        receipt_command,
        root_text,
        prepared["seam_id"].as_str().ok_or("Rust seam absent")?,
    )?;
    // Reconsume the product's artifacts through its advertised producer. The
    // harness never reconstructs canonical snapshot commitments or receipts.
    let produced = product(&argv)?;
    record(evidence, "receipt", &argv, &produced)?;
    if !produced.success {
        return Err(format!(
            "Rust advertised receipt refused: {}",
            produced.stderr
        ));
    }
    let receipt = prefix::json_file(root, Path::new(receipt_path))?;
    validate_receipt(&receipt, root, prepared)?;
    let mut fixture_digests = serde_json::Map::new();
    for path in ["Cargo.toml", "src/lib.rs", ".gitignore"] {
        let observed = prefix::digest(&prefix::plain_bytes(root, &root.join(path))?);
        if manifest["repository"]["snapshots"]["head"]["files"][path] != observed {
            return Err(format!(
                "Rust fixture input changed during literal route: {path}"
            ));
        }
        fixture_digests.insert(path.into(), json!(observed));
    }
    if prefix::plain_bytes(root, &target)? != edited {
        return Err("Rust selected test changed after focused execution".into());
    }
    let artifacts = [
        "target/ripr/workflow/after.repo-exposure.json",
        "target/ripr/workflow/agent-verify.json",
        "target/ripr/workflow/analysis-outcome.json",
        "target/ripr/reports/agent-receipt.json",
    ]
    .iter()
    .map(|path| prefix::artifact(root, &root.join(path)))
    .collect::<Result<Vec<_>, _>>()?;
    Ok(
        json!({"preparation_ready":runtime_ready,"edit":edit,"project_verification":runtime,
        "emitted_continue":literal,"emitted_receipt_command":receipt_command,
        "receipt_argv":argv,"receipt_path":receipt_path,"producer_artifacts":artifacts,
        "fixture_source_digests":fixture_digests,
        "typed_command_specs":row["evidence_record"]["canonical_item"]["command_specs"],
        "executed_product_command_form":"literal public workflow command",
        "fixture_declared_focused_command":independent["focused_verification_command"],
        "changed_paths":changed,"edit_cage":"compliant","static_movement":receipt["seam"]["change"],
        "receipt_status":receipt["status"],"product_verification":receipt["verification"],
        "authoritative_closure":false,"installed_acceptance":"not_established","blind_acceptance":"not_established"}),
    )
}

fn validate_receipt(receipt: &Value, root: &Path, prepared: &Value) -> Result<(), String> {
    if receipt["status"] != "advisory"
        || receipt["seam"]["change"] != "improved"
        || receipt["seam"]["seam_id"] != prepared["seam_id"]
        || receipt["provenance"]["repo_root"] != root.to_str().ok_or("non-UTF8 receipt root")?
        || receipt["repair_attempt"]["current"] != true
        || receipt["repair_attempt"]["edit_cage_verdict"]["status"] != "compliant"
        || receipt["verification"]["status"] != "verification_not_run"
        || receipt["verification"]["commands_run"] != json!([])
    {
        return Err(
            "Rust receipt has unsupported root/subject/cage/static/verification axes".into(),
        );
    }
    Ok(())
}

fn receipt_argv(command: &str, root: &str, seam: &str) -> Result<Vec<String>, String> {
    let words = prefix::printed_words::printed_words(command)?;
    let verify = Path::new(root).join("target/ripr/workflow/agent-verify.json");
    if words
        != [
            "ripr",
            "agent",
            "receipt",
            "--root",
            root,
            "--verify-json",
            verify.to_str().ok_or("non-UTF8 verify path")?,
            "--seam-id",
            seam,
            "--json",
            "--out",
            "target/ripr/reports/agent-receipt.json",
        ]
    {
        return Err("unsupported or misbound Rust receipt producer command".into());
    }
    Ok(words[1..].to_vec())
}

fn runtime_counts(stdout: &str, name: &str) -> Value {
    let mut passed = 0;
    let mut failed = 0;
    let mut ignored = 0;
    let mut measured = 0;
    let mut summaries = 0;
    let mut malformed = 0;
    for line in stdout.lines() {
        if let Some(counts) = crate::windows_advisory::completion_counts(line) {
            summaries += 1;
            passed += counts.passed;
            failed += counts.failed;
            ignored += counts.ignored;
            measured += counts.measured;
        } else if line.starts_with("test result:") {
            malformed += 1;
        }
    }
    let exact_pass = format!("test {name} ... ok");
    let exact_fail = format!("test {name} ... FAILED");
    let named = stdout
        .lines()
        .filter(|line| *line == exact_pass || *line == exact_fail)
        .count();
    json!({"selected":passed+failed+ignored+measured,"executed":passed+failed,"passed":passed,"failed":failed,
        "ignored":ignored,"measured":measured,"named_subjects":named,"completion_summaries":summaries,"malformed_summaries":malformed})
}
fn record(root: &Path, label: &str, args: &[String], result: &CommandResult) -> Result<(), String> {
    fs::write(
        root.join(format!("repair-{label}.json")),
        serde_json::to_vec_pretty(&json!({
        "argv":args,"exit":result.status,"stdout":result.stdout,"stderr":result.stderr}))
        .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

#[cfg(test)]
#[path = "rust_repair/tests.rs"]
mod tests;
