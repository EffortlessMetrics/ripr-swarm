use super::{
    init_git_fixture_repo, printed_ripr_args, run_command, run_git, unique_temp_workspace,
};
use std::path::Path;
use std::process::Output;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn replay(command: &str, cwd: &Path) -> Result<Output, Box<dyn std::error::Error>> {
    let args = printed_ripr_args(command)?;
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    Ok(run_command(env!("CARGO_BIN_EXE_ripr"), Some(cwd), &args)?)
}

/// The missing-ledger refusal names real producers. Follow its Rust capture
/// and ledger commands from a foreign CWD, including a quoted root. The
/// fixture is an ordinary pricing crate with independent literal assertions.
#[test]
fn rerun_missing_gap_ledger_recovery_executes_the_rust_producer() -> TestResult {
    let scratch = unique_temp_workspace("rerun-ledger-recovery");
    let root = scratch.join("ordinary shop");
    let foreign = scratch.join("foreign");
    let result: TestResult = (|| {
        std::fs::create_dir_all(root.join("src"))?;
        std::fs::create_dir_all(root.join("tests"))?;
        std::fs::create_dir_all(&foreign)?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"ordinary_shop\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )?;
        std::fs::write(
            root.join("src/lib.rs"),
            "pub fn discounted_total(amount: i32, threshold: i32) -> i32 {\n    if amount > threshold {\n        amount - 10\n    } else {\n        amount\n    }\n}\n",
        )?;
        std::fs::write(
            root.join("tests/pricing.rs"),
            "use ordinary_shop::discounted_total;\n#[test]\nfn below_threshold_has_no_discount() {\n    assert_eq!(discounted_total(50, 100), 50);\n}\n#[test]\nfn above_threshold_discounts() {\n    assert_eq!(discounted_total(1_000, 100), 990);\n}\n",
        )?;
        init_git_fixture_repo(&root)?;
        run_git(&root, &["add", "Cargo.toml", "src", "tests"])?;
        run_git(
            &root,
            &[
                "-c",
                "user.name=RIPR test",
                "-c",
                "user.email=ripr@example.invalid",
                "commit",
                "-m",
                "pricing crate",
            ],
        )?;
        std::fs::write(
            root.join("src/lib.rs"),
            "pub fn discounted_total(amount: i32, threshold: i32) -> i32 {\n    if amount >= threshold {\n        amount - 10\n    } else {\n        amount\n    }\n}\n",
        )?;
        let root_arg = root.display().to_string();
        let bin = env!("CARGO_BIN_EXE_ripr");
        // Literal identity for this retained repo-inventory predicate. Do not
        // derive the expectation with the production canonical-ID function.
        let gap = "gap:54c07ec8eeadf203";
        let inventory = run_command(
            bin,
            Some(&foreign),
            &[
                "check",
                "--root",
                &root_arg,
                "--format",
                "repo-exposure-json",
            ],
        )?;
        assert!(
            inventory.status.success(),
            "fixture inventory failed: {inventory:?}"
        );
        let inventory_json: serde_json::Value = serde_json::from_slice(&inventory.stdout)?;
        assert_eq!(
            inventory_json["seams"]
                .as_array()
                .ok_or("missing seams")?
                .len(),
            1
        );
        assert_eq!(
            inventory_json["seams"][0]["evidence_record"]["canonical_gap_id"],
            gap
        );

        let refused = run_command(
            bin,
            Some(&foreign),
            &["rerun", "--root", &root_arg, "--gap", gap],
        )?;
        assert_eq!(refused.status.code(), Some(2));
        assert!(refused.stdout.is_empty());
        assert!(
            !foreign.join("gap-ledger.json").exists(),
            "refusal must not create a ledger"
        );
        let stderr = String::from_utf8(refused.stderr)?;
        assert!(stderr.contains("rerun --gap requires --gap-ledger <path>"));
        assert!(
            stderr.contains("save stdout as repo-exposure.json"),
            "missing capture instruction: {stderr}"
        );
        assert!(
            stderr.contains("--check-output check.json"),
            "missing TypeScript producer: {stderr}"
        );
        assert!(
            stderr.contains("the ledger must contain the selected canonical gap ID"),
            "missing identity boundary: {stderr}"
        );
        let capture = stderr
            .split('`')
            .find(|part| part.starts_with("ripr check "))
            .ok_or("missing capture command")?;
        let producer = stderr
            .split('`')
            .find(|part| {
                part.starts_with("ripr reports gap-ledger ") && part.contains("--repo-exposure")
            })
            .ok_or("missing Rust ledger producer")?;
        // Independent literal argv expectations reject a wrong producer,
        // source kind, destination, or root even if it prints plausible text.
        let bound_root = if cfg!(windows) {
            root_arg.replace('\\', "/")
        } else {
            root_arg.clone()
        };
        assert_eq!(
            printed_ripr_args(capture)?,
            [
                "check",
                "--root",
                &bound_root,
                "--format",
                "repo-exposure-json"
            ]
        );
        assert_eq!(
            printed_ripr_args(producer)?,
            [
                "reports",
                "gap-ledger",
                "--root",
                &bound_root,
                "--repo-exposure",
                "repo-exposure.json",
                "--out",
                "gap-ledger.json"
            ]
        );
        let captured = replay(capture, &foreign)?;
        assert!(
            captured.status.success(),
            "printed capture failed: {captured:?}"
        );
        // Saving stdout is the explicit instruction, without shell-specific
        // redirect encoding or an invented check --out flag.
        std::fs::write(foreign.join("repo-exposure.json"), captured.stdout)?;
        let produced = replay(producer, &foreign)?;
        assert!(
            produced.status.success(),
            "printed producer failed: {produced:?}"
        );
        let ledger: serde_json::Value =
            serde_json::from_slice(&std::fs::read(foreign.join("gap-ledger.json"))?)?;
        assert_eq!(
            ledger["records"].as_array().ok_or("missing records")?.len(),
            1
        );
        assert_eq!(ledger["records"][0]["canonical_gap_id"], gap);
        assert!(stderr.contains("retry with `--gap-ledger gap-ledger.json`"));
        let rerun = run_command(
            bin,
            Some(&foreign),
            &[
                "rerun",
                "--root",
                &root_arg,
                "--gap",
                gap,
                "--gap-ledger",
                "gap-ledger.json",
                "--json",
            ],
        )?;
        assert!(rerun.status.success(), "recovered rerun failed: {rerun:?}");
        let selected: serde_json::Value = serde_json::from_slice(&rerun.stdout)?;
        assert_eq!(selected["state"], "current_state_only");
        assert_eq!(selected["selector"]["canonical_gap_id"], gap);
        assert_eq!(selected["selector"]["matched_record_count"], 1);
        assert_eq!(selected["selector"]["recomputed_scope_count"], 1);
        assert_eq!(
            selected["seams"]
                .as_array()
                .ok_or("missing selected seams")?
                .len(),
            1
        );

        // Capturing a repo ledger does not translate diff finding identities.
        let absent =
            "gap:rust:src/lib.rs:discounted_total:predicate_boundary:predicate:amount==threshold";
        let missing = run_command(
            bin,
            Some(&foreign),
            &[
                "rerun",
                "--root",
                &root_arg,
                "--gap",
                absent,
                "--gap-ledger",
                "gap-ledger.json",
                "--json",
            ],
        )?;
        assert!(
            missing.status.success(),
            "missing ID must return a typed limitation: {missing:?}"
        );
        let limited: serde_json::Value = serde_json::from_slice(&missing.stdout)?;
        assert_eq!(limited["state"], "limited");
        assert_eq!(limited["selector"]["matched_record_count"], 0);
        assert_eq!(limited["limitation"]["kind"], "canonical_gap_unresolved");
        assert_eq!(limited["seams"], serde_json::json!([]));
        Ok(())
    })();
    let cleanup = std::fs::remove_dir_all(&scratch);
    result?;
    cleanup?;
    Ok(())
}

#[test]
fn rerun_missing_gap_ledger_help_names_source_specific_capture_steps() -> TestResult {
    let help = run_command(env!("CARGO_BIN_EXE_ripr"), None, &["help", "rerun"])?;
    assert!(help.status.success());
    let text = String::from_utf8(help.stdout)?;
    for expected in [
        "ripr check --format repo-exposure-json",
        "ripr reports gap-ledger --repo-exposure repo-exposure.json --out gap-ledger.json",
        "ripr reports gap-ledger --check-output check.json --out gap-ledger.json",
        "the ledger must contain the selected canonical gap ID",
    ] {
        assert!(
            text.contains(expected),
            "rerun help must contain {expected:?}: {text}"
        );
    }
    Ok(())
}
// Literal identities for the real pricing inventory used by the recovery
// control above. Neither expectation is derived by the identity under test.
const ROOT_BINDING_GAP: &str = "gap:54c07ec8eeadf203";
const ROOT_BINDING_SOURCE: &str = "pub fn discounted_total(amount: i32, threshold: i32) -> i32 {\n    if amount >= threshold {\n        amount - 10\n    } else {\n        amount\n    }\n}\n";
const ROOT_BINDING_TESTS: &str = "use ordinary_shop::discounted_total;\n#[test]\nfn below_threshold_has_no_discount() {\n    assert_eq!(discounted_total(50, 100), 50);\n}\n#[test]\nfn above_threshold_discounts() {\n    assert_eq!(discounted_total(1_000, 100), 990);\n}\n";

/// Every CLI child goes through the existing owned process harness, with an
/// empty stdin and a deadline. The absolute Cargo-selected binary is the only
/// executable used; no PATH lookup, separate spawn site or frozen copy.
fn root_binding_probe(cwd: &Path, args: &[&str]) -> Result<Output, Box<dyn std::error::Error>> {
    let mut command = super::probe_command(env!("CARGO_BIN_EXE_ripr"));
    command
        .current_dir(cwd)
        .args(args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    Ok(super::run_owned_stdin_probe(
        command,
        &[],
        std::time::Duration::from_secs(30),
    )?)
}

fn root_binding_json(
    cwd: &Path,
    args: &[&str],
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let output = root_binding_probe(cwd, args)?;
    assert!(
        output.status.success(),
        "CLI setup or observation failed: {output:?}"
    );
    Ok(serde_json::from_slice(&output.stdout)?)
}

struct RootBindingFixture {
    scratch: std::path::PathBuf,
    root: std::path::PathBuf,
    foreign: std::path::PathBuf,
    ledger: std::path::PathBuf,
}

impl Drop for RootBindingFixture {
    fn drop(&mut self) {
        // Preserve recoverable ownership if the host prevents closeout.
        if let Err(error) = std::fs::remove_dir_all(&self.scratch) {
            eprintln!(
                "root-binding fixture retained at {}: {error}",
                self.scratch.display()
            );
        }
    }
}

fn root_binding_fixture(
    label: &str,
) -> Result<(RootBindingFixture, serde_json::Value), Box<dyn std::error::Error>> {
    root_binding_fixture_named(label, "repository A", "repository B")
}

fn root_binding_fixture_named(
    label: &str,
    root_name: &str,
    foreign_name: &str,
) -> Result<(RootBindingFixture, serde_json::Value), Box<dyn std::error::Error>> {
    let scratch = unique_temp_workspace(label);
    let fixture = RootBindingFixture {
        root: scratch.join(root_name),
        foreign: scratch.join(foreign_name),
        ledger: scratch
            .join(root_name)
            .join("target/ripr/reports/gap-decision-ledger.json"),
        scratch,
    };
    // B deliberately has identical analyzed bytes. A content-digest-only
    // implementation cannot substitute for new producers' concrete root.
    for root in [&fixture.root, &fixture.foreign] {
        std::fs::create_dir_all(root.join("src"))?;
        std::fs::create_dir_all(root.join("tests"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"ordinary_shop\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )?;
        std::fs::write(root.join("src/lib.rs"), ROOT_BINDING_SOURCE)?;
        std::fs::write(root.join("tests/pricing.rs"), ROOT_BINDING_TESTS)?;
        run_git(root, &["init"])?;
        run_git(root, &["config", "core.autocrlf", "false"])?;
        std::fs::write(root.join(".git/info/exclude"), "/target/\n")?;
        run_git(root, &["add", "Cargo.toml", "src", "tests"])?;
        run_git(
            root,
            &[
                "-c",
                "user.name=RIPR test",
                "-c",
                "user.email=ripr@example.invalid",
                "commit",
                "-m",
                "pricing crate",
            ],
        )?;
    }
    for relative in ["Cargo.toml", "src/lib.rs", "tests/pricing.rs"] {
        assert_eq!(
            std::fs::read(fixture.root.join(relative))?,
            std::fs::read(fixture.foreign.join(relative))?,
            "A and B must have byte-identical subject files before consuming A's ledger"
        );
    }
    let captured = root_binding_probe(&fixture.root, &["check", "--format", "repo-exposure-json"])?;
    assert!(
        captured.status.success(),
        "default capture failed: {captured:?}"
    );
    let inventory: serde_json::Value = serde_json::from_slice(&captured.stdout)?;
    assert_eq!(
        inventory["seams"].as_array().ok_or("missing seams")?.len(),
        1
    );
    assert_eq!(
        inventory["seams"][0]["evidence_record"]["canonical_gap_id"],
        ROOT_BINDING_GAP
    );
    assert_eq!(
        inventory["seams"][0]["owner"],
        "src/lib.rs::discounted_total"
    );
    std::fs::write(fixture.root.join("repo-exposure.json"), captured.stdout)?;
    // Both the root and destination use actual CLI defaults.
    let produced = root_binding_probe(
        &fixture.root,
        &[
            "reports",
            "gap-ledger",
            "--repo-exposure",
            "repo-exposure.json",
        ],
    )?;
    assert!(
        produced.status.success(),
        "default producer failed: {produced:?}"
    );
    let ledger: serde_json::Value = serde_json::from_slice(&std::fs::read(&fixture.ledger)?)?;
    assert_eq!(
        ledger["records"].as_array().ok_or("missing records")?.len(),
        1
    );
    assert_eq!(ledger["records"][0]["canonical_gap_id"], ROOT_BINDING_GAP);
    assert!(
        ledger["records"][0]["seam_id"]
            .as_str()
            .is_some_and(|id| !id.is_empty()),
        "the real producer must supply the seam identity retained after gap closure"
    );
    assert_eq!(ledger["records"][0]["anchor"]["file"], "src/lib.rs");
    assert_eq!(ledger["source_subject"]["digest_algorithm"], "sha256");
    let files = ledger["source_subject"]["files"]
        .as_array()
        .ok_or("missing producer source subject")?;
    assert!(
        !files.is_empty(),
        "real producer must stamp nonempty subjects"
    );
    for relative in ["src/lib.rs", "tests/pricing.rs"] {
        let file = files
            .iter()
            .find(|file| file["path"] == relative)
            .ok_or("named subject was not stamped")?;
        let digest = format!(
            "sha256:{:x}",
            <sha2::Sha256 as sha2::Digest>::digest(std::fs::read(fixture.root.join(relative))?)
        );
        assert_eq!(file["digest"], digest);
    }
    Ok((fixture, ledger))
}

fn root_binding_rerun(
    fixture: &RootBindingFixture,
    ledger: &Path,
    explicit_root: bool,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let ledger_arg = ledger.display().to_string();
    let root_arg = fixture.root.display().to_string();
    let mut args = vec![
        "rerun",
        "--gap",
        ROOT_BINDING_GAP,
        "--gap-ledger",
        &ledger_arg,
        "--json",
    ];
    if explicit_root {
        args.extend(["--root", &root_arg]);
    }
    root_binding_json(&fixture.foreign, &args)
}

fn assert_root_binding_selected(selected: &serde_json::Value, expected_class: &str) -> TestResult {
    assert_eq!(selected["state"], "current_state_only", "{selected}");
    assert_eq!(selected["selector"]["canonical_gap_id"], ROOT_BINDING_GAP);
    assert_eq!(selected["selector"]["matched_record_count"], 1);
    assert_eq!(selected["selector"]["recomputed_scope_count"], 1);
    let seams = selected["seams"]
        .as_array()
        .ok_or("missing selected seams")?;
    assert_eq!(seams.len(), 1, "{selected}");
    assert_eq!(seams[0]["canonical_gap_id"], ROOT_BINDING_GAP);
    assert_eq!(seams[0]["file"], "src/lib.rs");
    assert_eq!(seams[0]["owner"], "src/lib.rs::discounted_total");
    assert_eq!(seams[0]["line"], 2);
    assert_eq!(seams[0]["static_class"], expected_class);
    assert!(selected.get("movement").is_none());
    assert!(selected.get("limitation").is_none());
    Ok(())
}

fn assert_root_binding_refused(
    refused: &serde_json::Value,
    kind: &str,
    reason: &str,
) -> TestResult {
    assert_eq!(refused["state"], "limited", "{refused}");
    assert_eq!(refused["limitation"]["kind"], kind, "{refused}");
    assert!(
        refused["limitation"]["message"]
            .as_str()
            .ok_or("missing refusal message")?
            .contains(reason),
        "{refused}"
    );
    assert_eq!(refused["selector"]["matched_record_count"], 0);
    assert_eq!(refused["selector"]["recomputed_scope_count"], 0);
    assert_eq!(refused["seams"], serde_json::json!([]));
    assert_eq!(refused["cache"]["reuse_state"], "not_run");
    assert_eq!(refused["cache"]["file_fact_status"], "not_run");
    assert_eq!(refused["cache"]["hits"], 0);
    assert_eq!(refused["cache"]["misses"], 0);
    assert!(refused.get("route").is_none());
    Ok(())
}

/// #7382: a real default-root ledger cannot borrow B's meaning of `.`.
/// Identical source bytes make a digest-only repair fail this root oracle.
#[test]
fn rerun_default_gap_ledger_binds_producer_root_before_scope_recomputation() -> TestResult {
    let (fixture, ledger) = root_binding_fixture("rerun-default-ledger-root")?;
    let local = root_binding_json(
        &fixture.root,
        &[
            "rerun",
            "--gap",
            ROOT_BINDING_GAP,
            "--gap-ledger",
            "target/ripr/reports/gap-decision-ledger.json",
            "--json",
        ],
    )?;
    assert_root_binding_selected(&local, "weakly_gripped")?;
    let refused = root_binding_rerun(&fixture, &fixture.ledger, false)?;
    assert_root_binding_refused(&refused, "stale_gap_ledger", "does not match rerun root")?;
    let selected = root_binding_rerun(&fixture, &fixture.ledger, true)?;
    assert_root_binding_selected(&selected, "weakly_gripped")?;
    assert_eq!(
        selected["seams"][0]["seam_id"],
        ledger["records"][0]["seam_id"]
    );
    let recorded_root = Path::new(ledger["root"].as_str().ok_or("missing ledger root")?);
    assert!(
        recorded_root.is_absolute(),
        "producer persisted a CWD-relative root: {ledger}"
    );
    assert_eq!(recorded_root.canonicalize()?, fixture.root.canonicalize()?);
    Ok(())
}

/// Native `%` is a filename byte, not an encoded display token. Encoding A's
/// root must not select byte-identical sibling B whose literal name is `%25`.
#[test]
fn rerun_gap_ledger_preserves_literal_percent_root_and_refuses_encoded_sibling() -> TestResult {
    let (fixture, ledger) = root_binding_fixture_named(
        "rerun-percent-ledger-root",
        "repo%percent",
        "repo%25percent",
    )?;
    let expected_root = fixture.root.canonicalize()?;
    assert_ne!(expected_root, fixture.foreign.canonicalize()?);
    let local = root_binding_json(
        &fixture.root,
        &[
            "rerun",
            "--gap",
            ROOT_BINDING_GAP,
            "--gap-ledger",
            "target/ripr/reports/gap-decision-ledger.json",
            "--json",
        ],
    )?;
    let refused = root_binding_rerun(&fixture, &fixture.ledger, false)?;
    let explicit = root_binding_rerun(&fixture, &fixture.ledger, true)?;
    // Observe every consumer before asserting, so a red witness retains both
    // the real same-root false refusal and the distinct-root false admission.
    let observation = serde_json::json!({
        "persisted_root": ledger["root"],
        "default_a": {
            "state": local["state"],
            "matched": local["selector"]["matched_record_count"],
            "recomputed": local["selector"]["recomputed_scope_count"],
            "seams": local["seams"].as_array().map(Vec::len),
        },
        "default_b": {
            "state": refused["state"],
            "matched": refused["selector"]["matched_record_count"],
            "recomputed": refused["selector"]["recomputed_scope_count"],
            "seams": refused["seams"].as_array().map(Vec::len),
            "limitation": refused["limitation"]["kind"],
        },
        "explicit_a_from_b": {
            "state": explicit["state"],
            "matched": explicit["selector"]["matched_record_count"],
            "recomputed": explicit["selector"]["recomputed_scope_count"],
            "seams": explicit["seams"].as_array().map(Vec::len),
        },
    });
    eprintln!("literal-percent root observations: {observation}");
    assert_eq!(
        ledger["root"].as_str(),
        Some(
            expected_root
                .to_str()
                .ok_or("native fixture root is not UTF-8")?
        ),
        "persisted producer identity must be the native path, without display encoding"
    );
    assert_root_binding_selected(&local, "weakly_gripped")?;
    assert_root_binding_refused(&refused, "stale_gap_ledger", "does not match rerun root")?;
    assert_root_binding_selected(&explicit, "weakly_gripped")?;
    for selected in [local, explicit] {
        assert_eq!(
            selected["seams"][0]["seam_id"],
            ledger["records"][0]["seam_id"]
        );
    }
    Ok(())
}

/// Absolute root metadata already binds the checkout. Rerun must preserve its
/// existing edited-source recomputation contract, including older unstamped
/// records and explicit seam retention after the canonical gap closes.
#[test]
fn rerun_absolute_root_ledger_keeps_edited_source_and_unstamped_legacy_recomputation() -> TestResult
{
    let (fixture, mut ledger) = root_binding_fixture("rerun-bound-ledger-recompute")?;
    ledger["root"] = serde_json::json!(
        fixture
            .root
            .canonicalize()?
            .to_string_lossy()
            .replace('\\', "/")
    );
    let stamped = fixture.root.join("bound-stamped.json");
    std::fs::write(&stamped, serde_json::to_vec(&ledger)?)?;
    let legacy = fixture.root.join("bound-legacy.json");
    let object = ledger.as_object_mut().ok_or("ledger is not an object")?;
    object.remove("source_subject");
    object.remove("source_subject_unavailable");
    object.remove("inputs");
    std::fs::write(&legacy, serde_json::to_vec(&ledger)?)?;
    let after = format!(
        "{ROOT_BINDING_TESTS}#[test]\nfn equality_boundary_discounts() {{\n    assert_eq!(discounted_total(100, 100), 90);\n}}\n"
    );
    std::fs::write(fixture.root.join("tests/pricing.rs"), after)?;
    std::fs::write(
        fixture.root.join("src/lib.rs"),
        format!("{ROOT_BINDING_SOURCE}// source edited after ledger capture\n"),
    )?;
    for path in [stamped, legacy] {
        let selected = root_binding_rerun(&fixture, &path, true)?;
        assert_root_binding_selected(&selected, "strongly_gripped")?;
        assert_eq!(
            selected["seams"][0]["seam_id"],
            ledger["records"][0]["seam_id"]
        );
        assert!(
            selected["seams"][0]["missing_discriminators"]
                .as_array()
                .ok_or("missing discriminators")?
                .is_empty()
        );
        assert!(
            selected["seams"][0]["related_tests"]
                .as_array()
                .ok_or("missing related tests")?
                .iter()
                .any(|test| test["test_name"] == "equality_boundary_discounts")
        );
    }
    Ok(())
}

/// Legacy relative or missing roots cannot recover the producer's CWD. Use
/// the existing source-subject authority, with real records left unchanged.
#[test]
fn rerun_legacy_unbound_roots_require_current_complete_source_subject() -> TestResult {
    let (fixture, produced) = root_binding_fixture("rerun-legacy-ledger-subject")?;
    for root in [Some("."), Some("relative/producer"), None] {
        let mut ledger = produced.clone();
        let object = ledger.as_object_mut().ok_or("ledger is not an object")?;
        match root {
            Some(root) => {
                object.insert("root".to_string(), serde_json::json!(root));
            }
            None => {
                object.remove("root");
            }
        }
        let path = fixture.root.join("legacy-subject.json");
        std::fs::write(&path, serde_json::to_vec(&ledger)?)?;
        assert_root_binding_selected(
            &root_binding_rerun(&fixture, &path, true)?,
            "weakly_gripped",
        )?;
        // Matching is evaluated at explicit A, not at the foreign process CWD.
        std::fs::write(
            fixture.foreign.join("src/lib.rs"),
            "pub fn foreign_repository_only() {}\n",
        )?;
        assert_root_binding_refused(
            &root_binding_rerun(&fixture, &path, false)?,
            "stale_subject",
            "src/lib.rs",
        )?;
        let mut unsupported = ledger["source_subject"].clone();
        unsupported["digest_algorithm"] = serde_json::json!("md5");
        let mut incomplete = ledger["source_subject"].clone();
        incomplete["files"]
            .as_array_mut()
            .ok_or("missing subject files")?
            .retain(|file| file["path"] != "tests/pricing.rs");
        assert!(
            !incomplete["files"]
                .as_array()
                .ok_or("missing subject files")?
                .is_empty()
        );
        for (stamp, reason) in [
            (None, "source_subject_missing"),
            (Some(serde_json::Value::Null), "source_subject_malformed"),
            (
                Some(serde_json::json!({"files": "not an array"})),
                "source_subject_malformed",
            ),
            (Some(unsupported), "source_subject_unsupported_digest"),
            (Some(incomplete), "source_subject_incomplete"),
        ] {
            let mut unavailable = ledger.clone();
            let object = unavailable
                .as_object_mut()
                .ok_or("ledger is not an object")?;
            object.remove("source_subject");
            if let Some(stamp) = stamp {
                object.insert("source_subject".to_string(), stamp);
            }
            std::fs::write(&path, serde_json::to_vec(&unavailable)?)?;
            assert_root_binding_refused(
                &root_binding_rerun(&fixture, &path, true)?,
                "unverifiable_subject",
                reason,
            )?;
        }
    }
    Ok(())
}

/// The public production API loads the default ledger, validates its root,
/// paths, commands, typed specs and source subject, then projects diagnostics.
/// Filter by the ledger source so unrelated diff or seam output cannot stand
/// in for admission of this actual producer artifact.
fn root_binding_lsp_diagnostics(
    root: &Path,
) -> Result<Vec<serde_json::Value>, Box<dyn std::error::Error>> {
    let batches = ripr::lsp::workspace_diagnostic_batches(root)?;
    let mut selected = Vec::new();
    for batch in batches {
        for diagnostic in batch.diagnostics {
            if diagnostic
                .data
                .as_ref()
                .and_then(|data| data.get("source"))
                .and_then(serde_json::Value::as_str)
                == Some("gap_decision_ledger")
            {
                selected.push(serde_json::json!({
                    "uri": batch.uri.as_str(),
                    "diagnostic": diagnostic,
                }));
            }
        }
    }
    Ok(selected)
}

fn assert_root_binding_lsp_selected(
    selected: &[serde_json::Value],
    ledger: &serde_json::Value,
) -> TestResult {
    assert_eq!(
        selected.len(),
        1,
        "expected the real ledger diagnostic: {selected:?}"
    );
    let diagnostic = &selected[0]["diagnostic"];
    let data = &diagnostic["data"];
    assert_eq!(data["source"], "gap_decision_ledger");
    assert_eq!(data["canonical_gap_id"], ROOT_BINDING_GAP);
    assert_eq!(data["gap_id"], ledger["records"][0]["gap_id"]);
    assert_eq!(data["anchor"]["file"], "src/lib.rs");
    assert_eq!(data["anchor"]["owner"], "src/lib.rs::discounted_total");
    assert_eq!(data["anchor"]["line"], 2);
    assert_eq!(diagnostic["range"]["start"]["line"], 1);
    assert_eq!(data["gap_state"], "actionable");
    assert!(
        selected[0]["uri"]
            .as_str()
            .ok_or("missing projected URI")?
            .ends_with("/src/lib.rs")
    );
    assert!(
        !data["verification_commands"]
            .as_array()
            .ok_or("missing admitted verification commands")?
            .is_empty()
    );
    assert!(
        data["receipt_command"]
            .as_str()
            .is_some_and(|command| !command.is_empty())
    );
    Ok(())
}

/// Shared admission contract for a Unix symlink root and an ordinary Windows
/// root whose CLI producer uses the canonical extended-prefix spelling.
fn root_binding_lsp_alias_contract(
    fixture: &RootBindingFixture,
    ledger: &serde_json::Value,
    selected_root: &Path,
) -> TestResult {
    let physical_root = fixture.root.canonicalize()?;
    assert_eq!(selected_root.canonicalize()?, physical_root);
    assert_ne!(selected_root.as_os_str(), physical_root.as_os_str());
    for root in [&fixture.root, &fixture.foreign] {
        // Give the public API's normal default-base resolver a real local
        // main ref. No source bytes, records or source stamp are altered.
        run_git(root, &["branch", "-M", "main"])?;
    }
    let physical = root_binding_lsp_diagnostics(&physical_root)?;
    assert_root_binding_lsp_selected(&physical, ledger)?;
    // At least one genuine producer display carries A's absolute native
    // root. The alias case therefore passes through command-root validation
    // as well as report-root comparison, rather than testing `--root .` only.
    let mut has_bound_verify = false;
    for command in ledger["records"][0]["verification_commands"]
        .as_array()
        .ok_or("missing producer verification commands")?
    {
        let command = command.as_str().ok_or("verification command is not text")?;
        if !command.starts_with("ripr ") {
            continue;
        }
        let args = printed_ripr_args(command)?;
        for pair in args.windows(2).filter(|pair| pair[0] == "--root") {
            let root_arg = Path::new(&pair[1]);
            if root_arg.is_absolute() && root_arg.canonicalize()? == physical_root {
                has_bound_verify = true;
            }
        }
    }
    assert!(
        has_bound_verify,
        "the real producer must exercise an absolute command root"
    );

    // B's own real default producer is a nonempty positive control before
    // copying A's unchanged ledger bytes into B's default artifact location.
    let captured = root_binding_probe(
        &fixture.foreign,
        &["check", "--format", "repo-exposure-json"],
    )?;
    assert!(captured.status.success(), "B capture failed: {captured:?}");
    let inventory: serde_json::Value = serde_json::from_slice(&captured.stdout)?;
    assert_eq!(
        inventory["seams"]
            .as_array()
            .ok_or("missing B seams")?
            .len(),
        1
    );
    assert_eq!(
        inventory["seams"][0]["evidence_record"]["canonical_gap_id"],
        ROOT_BINDING_GAP
    );
    std::fs::write(fixture.foreign.join("repo-exposure.json"), captured.stdout)?;
    let produced = root_binding_probe(
        &fixture.foreign,
        &[
            "reports",
            "gap-ledger",
            "--repo-exposure",
            "repo-exposure.json",
        ],
    )?;
    assert!(produced.status.success(), "B producer failed: {produced:?}");
    let foreign_ledger = fixture
        .foreign
        .join("target/ripr/reports/gap-decision-ledger.json");
    let b_ledger: serde_json::Value = serde_json::from_slice(&std::fs::read(&foreign_ledger)?)?;
    assert_eq!(b_ledger["source_subject"], ledger["source_subject"]);
    assert_root_binding_lsp_selected(
        &root_binding_lsp_diagnostics(&fixture.foreign.canonicalize()?)?,
        &b_ledger,
    )?;
    let original_bytes = std::fs::read(&fixture.ledger)?;
    std::fs::write(&foreign_ledger, &original_bytes)?;
    let alias = root_binding_lsp_diagnostics(selected_root)?;
    let foreign = root_binding_lsp_diagnostics(&fixture.foreign)?;

    // Retain LSP's existing relative `.` and missing-root compatibility.
    // Only metadata changes; all real records and analysis-time stamps stay.
    let mut legacy_counts = Vec::new();
    for report_root in [Some("."), None] {
        let mut legacy = ledger.clone();
        let object = legacy.as_object_mut().ok_or("ledger is not an object")?;
        match report_root {
            Some(root) => {
                object.insert("root".to_string(), serde_json::json!(root));
            }
            None => {
                object.remove("root");
            }
        }
        std::fs::write(&fixture.ledger, serde_json::to_vec(&legacy)?)?;
        let current = root_binding_lsp_diagnostics(selected_root)?;
        legacy_counts.push(current.len());
        assert_root_binding_lsp_selected(&current, ledger)?;
        legacy
            .as_object_mut()
            .ok_or("ledger is not an object")?
            .remove("source_subject");
        std::fs::write(&fixture.ledger, serde_json::to_vec(&legacy)?)?;
        assert!(
            root_binding_lsp_diagnostics(selected_root)?.is_empty(),
            "an unstamped legacy artifact must remain unverifiable"
        );
    }
    // A foreign verification-command root remains rejected even when the
    // report's portable root is admissible and source bytes still match.
    let mut foreign_command = ledger.clone();
    foreign_command["root"] = serde_json::json!(".");
    foreign_command["records"][0]["verification_commands"] = serde_json::json!([format!(
        "ripr agent verify --root '{}' --before before.json --after after.json --json",
        fixture.foreign.display()
    )]);
    std::fs::write(&fixture.ledger, serde_json::to_vec(&foreign_command)?)?;
    assert!(
        root_binding_lsp_diagnostics(selected_root)?.is_empty(),
        "the alias repair must not admit commands bound to B"
    );
    std::fs::write(&fixture.ledger, &original_bytes)?;
    eprintln!(
        "LSP native-root admission observations: physical_A={}, selected_alias_A={}, own_B=1, foreign_ledger_B={}, legacy_current={legacy_counts:?}",
        physical.len(),
        alias.len(),
        foreign.len()
    );
    assert_root_binding_lsp_selected(&alias, ledger)?;
    assert!(
        foreign.is_empty(),
        "byte-identical B must refuse A's producer ledger: {foreign:?}"
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn lsp_real_gap_ledger_admits_symlink_selected_root_and_refuses_foreign_checkout() -> TestResult {
    let (fixture, ledger) = root_binding_fixture("lsp-ledger-symlink-root")?;
    let alias = fixture.scratch.join("editor-selected-alias");
    std::os::unix::fs::symlink(&fixture.root, &alias)?;
    assert!(std::fs::symlink_metadata(&alias)?.file_type().is_symlink());
    root_binding_lsp_alias_contract(&fixture, &ledger, &alias)
}

#[cfg(windows)]
#[test]
fn lsp_real_gap_ledger_admits_native_root_beside_canonical_prefix_and_refuses_foreign_checkout()
-> TestResult {
    let (fixture, ledger) = root_binding_fixture("lsp-ledger-native-prefix-root")?;
    // No symlink privilege is needed: the producer's canonical Windows root
    // and the editor's ordinary native path refer to this accessible fixture.
    root_binding_lsp_alias_contract(&fixture, &ledger, &fixture.root)
}
