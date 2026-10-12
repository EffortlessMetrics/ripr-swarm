//! Actual separate source/controller fixture for the #4510 admission seam.
//! This characterizes registry machinery; it does not qualify an installation.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

use super::{
    CandidateOperation, PINNED_JSON, RegistryDocument, controllers, evaluate_candidate_registry,
    pinned_artifact, read_artifact_tree, render_projection, repository_registry,
    resolve_candidate_authority, retire_template, sha256_hex, tree,
};

struct OwnedFixture {
    root: PathBuf,
    finished: bool,
    retain_for_proof_owner: bool,
}

impl OwnedFixture {
    fn finish(mut self) -> Result<(), String> {
        if self.retain_for_proof_owner {
            eprintln!(
                "4510 proof fixture retained: {}",
                json!({"root": self.root})
            );
        } else {
            fs::remove_dir_all(&self.root)
                .map_err(|error| format!("owned fixture cleanup failed: {error}"))?;
        }
        self.finished = true;
        Ok(())
    }
}

impl Drop for OwnedFixture {
    fn drop(&mut self) {
        if !self.finished {
            if self.retain_for_proof_owner {
                eprintln!(
                    "4510 proof fixture retained after unfinished test: {}",
                    json!({"root": self.root})
                );
            } else {
                let _ = fs::remove_dir_all(&self.root);
            }
        }
    }
}

fn git(root: &Path, values: &[&str]) -> Result<String, String> {
    let args = values
        .iter()
        .map(|value| (*value).to_string())
        .collect::<Vec<_>>();
    let output = crate::run::capture_bytes_in_dir_with_timeout(
        Path::new("git"),
        &args,
        root,
        &[],
        &["GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE"],
        Duration::from_secs(30),
        "4510 separate-source fixture Git",
    )?;
    if output.timed_out || !output.status.is_some_and(|status| status.success()) {
        return Err(format!(
            "fixture Git {values:?} in {} failed: timed_out={}, status={:?}, duration={:?}, stdout(first 4096 bytes)={}, stderr(first 4096 bytes)={}",
            root.display(),
            output.timed_out,
            output.status,
            output.duration,
            String::from_utf8_lossy(
                output
                    .stdout
                    .get(..output.stdout.len().min(4096))
                    .unwrap_or_default()
            ),
            String::from_utf8_lossy(
                output
                    .stderr
                    .get(..output.stderr.len().min(4096))
                    .unwrap_or_default()
            )
        ));
    }
    String::from_utf8(output.stdout)
        .map(|value| value.trim().to_string())
        .map_err(|error| format!("fixture Git output is not UTF-8: {error}"))
}

fn require_refusal<T>(result: Result<T, String>, expected: &str) -> Result<(), String> {
    match result {
        Ok(_) => Err(format!("custody unexpectedly accepted {expected}")),
        Err(error) if error.contains(expected) => Ok(()),
        Err(error) => Err(format!(
            "expected custody refusal {expected}; actual unrelated failure: {error}"
        )),
    }
}

fn initialize(root: &Path) -> Result<(), String> {
    git(root, &["init", "--initial-branch=main"])?;
    for (key, value) in [
        ("user.name", "RIPR fixture"),
        ("user.email", "fixture@example.invalid"),
        ("core.autocrlf", "false"),
        ("core.fsmonitor", "false"),
        ("commit.gpgsign", "false"),
        ("tag.gpgsign", "false"),
        ("core.hooksPath", ".fixture-empty-hooks"),
    ] {
        git(root, &["config", "--local", key, value])?;
    }
    Ok(())
}

fn replace_row(
    registry: &mut Value,
    artifact: &[u8],
    sha: &str,
    tree_id: &str,
    reference: &str,
) -> Result<(), String> {
    let row = registry
        .get_mut("artifacts")
        .and_then(Value::as_array_mut)
        .and_then(|rows| {
            rows.iter_mut()
                .find(|row| row.get("path") == Some(&json!(PINNED_JSON)))
        })
        .ok_or_else(|| "legal fixture has no candidate row".to_string())?;
    let object = row
        .as_object_mut()
        .ok_or_else(|| "legal candidate row is not an object".to_string())?;
    object.insert("sha256".to_string(), json!(sha256_hex(artifact)));
    object.insert(
        "candidate".to_string(),
        json!({"sha": sha, "tree": tree_id, "ref": reference}),
    );
    Ok(())
}

fn create_source_fixture() -> Result<SourceFixture, String> {
    // Explicit local proof custody only. The root's compiler-bound driver owns
    // namespace/identity, readers, archive/restore and final removal after exit.
    // Ordinary CI retains the existing finish/drop cleanup.
    let retain_for_proof_owner = match std::env::var("RIPR_4510_RETAIN_PROOF_FIXTURES") {
        Ok(value) if value == "1" => true,
        Err(std::env::VarError::NotPresent) => false,
        _ => return Err("RIPR_4510_RETAIN_PROOF_FIXTURES must be unset or exactly 1".to_string()),
    };
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("fixture clock: {error}"))?
        .as_nanos();
    let owned =
        std::env::temp_dir().join(format!("ripr-4510-source-{}-{stamp}", std::process::id()));
    fs::create_dir(&owned).map_err(|error| format!("exclusive fixture root: {error}"))?;
    let guard = OwnedFixture {
        root: owned,
        finished: false,
        retain_for_proof_owner,
    };
    let source = guard.root.join("source é");
    let controller = guard.root.join("controller with spaces");
    fs::create_dir(&source).map_err(|error| error.to_string())?;
    fs::create_dir(&controller).map_err(|error| error.to_string())?;
    initialize(&source)?;
    fs::create_dir(source.join("src")).map_err(|error| error.to_string())?;
    fs::write(source.join("Cargo.toml"),
        "[package]\nname='ripr'\nversion='0.11.0'\nedition='2024'\ninclude=['src/**','Cargo.toml','Cargo.lock']\n\n[workspace]\n")
        .map_err(|error| error.to_string())?;
    fs::write(
        source.join("src/main.rs"),
        "fn main() { println!(\"fixture source A\"); }\n",
    )
    .map_err(|error| error.to_string())?;
    fs::write(
        source.join("Cargo.lock"),
        "# This file is automatically @generated by Cargo.
version = 4

[[package]]
name = 'ripr'
version = '0.11.0'
",
    )
    .map_err(|error| error.to_string())?;
    fs::write(source.join(".gitignore"), "src/foreign.rs\n").map_err(|error| error.to_string())?;
    fs::write(source.join("source.bin"), [0, b'\n', 255, b' ', b'\n'])
        .map_err(|error| error.to_string())?;
    let fixture_input = source.join("fixtures/boundary_gap/input");
    fs::create_dir_all(&fixture_input).map_err(|error| error.to_string())?;
    fs::write(fixture_input.join("Cargo.toml"), "selected fixture A\n")
        .map_err(|error| error.to_string())?;
    git(
        &source,
        &[
            "add",
            "--",
            "Cargo.toml",
            "Cargo.lock",
            "src/main.rs",
            ".gitignore",
            "source.bin",
            "fixtures",
        ],
    )?;
    git(
        &source,
        &["commit", "-m", "fixture candidate before controller"],
    )?;
    let sha = git(&source, &["rev-parse", "HEAD"])?;
    let tree_id = git(&source, &["rev-parse", "HEAD^{tree}"])?;
    let reference = format!("refs/tags/ripr-release-0.11.0-{sha}");
    git(&source, &["update-ref", &reference, &sha])?;
    if git(&source, &["rev-parse", &reference])? != sha {
        return Err("fixture ref does not name the actual source commit".to_string());
    }

    let mut artifact: Value =
        serde_json::from_slice(&pinned_artifact()).map_err(|error| error.to_string())?;
    artifact
        .as_object_mut()
        .ok_or_else(|| "legal candidate artifact is not an object".to_string())?
        .insert("selected_swarm_parent".to_string(), json!(sha));
    let artifact = serde_json::to_vec_pretty(&artifact).map_err(|error| error.to_string())?;
    let mut registry = retire_template(repository_registry());
    replace_row(&mut registry, &artifact, &sha, &tree_id, &reference)?;
    let fixture = tree(&registry, &[(PINNED_JSON, artifact.clone())]);
    // Write the actual controller snapshot, including its derived projection.
    let document: RegistryDocument =
        serde_json::from_value(registry).map_err(|error| error.to_string())?;
    if render_projection(&document.artifacts).is_empty() {
        return Err("fixture projection unexpectedly empty".to_string());
    }
    initialize(&controller)?;
    for (path, bytes) in &fixture.files {
        let destination = controller.join(path);
        let parent = destination
            .parent()
            .ok_or_else(|| "fixture path has no parent".to_string())?;
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        fs::write(destination, bytes).map_err(|error| error.to_string())?;
    }
    fs::create_dir(controller.join("policy")).map_err(|error| error.to_string())?;
    fs::write(
        controller.join("policy/release-targets.toml"),
        include_bytes!("../../../../../../policy/release-targets.toml"),
    )
    .map_err(|error| error.to_string())?;
    git(&controller, &["add", "--", "docs", "policy"])?;
    git(
        &controller,
        &["commit", "-m", "control packet after source selection"],
    )?;
    if git(&controller, &["rev-parse", "HEAD"])? == sha {
        return Err("controller must be distinct from immutable source commit".to_string());
    }
    Ok(SourceFixture {
        guard,
        source,
        controller,
        sha,
        reference,
        artifact,
    })
}

struct SourceFixture {
    guard: OwnedFixture,
    source: PathBuf,
    controller: PathBuf,
    sha: String,
    reference: String,
    artifact: Vec<u8>,
}

#[test]
fn registered_source_fixture_precedes_separate_controller_commit() -> Result<(), String> {
    let SourceFixture {
        guard,
        source,
        controller,
        sha,
        artifact,
        ..
    } = create_source_fixture()?;
    let readback = read_artifact_tree(&controller);
    let outcome = evaluate_candidate_registry(&readback, &controllers());
    let validated = outcome.validated().map_err(|error| {
        format!(
            "legal actual-source fixture invalid: {error}; {:?}",
            outcome.violations
        )
    })?;
    let granted = resolve_candidate_authority(
        &validated,
        "0.11.0",
        &artifact,
        CandidateOperation::ExactCandidate,
    )?;
    if granted.candidate_sha() != Some(sha.as_str()) {
        return Err("registry did not retain actual candidate SHA".to_string());
    }
    if !git(
        &source,
        &["status", "--porcelain=v1", "--untracked-files=all"],
    )?
    .is_empty()
    {
        return Err("candidate source unexpectedly dirty".to_string());
    }
    let mut changed = artifact;
    changed.push(b' ');
    if resolve_candidate_authority(
        &validated,
        "0.11.0",
        &changed,
        CandidateOperation::ExactCandidate,
    )
    .is_ok()
    {
        return Err("changed actual controller artifact bytes admitted".to_string());
    }
    guard.finish()
}

#[test]
fn admitted_source_rechecks_actual_git_and_controller_bytes() -> Result<(), String> {
    use crate::reports::release::candidate_harness::{AdmittedSource, QualificationInput};
    let fixture = create_source_fixture()?;
    let input = QualificationInput::new(
        fixture.controller.clone(),
        fixture.source.clone(),
        PathBuf::from(PINNED_JSON),
    )?;
    let admitted = AdmittedSource::admit(&input, "0.11.0")?;
    let original_controller = read_artifact_tree(&fixture.controller);
    let mut wrong_tree_registry = retire_template(repository_registry());
    replace_row(
        &mut wrong_tree_registry,
        &fixture.artifact,
        &fixture.sha,
        &"0".repeat(40),
        &fixture.reference,
    )?;
    let wrong_tree = tree(
        &wrong_tree_registry,
        &[(PINNED_JSON, fixture.artifact.clone())],
    );
    let wrong_tree_outcome = evaluate_candidate_registry(&wrong_tree, &controllers());
    wrong_tree_outcome
        .validated()
        .map_err(|error| format!("wrong-tree registry fixture is not legal: {error}"))?;
    for (path, bytes) in &wrong_tree.files {
        fs::write(fixture.controller.join(path), bytes).map_err(|error| error.to_string())?;
    }
    require_refusal(
        AdmittedSource::admit(&input, "0.11.0"),
        &format!(
            "candidate source identity changed at {}^{{tree}}",
            fixture.sha
        ),
    )?;
    for (path, bytes) in &original_controller.files {
        fs::write(fixture.controller.join(path), bytes).map_err(|error| error.to_string())?;
    }
    AdmittedSource::admit(&input, "0.11.0")?.revalidate()?;
    if admitted.committed_file("source.bin") != Some([0, b'\n', 255, b' ', b'\n'].as_slice()) {
        return Err("Git binary batch capture changed non-UTF8/NUL/newline blob bytes".to_string());
    }
    if admitted.source_sha()? != fixture.sha
        || admitted.root()
            != fixture
                .source
                .canonicalize()
                .map_err(|error| error.to_string())?
    {
        return Err("source admission ignored selected source root/identity".to_string());
    }
    git(
        &fixture.source,
        &[
            "commit",
            "--allow-empty",
            "-m",
            "different actual source identity",
        ],
    )?;
    let other_sha = git(&fixture.source, &["rev-parse", "HEAD"])?;
    require_refusal(
        admitted.revalidate(),
        "candidate source identity changed at HEAD",
    )?;
    git(&fixture.source, &["checkout", "--detach", &fixture.sha])?;
    admitted.revalidate()?;
    git(
        &fixture.source,
        &["update-ref", &fixture.reference, &other_sha],
    )?;
    require_refusal(
        admitted.revalidate(),
        &format!("candidate source identity changed at {}", fixture.reference),
    )?;
    git(
        &fixture.source,
        &["update-ref", &fixture.reference, &fixture.sha],
    )?;
    admitted.revalidate()?;
    fs::write(
        fixture.source.join("src/main.rs"),
        "fn main() {}
",
    )
    .map_err(|error| error.to_string())?;
    require_refusal(
        admitted.revalidate(),
        "candidate source checkout is not clean",
    )?;
    git(&fixture.source, &["checkout", "--", "src/main.rs"])?;
    admitted.revalidate()?;
    let path = fixture.controller.join(PINNED_JSON);
    let mut changed = fixture.artifact.clone();
    changed.push(b' ');
    fs::write(&path, changed).map_err(|error| error.to_string())?;
    require_refusal(admitted.revalidate(), "candidate_digest")?;
    fs::write(path, &fixture.artifact).map_err(|error| error.to_string())?;
    admitted.revalidate()?;
    fixture.guard.finish()
}

#[test]
fn source_admission_refuses_same_alias_and_nested_physical_roots() -> Result<(), String> {
    use crate::reports::release::candidate_harness::{AdmittedSource, QualificationInput};
    let fixture = create_source_fixture()?;
    let artifact = PathBuf::from(PINNED_JSON);
    let separate = QualificationInput::new(
        fixture.controller.clone(),
        fixture.source.clone(),
        artifact.clone(),
    )?;
    AdmittedSource::admit(&separate, "0.11.0")?;
    let nested_source = fixture.controller.join("nested source");
    fs::create_dir(&nested_source).map_err(|error| error.to_string())?;
    for source in [
        fixture.controller.clone(),
        fixture.controller.join("."),
        nested_source,
    ] {
        let input = QualificationInput::new(fixture.controller.clone(), source, artifact.clone())?;
        require_refusal(
            AdmittedSource::admit(&input, "0.11.0"),
            "source/controller roots must be physically separate",
        )?;
    }
    // The other containment direction uses an actual valid controller tree,
    // copied below the source. The topology guard must win before dirty-source
    // or wrong-HEAD errors; no admitted handle is fabricated.
    let nested_controller = fixture.source.join("nested controller");
    fs::create_dir(&nested_controller).map_err(|error| error.to_string())?;
    let tree = read_artifact_tree(&fixture.controller);
    for (path, bytes) in tree.files {
        let output = nested_controller.join(path);
        let parent = output
            .parent()
            .ok_or_else(|| "nested controller file parent missing".to_string())?;
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        fs::write(output, bytes).map_err(|error| error.to_string())?;
    }
    fs::create_dir(nested_controller.join("policy")).map_err(|error| error.to_string())?;
    fs::copy(
        fixture.controller.join("policy/release-targets.toml"),
        nested_controller.join("policy/release-targets.toml"),
    )
    .map_err(|error| error.to_string())?;
    let input = QualificationInput::new(nested_controller, fixture.source.clone(), artifact)?;
    require_refusal(
        AdmittedSource::admit(&input, "0.11.0"),
        "source/controller roots must be physically separate",
    )?;
    fixture.guard.finish()
}

#[test]
fn real_package_install_custody_rejects_ignored_foreign_and_changed_bytes() -> Result<(), String> {
    use crate::reports::release::candidate_harness::{
        AdmittedSource, AttributedArchive, CandidateExecution, QualificationInput,
    };
    let fixture = create_source_fixture()?;
    // A valid ancestor Cargo workspace must not capture the attributed install input.
    // The selected source remains its own standalone workspace; archive bytes stay exact.
    fs::write(
        fixture.guard.root.join("Cargo.toml"),
        "[package]\nname='controller-workspace-fixture'\nversion='0.1.0'\nedition='2024'\n[workspace]\n",
    )
    .map_err(|error| error.to_string())?;
    fs::create_dir(fixture.guard.root.join("src")).map_err(|error| error.to_string())?;
    fs::write(
        fixture.guard.root.join("src/lib.rs"),
        "pub fn controller_fixture() {}\n",
    )
    .map_err(|error| error.to_string())?;
    let input = QualificationInput::new(
        fixture.controller.clone(),
        fixture.source.clone(),
        PathBuf::from(PINNED_JSON),
    )?;
    let owned = fixture.guard.root.join("actual package producer");
    fs::create_dir(&owned).map_err(|error| error.to_string())?;
    let archive = AttributedArchive::produce(AdmittedSource::admit(&input, "0.11.0")?, &owned)?;
    let archive_path = archive.archive_path().to_path_buf();
    let original = fs::read(&archive_path).map_err(|error| error.to_string())?;
    let mut changed = original.clone();
    changed.push(b' ');
    fs::write(&archive_path, changed).map_err(|error| error.to_string())?;
    require_refusal(archive.revalidate(), "byte budget")?;
    fs::write(&archive_path, original).map_err(|error| error.to_string())?;
    archive.revalidate()?;
    let installed = archive.install(&owned)?;
    if installed.fixture_bytes("Cargo.toml")? != b"selected fixture A\n" {
        return Err(
            "authentic fixture bytes did not come from admitted committed source".to_string(),
        );
    }
    let foreign = fixture.guard.root.join("foreign launch B");
    fs::create_dir(&foreign).map_err(|error| error.to_string())?;
    fs::write(
        foreign.join("Cargo.toml"),
        "[package]\nname='ripr'\nversion='0.11.0'\nedition='2024'\n",
    )
    .map_err(|error| error.to_string())?;
    fs::create_dir(foreign.join("src")).map_err(|error| error.to_string())?;
    fs::write(
        foreign.join("src/main.rs"),
        "fn main() { println!(\"decoy source B\"); }\n",
    )
    .map_err(|error| error.to_string())?;
    let output = CandidateExecution::Qualified(&installed).run(
        &[],
        &foreign,
        "actual installed source A token",
    )?;
    if !output.success || output.stdout.trim() != "fixture source A" {
        return Err(format!(
            "selected package A/foreign launch B custody lost: {}",
            output.stdout
        ));
    }
    let executable = fs::read(installed.binary()).map_err(|error| error.to_string())?;
    let mut changed = executable.clone();
    changed.push(b' ');
    fs::write(installed.binary(), changed).map_err(|error| error.to_string())?;
    require_refusal(installed.revalidate(), "byte budget")?;
    fs::write(installed.binary(), executable).map_err(|error| error.to_string())?;
    installed.revalidate()?;
    fs::write(
        fixture.source.join("src/foreign.rs"),
        "pub fn foreign() {}\n",
    )
    .map_err(|error| error.to_string())?;
    if !git(
        &fixture.source,
        &["status", "--porcelain=v1", "--untracked-files=all"],
    )?
    .is_empty()
    {
        return Err("ignored-included-file fixture is not actually Git-clean".to_string());
    }
    let rejected_root = fixture.guard.root.join("ignored included package");
    fs::create_dir(&rejected_root).map_err(|error| error.to_string())?;
    let refusal =
        AttributedArchive::produce(AdmittedSource::admit(&input, "0.11.0")?, &rejected_root);
    match refusal {
        Err(error)
            if error.contains(
                "packaged ordinary entry is not the committed source blob: src/foreign.rs",
            ) => {}
        Err(error)
            if error.contains("qualified cargo package failed with native status")
                && error.contains("src/foreign.rs")
                && (error.contains("not yet committed") || error.contains("uncommitted")) => {}
        Err(error) => {
            return Err(format!(
                "ignored-file control had unrelated failure: {error}"
            ));
        }
        Ok(_) => {
            return Err(
                "Git-clean ignored included foreign file was attributed to source A".to_string(),
            );
        }
    }
    fixture.guard.finish()
}

// Real dependency-free Cargo package -> install -> retained selected artifact ->
// witness -> the existing Qualified execution. This qualifies that harness
// boundary, not a RIPR release. Setup/compile failures are not product RED.
fn capture_fixture_input(
    fixture: &SourceFixture,
    label: &str,
) -> Result<
    (
        crate::reports::release::candidate_harness::QualificationInput,
        PathBuf,
    ),
    String,
> {
    use crate::reports::release::candidate_harness::QualificationInput;
    let owned = fixture.guard.root.join(label);
    fs::create_dir(&owned).map_err(|error| format!("capture fixture directory: {error}"))?;
    let input = QualificationInput::new(
        fixture.controller.clone(),
        fixture.source.clone(),
        PathBuf::from(PINNED_JSON),
    )?;
    Ok((
        input,
        owned.canonicalize().map_err(|error| error.to_string())?,
    ))
}

fn capture_observed_verbose_version(program: &str, root: &Path) -> Result<String, String> {
    let output = crate::run::capture_bytes_in_dir_with_timeout(
        Path::new(program),
        &["-vV".to_string()],
        root,
        &[],
        &[
            "CARGO_TARGET_DIR",
            "GIT_DIR",
            "GIT_WORK_TREE",
            "GIT_INDEX_FILE",
        ],
        Duration::from_secs(30),
        "4510 capture fixture actual verbose toolchain observation",
    )?;
    if output.timed_out || !output.status.is_some_and(|status| status.success()) {
        return Err(format!(
            "SETUP: {program} verbose observation failed: timed_out={}, status={:?}",
            output.timed_out, output.status
        ));
    }
    String::from_utf8(output.stdout)
        .map(|value| value.trim().to_string())
        .map_err(|error| format!("SETUP: {program} verbose output is not UTF-8: {error}"))
}

fn capture_assert_real_install_identity(
    custody: &Value,
    installed_path: &Path,
    installed_bytes: &[u8],
    owned: &Path,
    rustc: &str,
    cargo: &str,
) -> Result<PathBuf, String> {
    assert_eq!(custody["capture_mode"], "installed_output");
    assert_eq!(custody["executable"], json!(installed_path));
    assert_eq!(custody["installed_executable"], json!(installed_path));
    assert_eq!(custody["executable_sha256"], sha256_hex(installed_bytes));
    let artifact = &custody["compiler_artifact"];
    assert_eq!(artifact["reason"], "compiler-artifact");
    assert_eq!(artifact["target"]["name"], "ripr");
    assert_eq!(artifact["target"]["kind"], json!(["bin"]));
    assert_eq!(artifact["profile"]["test"], false);
    assert_eq!(artifact["features"], json!([]));
    assert!(
        artifact["package_id"]
            .as_str()
            .is_some_and(|value| { value.starts_with("path+file:") && value.ends_with("0.11.0") })
    );
    let selected = PathBuf::from(
        artifact["executable"]
            .as_str()
            .ok_or_else(|| "SETUP: real Cargo message lacks selected executable".to_string())?,
    );
    let selected_canonical = selected.canonicalize().map_err(|error| error.to_string())?;
    let owned_canonical = owned.canonicalize().map_err(|error| error.to_string())?;
    assert!(selected_canonical.starts_with(&owned_canonical));
    let manifest = PathBuf::from(
        artifact["manifest_path"]
            .as_str()
            .ok_or_else(|| "SETUP: real Cargo message lacks manifest_path".to_string())?,
    );
    assert_eq!(
        manifest.canonicalize().map_err(|error| error.to_string())?,
        owned
            .join("target/package/extracted/Cargo.toml")
            .canonicalize()
            .map_err(|error| error.to_string())?
    );
    let selected_bytes = fs::read(&selected).map_err(|error| error.to_string())?;
    assert!(!selected_bytes.is_empty());
    if selected_bytes != installed_bytes {
        return Err("SETUP: compiler-selected executable and installed bytes differ".to_string());
    }
    assert_eq!(
        custody["rustc_verbose_version"].as_str().map(str::trim),
        Some(rustc)
    );
    assert_eq!(
        custody["cargo_verbose_version"].as_str().map(str::trim),
        Some(cargo)
    );
    Ok(selected)
}

// 4510 successor proof: observed path metadata helper start.
// Test observations use existing native metadata; this is not custody authority.
#[derive(Debug, PartialEq, Eq)]
struct CapturePathMetadata {
    regular_file: bool,
    directory: bool,
    symlink: bool,
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
    #[cfg(unix)]
    links: u64,
}

fn capture_path_metadata(path: &Path) -> Result<CapturePathMetadata, String> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        format!(
            "witness path metadata unavailable at {}: {error}",
            path.display()
        )
    })?;
    #[cfg(unix)]
    use std::os::unix::fs::MetadataExt;
    let kind = metadata.file_type();
    Ok(CapturePathMetadata {
        regular_file: kind.is_file(),
        directory: kind.is_dir(),
        symlink: kind.is_symlink(),
        #[cfg(unix)]
        device: metadata.dev(),
        #[cfg(unix)]
        inode: metadata.ino(),
        #[cfg(unix)]
        links: metadata.nlink(),
    })
}
// 4510 successor proof: observed path metadata helper end.

#[test]
fn real_compiler_bound_witness_consumes_frozen_reference_after_mutable_outputs_change()
-> Result<(), String> {
    use crate::reports::release::candidate_harness::{
        AdmittedSource, AttributedArchive, CandidateExecution,
    };
    let fixture = create_source_fixture()?;
    let (input, owned) = capture_fixture_input(&fixture, "actual frozen-reference producer")?;
    let admitted = AdmittedSource::admit(&input, "0.11.0")?;
    assert_eq!(admitted.source_sha()?, fixture.sha);
    let rustc = capture_observed_verbose_version("rustc", &fixture.source)?;
    let cargo = capture_observed_verbose_version("cargo", &fixture.source)?;
    let mut installed = AttributedArchive::produce(admitted, &owned)?.install(&owned)?;
    let installed_path = installed.binary().to_path_buf();
    let installed_bytes = fs::read(&installed_path).map_err(|error| error.to_string())?;
    let original_custody = installed.custody_json();
    let selected = capture_assert_real_install_identity(
        &original_custody,
        &installed_path,
        &installed_bytes,
        &owned,
        &rustc,
        &cargo,
    )?;
    eprintln!("4510 real compiler/install binding before capture: {original_custody}");
    let foreign = fixture.guard.root.join("foreign consumer cwd");
    fs::create_dir(&foreign).map_err(|error| error.to_string())?;
    let before = CandidateExecution::Qualified(&installed).run(
        &[],
        &foreign,
        "4510 real installed source positive before capture",
    )?;
    assert!(before.success && before.status == Some(0));
    assert_eq!(before.stdout, "fixture source A\n");
    let slot = owned.join(format!(
        "source-{}-frozen{}",
        fixture.sha,
        std::env::consts::EXE_SUFFIX
    ));
    installed.freeze_into(&slot)?;
    let captured = installed.custody_json();
    assert_eq!(installed.binary(), slot.as_path());
    assert_eq!(captured["capture_mode"], "exclusive_frozen_reference");
    assert_eq!(captured["installed_executable"], json!(installed_path));
    assert_eq!(
        captured["compiler_artifact"],
        original_custody["compiler_artifact"]
    );
    assert_eq!(
        captured["rustc_verbose_version"],
        original_custody["rustc_verbose_version"]
    );
    assert_eq!(
        captured["cargo_verbose_version"],
        original_custody["cargo_verbose_version"]
    );
    assert_eq!(captured["source"], original_custody["source"]);
    assert_eq!(
        captured["archive_sha256"],
        original_custody["archive_sha256"]
    );
    assert_eq!(
        captured["executable_sha256"],
        original_custody["executable_sha256"]
    );
    assert_eq!(
        fs::read(&slot).map_err(|error| error.to_string())?,
        installed_bytes
    );

    // Both are owned real producer outputs, not archived original witnesses.
    // The unchanged admitted source/archive and frozen execution stay binding.
    assert_ne!(selected, slot);
    assert_ne!(installed_path, slot);
    fs::write(&selected, b"changed mutable compiler output\n")
        .map_err(|error| error.to_string())?;
    fs::write(&installed_path, b"changed mutable installed output\n")
        .map_err(|error| error.to_string())?;
    for _ in 0..2 {
        let result = CandidateExecution::Qualified(&installed).run(
            &[],
            &foreign,
            "4510 explicit frozen handle reuse",
        )?;
        assert!(result.success && result.status == Some(0));
        assert_eq!(result.stdout, "fixture source A\n");
        assert_eq!(installed.custody_json(), captured);
        assert_eq!(
            fs::read(&slot).map_err(|error| error.to_string())?,
            installed_bytes
        );
    }
    require_refusal(installed.freeze_into(&slot), "already frozen")?;
    assert_eq!(
        fs::read(&slot).map_err(|error| error.to_string())?,
        installed_bytes
    );
    eprintln!("4510 compiler-bound frozen handle proof: {captured}");
    fixture.guard.finish()
}

#[test]
fn real_compiler_bound_capture_refuses_occupied_regular_and_link_canaries() -> Result<(), String> {
    use crate::reports::release::candidate_harness::{
        AdmittedSource, AttributedArchive, CandidateExecution,
    };
    let fixture = create_source_fixture()?;
    let (input, owned) = capture_fixture_input(&fixture, "actual occupied-slot producer")?;
    let mut installed =
        AttributedArchive::produce(AdmittedSource::admit(&input, "0.11.0")?, &owned)?
            .install(&owned)?;
    let positive = CandidateExecution::Qualified(&installed).run(
        &[],
        &fixture.controller,
        "4510 actual installed positive before occupied slots",
    )?;
    assert!(positive.success && positive.status == Some(0));
    assert_eq!(positive.stdout, "fixture source A\n");
    let original_path = installed.binary().to_path_buf();
    let original_custody = installed.custody_json();
    eprintln!(
        "4510 real occupied-slot setup: {}",
        json!({"custody": original_custody, "native_success": positive.success,
            "native_status": positive.status, "native_stdout": positive.stdout})
    );
    #[cfg(unix)]
    {
        let parent_alias = fixture.guard.root.join("producer-parent-alias");
        std::os::unix::fs::symlink(&owned, &parent_alias).map_err(|error| error.to_string())?;
        let alias_slot = parent_alias.join("must-not-be-created");
        require_refusal(
            installed.freeze_into(&alias_slot),
            "capture destination must be an ordinary file in the owned producer root",
        )?;
        assert!(!owned.join("must-not-be-created").exists());
    }
    // 4510 successor proof: byte-equal occupied payload start.
    // Initial capture refuses even an actual byte-equal payload. Only the
    // already-published handle may reuse a frozen reference.
    let actual_bytes = fs::read(&original_path).map_err(|error| error.to_string())?;
    assert!(
        !actual_bytes.is_empty(),
        "real installed payload must be nonempty"
    );
    assert_eq!(
        original_custody["executable_sha256"],
        sha256_hex(&actual_bytes)
    );
    let byte_equal = owned.join(format!(
        "occupied-byte-equal{}",
        std::env::consts::EXE_SUFFIX
    ));
    fs::write(&byte_equal, &actual_bytes).map_err(|error| error.to_string())?;
    let byte_equal_before = capture_path_metadata(&byte_equal)?;
    let refused = installed.freeze_into(&byte_equal);
    assert!(
        fs::symlink_metadata(&byte_equal).is_ok(),
        "byte-equal occupied payload must survive initial capture refusal"
    );
    assert_eq!(
        fs::read(&byte_equal).map_err(|error| error.to_string())?,
        actual_bytes,
        "initial capture must not rewrite byte-equal occupied payload"
    );
    assert_eq!(
        capture_path_metadata(&byte_equal)?,
        byte_equal_before,
        "byte-equal occupied payload must preserve native type and identity"
    );
    require_refusal(refused, "capture destination creation refused")?;
    assert_eq!(installed.binary(), original_path.as_path());
    assert_eq!(installed.custody_json(), original_custody);
    // 4510 successor proof: byte-equal occupied payload end.

    let canary = owned.join("protected-canary");
    let canary_bytes = b"preserve this occupied witness canary\n";
    fs::write(&canary, canary_bytes).map_err(|error| error.to_string())?;
    let regular = owned.join("occupied-regular");
    fs::write(&regular, canary_bytes).map_err(|error| error.to_string())?;
    let hardlink = owned.join("occupied-hardlink");
    fs::hard_link(&canary, &hardlink).map_err(|error| error.to_string())?;
    let slots = vec![regular, hardlink];
    #[cfg(unix)]
    let slots = {
        let mut slots = slots;
        let symlink = owned.join("occupied-symlink");
        std::os::unix::fs::symlink(&canary, &symlink).map_err(|error| error.to_string())?;
        slots.push(symlink);
        slots
    };
    for slot in slots {
        // 4510 successor proof: occupied metadata before start.
        let slot_before = capture_path_metadata(&slot)?;
        let canary_before = capture_path_metadata(&canary)?;
        // 4510 successor proof: occupied metadata before end.
        let refused = installed.freeze_into(&slot);
        // 4510 successor proof: occupied metadata after start.
        assert!(
            fs::symlink_metadata(&slot).is_ok(),
            "occupied witness path must survive creation refusal: {}",
            slot.display()
        );
        assert_eq!(
            capture_path_metadata(&slot)?,
            slot_before,
            "occupied witness must preserve native type and identity: {}",
            slot.display()
        );
        assert_eq!(
            capture_path_metadata(&canary)?,
            canary_before,
            "creation refusal must preserve the canary target's native type and identity"
        );
        // 4510 successor proof: occupied metadata after end.
        let slot_bytes = fs::read(&slot).map_err(|error| error.to_string())?;
        if slot_bytes != canary_bytes
            || fs::read(&canary).map_err(|error| error.to_string())? != canary_bytes
        {
            return Err(format!(
                "capture truncated/replaced an occupied canary: {}; observed_len={}, observed_sha256={}; refusal={refused:?}",
                slot.display(),
                slot_bytes.len(),
                sha256_hex(&slot_bytes)
            ));
        }
        require_refusal(refused, "capture destination creation refused")?;
        assert_eq!(installed.binary(), original_path.as_path());
        assert_eq!(installed.custody_json(), original_custody);
    }
    let clean = owned.join(format!("exclusive-clean{}", std::env::consts::EXE_SUFFIX));
    installed.freeze_into(&clean)?;
    let consumed = CandidateExecution::Qualified(&installed).run(
        &[],
        &fixture.controller,
        "4510 actual clean slot after occupied refusals",
    )?;
    assert!(consumed.success && consumed.status == Some(0));
    assert_eq!(consumed.stdout, "fixture source A\n");
    assert_eq!(
        fs::read(&canary).map_err(|error| error.to_string())?,
        canary_bytes
    );
    eprintln!(
        "4510 occupied canaries and genuine clean capture: {}",
        installed.custody_json()
    );
    fixture.guard.finish()
}

#[test]
fn real_compiler_bound_partial_capture_cleans_only_its_observed_owned_object() -> Result<(), String>
{
    use crate::reports::release::candidate_harness::{
        AdmittedSource, AttributedArchive, CandidateExecution,
    };
    use std::io::Write;
    let fixture = create_source_fixture()?;
    let (input, owned) = capture_fixture_input(&fixture, "actual partial-capture producer")?;
    let mut installed =
        AttributedArchive::produce(AdmittedSource::admit(&input, "0.11.0")?, &owned)?
            .install(&owned)?;
    installed.revalidate()?;
    let before = CandidateExecution::Qualified(&installed).run(
        &[],
        &fixture.controller,
        "4510 real installed positive before partial write",
    )?;
    assert!(before.success && before.status == Some(0));
    assert_eq!(before.stdout, "fixture source A\n");
    let actual_bytes = fs::read(installed.binary()).map_err(|error| error.to_string())?;
    assert!(actual_bytes.len() > 16);
    let original_path = installed.binary().to_path_buf();
    let original_custody = installed.custody_json();
    eprintln!(
        "4510 real partial-write setup: {}",
        json!({"custody": original_custody, "native_success": before.success,
            "native_status": before.status, "native_stdout": before.stdout})
    );
    let partial = owned.join("owned-partial");
    let mut writer_reached = false;
    let refusal = installed.freeze_into_with_writer_for_test(&partial, |file, retained| {
        writer_reached = true;
        if retained != actual_bytes {
            return Err(std::io::Error::other(
                "wrong retained compiler-produced bytes",
            ));
        }
        file.write_all(&retained[..16])?;
        Err(std::io::Error::other("injected witness-copy failure"))
    });
    assert!(
        writer_reached,
        "must reach partial write after real exclusive creation"
    );
    require_refusal(refusal, "injected witness-copy failure")?;
    assert_eq!(installed.binary(), original_path.as_path());
    assert_eq!(installed.custody_json(), original_custody);
    #[cfg(unix)]
    assert!(
        !partial.exists(),
        "only this attempt's observed partial must be cleaned"
    );
    #[cfg(not(unix))]
    assert_eq!(
        fs::read(&partial).map_err(|error| error.to_string())?,
        actual_bytes[..16]
    );

    // Even the failure hook must refuse an occupied slot before its writer.
    let canary = owned.join("partial-protected-canary");
    let canary_bytes = b"do not clean a pre-existing or replacement witness\n";
    fs::write(&canary, canary_bytes).map_err(|error| error.to_string())?;
    // 4510 successor proof: partial canary metadata before start.
    let canary_before = capture_path_metadata(&canary)?;
    // 4510 successor proof: partial canary metadata before end.
    let mut occupied_writer_reached = false;
    let refused = installed.freeze_into_with_writer_for_test(&canary, |_, _| {
        occupied_writer_reached = true;
        Err(std::io::Error::other("occupied writer must not run"))
    });
    assert!(!occupied_writer_reached);
    // 4510 successor proof: partial canary metadata after start.
    assert!(
        fs::symlink_metadata(&canary).is_ok(),
        "occupied failure hook must retain its pre-existing canary"
    );
    assert_eq!(
        capture_path_metadata(&canary)?,
        canary_before,
        "occupied failure hook must preserve canary native type and identity"
    );
    // 4510 successor proof: partial canary metadata after end.
    assert_eq!(
        fs::read(&canary).map_err(|error| error.to_string())?,
        canary_bytes
    );
    require_refusal(refused, "capture destination creation refused")?;

    #[cfg(unix)]
    {
        let replaced = owned.join("partial-with-replacement");
        let own_backup = owned.join("owned-partial-backup");
        let refused = installed.freeze_into_with_writer_for_test(&replaced, |file, retained| {
            file.write_all(&retained[..16])?;
            fs::rename(&replaced, &own_backup)?;
            fs::hard_link(&canary, &replaced)?;
            Err(std::io::Error::other("injected replacement-path failure"))
        });
        require_refusal(refused, "injected replacement-path failure")?;
        // 4510 successor proof: hardlink replacement presence start.
        assert!(
            fs::symlink_metadata(&replaced).is_ok(),
            "failure cleanup must retain the hardlink replacement witness"
        );
        // 4510 successor proof: hardlink replacement presence end.
        assert_eq!(
            fs::read(&replaced).map_err(|error| error.to_string())?,
            canary_bytes
        );
        assert_eq!(
            fs::read(&canary).map_err(|error| error.to_string())?,
            canary_bytes
        );
        assert_eq!(
            fs::read(&own_backup).map_err(|error| error.to_string())?,
            actual_bytes[..16]
        );
        assert_eq!(installed.binary(), original_path.as_path());
    }
    // 4510 successor proof: single-link inode replacement start.
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let replaced = owned.join("partial-with-single-link-replacement");
        let own_backup = owned.join("owned-partial-single-link-backup");
        let replacement_bytes = b"retain this different single-link regular witness\n";
        let mut replacement_before = None;
        let refused = installed.freeze_into_with_writer_for_test(&replaced, |file, retained| {
            if retained != actual_bytes {
                return Err(std::io::Error::other(
                    "wrong retained compiler-produced bytes",
                ));
            }
            file.write_all(&retained[..16])?;
            fs::rename(&replaced, &own_backup)?;
            fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&replaced)?
                .write_all(replacement_bytes)?;
            let created = file.metadata()?;
            let replacement = fs::symlink_metadata(&replaced)?;
            if !replacement.file_type().is_file()
                || replacement.nlink() != 1
                || (replacement.dev(), replacement.ino()) == (created.dev(), created.ino())
            {
                return Err(std::io::Error::other(
                    "SETUP: replacement must be a different single-link regular inode",
                ));
            }
            replacement_before =
                Some(capture_path_metadata(&replaced).map_err(std::io::Error::other)?);
            Err(std::io::Error::other(
                "injected single-link replacement failure",
            ))
        });
        require_refusal(refused, "injected single-link replacement failure")?;
        assert!(
            fs::symlink_metadata(&replaced).is_ok(),
            "failure cleanup must retain a different single-link regular replacement"
        );
        let expected = replacement_before.ok_or_else(|| {
            "SETUP: writer did not observe the different single-link replacement".to_string()
        })?;
        assert_eq!(
            capture_path_metadata(&replaced)?,
            expected,
            "failure cleanup must preserve replacement type, single link and different inode"
        );
        assert_eq!(
            fs::read(&replaced).map_err(|error| error.to_string())?,
            replacement_bytes,
            "failure cleanup must not modify the single-link replacement bytes"
        );
        assert_eq!(
            fs::read(&own_backup).map_err(|error| error.to_string())?,
            actual_bytes[..16]
        );
        assert_eq!(installed.binary(), original_path.as_path());
        assert_eq!(installed.custody_json(), original_custody);
    }
    // 4510 successor proof: single-link inode replacement end.

    let retry = owned.join(format!(
        "complete-after-failure{}",
        std::env::consts::EXE_SUFFIX
    ));
    installed.freeze_into(&retry)?;
    assert_eq!(
        fs::read(&retry).map_err(|error| error.to_string())?,
        actual_bytes
    );
    installed.revalidate()?;
    let consumed = CandidateExecution::Qualified(&installed).run(
        &[],
        &fixture.controller,
        "4510 real complete retry after partial failure",
    )?;
    assert!(consumed.success && consumed.status == Some(0));
    assert_eq!(consumed.stdout, "fixture source A\n");
    eprintln!(
        "4510 partial cleanup and real complete retry: {}",
        installed.custody_json()
    );
    fixture.guard.finish()
}

// 4510 successor proof: competing creators test start.
#[test]
fn real_compiler_bound_competing_capture_creators_leave_one_complete_witness() -> Result<(), String>
{
    use crate::reports::release::candidate_harness::{
        AdmittedSource, AttributedArchive, CandidateExecution,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Condvar, Mutex};

    let fixture = create_source_fixture()?;
    let (input, owned) = capture_fixture_input(&fixture, "actual competing-capture producer")?;
    let rustc = capture_observed_verbose_version("rustc", &fixture.source)?;
    let cargo = capture_observed_verbose_version("cargo", &fixture.source)?;
    let mut installed =
        AttributedArchive::produce(AdmittedSource::admit(&input, "0.11.0")?, &owned)?
            .install(&owned)?;
    let original_path = installed.binary().to_path_buf();
    let actual_bytes = fs::read(&original_path).map_err(|error| error.to_string())?;
    let original_custody = installed.custody_json();
    capture_assert_real_install_identity(
        &original_custody,
        &original_path,
        &actual_bytes,
        &owned,
        &rustc,
        &cargo,
    )?;
    let before = CandidateExecution::Qualified(&installed).run(
        &[],
        &fixture.controller,
        "4510 real installed positive before competing creators",
    )?;
    assert!(before.success && before.status == Some(0));
    assert_eq!(before.stdout, "fixture source A\n");
    eprintln!(
        "4510 real competing-creator setup: {}",
        json!({"custody": original_custody, "native_success": before.success,
            "native_status": before.status, "native_stdout": before.stdout})
    );
    let slot = owned.join(format!(
        "one-competing-witness{}",
        std::env::consts::EXE_SUFFIX
    ));
    assert!(
        !slot.exists(),
        "SETUP: competing witness slot must be absent"
    );
    let gate = (Mutex::new(0_usize), Condvar::new());
    let gate_deadline = std::time::Instant::now() + Duration::from_secs(30);
    let reached = AtomicUsize::new(0);
    let attempt = || {
        // Both callbacks run after any precheck and immediately before the
        // same native creation owner. The test never copies an executable.
        let result = installed.capture_unpublished_with_gate_for_test(&slot, || {
            let mut participants = gate.0.lock().map_err(|error| {
                format!("INSTRUMENT: competing capture creation gate lock poisoned: {error}")
            })?;
            reached.fetch_add(1, Ordering::SeqCst);
            *participants += 1;
            gate.1.notify_all();
            if std::time::Instant::now() >= gate_deadline {
                return Err("INSTRUMENT: competing capture creation gate deadline expired before both creators arrived".to_string());
            }
            while *participants < 2 {
                let remaining = gate_deadline
                    .checked_duration_since(std::time::Instant::now())
                    .filter(|remaining| !remaining.is_zero())
                    .ok_or_else(|| {
                        "INSTRUMENT: competing capture creation gate deadline expired before both creators arrived".to_string()
                    })?;
                let (observed, wait) = gate.1.wait_timeout(participants, remaining)
                    .map_err(|error| {
                        format!("INSTRUMENT: competing capture creation gate wait poisoned: {error}")
                    })?;
                participants = observed;
                if wait.timed_out() && *participants < 2 {
                    return Err("INSTRUMENT: competing capture creation gate deadline expired before both creators arrived".to_string());
                }
            }
            Ok(())
        });
        let completed_metadata = result.is_ok().then(|| capture_path_metadata(&slot));
        (result, completed_metadata)
    };
    let outcomes = std::thread::scope(|scope| {
        let first = scope.spawn(attempt);
        let second = scope.spawn(attempt);
        // Reap both scoped workers before propagating either failure. A peer
        // that fails before its gate cannot leave an unbounded arrival wait.
        let first = first.join();
        let second = second.join();
        let first = first.map_err(|_panic_payload| {
            "INSTRUMENT: competing capture first worker did not complete normally".to_string()
        })?;
        let second = second.map_err(|_panic_payload| {
            "INSTRUMENT: competing capture second worker did not complete normally".to_string()
        })?;
        Ok::<_, String>([first, second])
    })?;
    for (result, _) in &outcomes {
        if let Err(error) = result
            && error.starts_with("INSTRUMENT:")
        {
            return Err(error.clone());
        }
    }
    assert_eq!(
        reached.load(Ordering::SeqCst),
        2,
        "both real capture attempts must reach the native-creation gate"
    );
    let mut winners = 0;
    let mut refusals = Vec::new();
    let mut winner_metadata = None;
    for (result, completed_metadata) in outcomes {
        match result {
            Ok(()) => {
                winners += 1;
                winner_metadata = completed_metadata;
            }
            Err(error) => refusals.push(error),
        }
    }
    assert_eq!(
        winners, 1,
        "competing creators must publish exactly one complete capture; refusals={refusals:?}"
    );
    assert_eq!(
        refusals.len(),
        1,
        "competing creators must have exactly one loser"
    );
    require_refusal(
        Err::<(), String>(refusals[0].clone()),
        "capture destination creation refused",
    )?;
    assert!(
        fs::symlink_metadata(&slot).is_ok(),
        "losing creator must not remove the completed winning witness"
    );
    let winner_metadata = winner_metadata
        .ok_or_else(|| "successful capture did not observe its completed native file".to_string())?
        .map_err(|error| format!("completed winning witness was lost: {error}"))?;
    assert!(winner_metadata.regular_file && !winner_metadata.symlink);
    #[cfg(unix)]
    assert_eq!(winner_metadata.links, 1);
    assert_eq!(
        capture_path_metadata(&slot)?,
        winner_metadata,
        "losing creator must preserve the winner's complete native file and inode"
    );
    assert_eq!(
        fs::read(&slot).map_err(|error| error.to_string())?,
        actual_bytes,
        "winning witness must contain the complete real compiler-selected installed bytes"
    );
    assert_eq!(installed.binary(), original_path.as_path());
    assert_eq!(installed.custody_json(), original_custody);

    // The gate intentionally publishes no InstalledCandidate state. Reopening
    // that completed slot is initial recapture, never implicit handle reuse.
    require_refusal(
        installed.freeze_into(&slot),
        "capture destination creation refused",
    )?;
    assert_eq!(capture_path_metadata(&slot)?, winner_metadata);
    assert_eq!(
        fs::read(&slot).map_err(|error| error.to_string())?,
        actual_bytes
    );
    assert_eq!(installed.binary(), original_path.as_path());
    assert_eq!(installed.custody_json(), original_custody);
    let clean = owned.join(format!(
        "published-after-competing-capture{}",
        std::env::consts::EXE_SUFFIX
    ));
    installed.freeze_into(&clean)?;
    assert_eq!(
        fs::read(&clean).map_err(|error| error.to_string())?,
        actual_bytes
    );
    let consumed = CandidateExecution::Qualified(&installed).run(
        &[],
        &fixture.controller,
        "4510 actual published capture after competing creators",
    )?;
    assert!(consumed.success && consumed.status == Some(0));
    assert_eq!(consumed.stdout, "fixture source A\n");
    eprintln!(
        "4510 competing creation and genuine published consumer: {}",
        json!({"custody": installed.custody_json(), "successful_creators": winners,
            "refusals": refusals, "unpublished_witness": slot,
            "unpublished_sha256": sha256_hex(&actual_bytes)})
    );
    fixture.guard.finish()
}
// 4510 successor proof: competing creators test end.

// 4510 independent proof: actual consumer owned-root cleanup start.
#[cfg(unix)]
#[test]
fn real_compiler_bound_consumer_retains_failed_capture_root_and_cleans_success()
-> Result<(), String> {
    use crate::reports::capture_with_owned_root_for_test;
    use crate::reports::release::candidate_harness::{
        AdmittedSource, AttributedArchive, CandidateExecution,
    };
    use std::io::Write;
    use std::os::unix::fs::MetadataExt;

    let fixture = create_source_fixture()?;
    let (input, owned) = capture_fixture_input(&fixture, "actual consumer-guard producer")?;
    let admitted = AdmittedSource::admit(&input, "0.11.0")?;
    assert_eq!(admitted.source_sha()?, fixture.sha);
    let rustc = capture_observed_verbose_version("rustc", &fixture.source)?;
    let cargo = capture_observed_verbose_version("cargo", &fixture.source)?;
    let mut installed = AttributedArchive::produce(admitted, &owned)?.install(&owned)?;
    let original_path = installed.binary().to_path_buf();
    let actual_bytes = fs::read(&original_path).map_err(|error| error.to_string())?;
    assert!(
        actual_bytes.len() > 16,
        "real installed executable must supply a nonempty strict prefix"
    );
    let original_custody = installed.custody_json();
    let selected = capture_assert_real_install_identity(
        &original_custody,
        &original_path,
        &actual_bytes,
        &owned,
        &rustc,
        &cargo,
    )?;
    let archive = PathBuf::from(original_custody["archive"].as_str().ok_or_else(|| {
        "SETUP: actual package custody must name its emitted archive".to_string()
    })?);
    assert!(
        archive
            .canonicalize()
            .map_err(|error| error.to_string())?
            .starts_with(&owned)
    );
    let actual_archive = fs::read(&archive).map_err(|error| error.to_string())?;
    assert!(
        !actual_archive.is_empty(),
        "actual Cargo package archive must be nonempty"
    );
    assert_eq!(
        original_custody["archive_sha256"],
        sha256_hex(&actual_archive)
    );
    let root_before = capture_path_metadata(&owned)?;
    let compiler_before = capture_path_metadata(&selected)?;
    let installed_before = capture_path_metadata(&original_path)?;
    let archive_before = capture_path_metadata(&archive)?;
    let positive = CandidateExecution::Qualified(&installed).run(
        &[],
        &fixture.controller,
        "4510 actual installed positive before consumer cleanup control",
    )?;
    assert!(positive.success && positive.status == Some(0));
    assert_eq!(positive.stdout, "fixture source A\n");

    // This explicit destructive control needs pre-failure copies outside the
    // producer root. These are observed bytes, never rebuilt originals or a
    // replacement for the source/Cargo/toolchain custody authority.
    let saved_executable = fixture.guard.root.join(format!(
        "observed-compiler-executable{}",
        std::env::consts::EXE_SUFFIX
    ));
    let mut saved_file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&saved_executable)
        .map_err(|error| format!("save observed compiler bytes: {error}"))?;
    saved_file
        .write_all(&actual_bytes)
        .map_err(|error| error.to_string())?;
    saved_file
        .set_permissions(
            fs::metadata(&selected)
                .map_err(|error| error.to_string())?
                .permissions(),
        )
        .map_err(|error| error.to_string())?;
    saved_file.sync_all().map_err(|error| error.to_string())?;
    drop(saved_file);
    let saved_archive = fixture.guard.root.join("observed-cargo-package.crate");
    let mut saved_file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&saved_archive)
        .map_err(|error| format!("save observed archive bytes: {error}"))?;
    saved_file
        .write_all(&actual_archive)
        .map_err(|error| error.to_string())?;
    saved_file.sync_all().map_err(|error| error.to_string())?;
    drop(saved_file);
    assert_eq!(
        fs::read(&saved_executable).map_err(|error| error.to_string())?,
        actual_bytes
    );
    assert_eq!(
        fs::read(&saved_archive).map_err(|error| error.to_string())?,
        actual_archive
    );
    let observations = json!({
        "custody": original_custody, "producer_root": owned,
        "compiler_selected": selected, "native_success": positive.success,
        "native_status": positive.status, "native_stdout": positive.stdout,
        "observed_executable_copy": saved_executable,
        "observed_archive_copy": saved_archive,
        "copies_claim": "exact pre-failure observed bytes; not rebuilt or reconstituted originals"
    });
    let observations_path = fixture.guard.root.join("observed-consumer-input.json");
    let mut saved_file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&observations_path)
        .map_err(|error| error.to_string())?;
    saved_file
        .write_all(&serde_json::to_vec_pretty(&observations).map_err(|error| error.to_string())?)
        .map_err(|error| error.to_string())?;
    saved_file.sync_all().map_err(|error| error.to_string())?;
    drop(saved_file);
    eprintln!("4510 actual consumer pre-failure observations: {observations}");

    let slot = owned.join("consumer-failure-single-link-replacement");
    let own_backup = owned.join("consumer-created-partial-backup");
    let replacement_bytes = b"retain this single-link consumer failure witness\n";
    let mut writer_reached = false;
    let mut replacement_before = None;
    let refused = capture_with_owned_root_for_test(&owned, || {
        installed.freeze_into_with_writer_for_test(&slot, |file, retained| {
            writer_reached = true;
            if retained != actual_bytes {
                return Err(std::io::Error::other(
                    "SETUP: wrong retained compiler-produced bytes",
                ));
            }
            file.write_all(&retained[..16])?;
            fs::rename(&slot, &own_backup)?;
            fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&slot)?
                .write_all(replacement_bytes)?;
            let created = file.metadata()?;
            let replacement = fs::symlink_metadata(&slot)?;
            if !replacement.file_type().is_file()
                || replacement.nlink() != 1
                || (replacement.dev(), replacement.ino()) == (created.dev(), created.ino())
            {
                return Err(std::io::Error::other(
                    "SETUP: consumer replacement must be a different single-link regular inode",
                ));
            }
            replacement_before = Some(capture_path_metadata(&slot).map_err(std::io::Error::other)?);
            Err(std::io::Error::other(
                "injected consumer-root capture failure",
            ))
        })
    });
    assert!(
        writer_reached,
        "SETUP: actual consumer must reach the retained-byte failure writer"
    );
    let refusal = refused
        .err()
        .ok_or_else(|| "consumer unexpectedly accepted injected capture failure".to_string())?;
    require_refusal(
        Err::<(), String>(refusal.clone()),
        "injected consumer-root capture failure",
    )?;
    assert!(
        owned.is_dir(),
        "consumer Drop must retain the original producer root after capture failure"
    );
    assert!(
        fs::symlink_metadata(&slot).is_ok(),
        "consumer Drop must retain the single-link replacement witness"
    );
    assert!(
        fs::symlink_metadata(&own_backup).is_ok(),
        "consumer Drop must retain the original attempt-owned partial backup"
    );
    let replacement_before = replacement_before.ok_or_else(|| {
        "SETUP: actual writer did not observe its single-link replacement".to_string()
    })?;
    assert_eq!(capture_path_metadata(&owned)?, root_before);
    assert_eq!(capture_path_metadata(&slot)?, replacement_before);
    assert_eq!(
        fs::read(&slot).map_err(|error| error.to_string())?,
        replacement_bytes
    );
    assert_eq!(
        fs::read(&own_backup).map_err(|error| error.to_string())?,
        actual_bytes[..16]
    );
    assert_eq!(capture_path_metadata(&selected)?, compiler_before);
    assert_eq!(capture_path_metadata(&original_path)?, installed_before);
    assert_eq!(capture_path_metadata(&archive)?, archive_before);
    assert_eq!(
        fs::read(&selected).map_err(|error| error.to_string())?,
        actual_bytes
    );
    assert_eq!(
        fs::read(&original_path).map_err(|error| error.to_string())?,
        actual_bytes
    );
    assert_eq!(
        fs::read(&archive).map_err(|error| error.to_string())?,
        actual_archive
    );
    assert!(
        refusal.contains(&owned.display().to_string()),
        "consumer refusal must name its retained producer root: {refusal}"
    );
    assert_eq!(installed.binary(), original_path.as_path());
    assert_eq!(installed.custody_json(), original_custody);
    installed.revalidate()?;
    eprintln!("4510 actual consumer retained capture-failure root: {refusal}");

    let refused = capture_with_owned_root_for_test(&owned, || installed.freeze_into(&slot));
    let refusal = refused.err().ok_or_else(|| {
        "consumer unexpectedly reused an occupied initial capture slot".to_string()
    })?;
    require_refusal(
        Err::<(), String>(refusal.clone()),
        "capture destination creation refused",
    )?;
    assert!(
        owned.is_dir(),
        "consumer Drop must retain producer root after occupied capture refusal"
    );
    assert_eq!(capture_path_metadata(&owned)?, root_before);
    assert_eq!(capture_path_metadata(&slot)?, replacement_before);
    assert_eq!(
        fs::read(&slot).map_err(|error| error.to_string())?,
        replacement_bytes
    );
    assert_eq!(
        fs::read(&own_backup).map_err(|error| error.to_string())?,
        actual_bytes[..16]
    );
    assert_eq!(capture_path_metadata(&selected)?, compiler_before);
    assert_eq!(capture_path_metadata(&original_path)?, installed_before);
    assert_eq!(capture_path_metadata(&archive)?, archive_before);
    assert_eq!(
        fs::read(&selected).map_err(|error| error.to_string())?,
        actual_bytes
    );
    assert_eq!(
        fs::read(&original_path).map_err(|error| error.to_string())?,
        actual_bytes
    );
    assert_eq!(
        fs::read(&archive).map_err(|error| error.to_string())?,
        actual_archive
    );
    assert!(
        refusal.contains(&owned.display().to_string()),
        "occupied capture refusal must name its retained producer root: {refusal}"
    );
    assert_eq!(installed.binary(), original_path.as_path());
    assert_eq!(installed.custody_json(), original_custody);

    // A fresh guard owns normal successful cleanup. A blanket keep-on-Drop
    // cannot satisfy this actual capture and Qualified execution positive.
    let clean = owned.join(format!(
        "consumer-successful-capture{}",
        std::env::consts::EXE_SUFFIX
    ));
    let mut successful_observations = None;
    capture_with_owned_root_for_test(&owned, || {
        installed.freeze_into(&clean)?;
        assert_eq!(
            fs::read(&clean).map_err(|error| error.to_string())?,
            actual_bytes
        );
        let consumed = CandidateExecution::Qualified(&installed).run(
            &[],
            &fixture.controller,
            "4510 actual consumer successful capture before normal cleanup",
        )?;
        assert!(consumed.success && consumed.status == Some(0));
        assert_eq!(consumed.stdout, "fixture source A\n");
        let captured = installed.custody_json();
        assert_eq!(captured["capture_mode"], "exclusive_frozen_reference");
        assert_eq!(
            captured["compiler_artifact"],
            original_custody["compiler_artifact"]
        );
        assert_eq!(
            captured["executable_sha256"],
            original_custody["executable_sha256"]
        );
        successful_observations = Some(json!({"custody": captured,
            "native_success": consumed.success, "native_status": consumed.status,
            "native_stdout": consumed.stdout}));
        Ok(())
    })?;
    let successful_observations = successful_observations.ok_or_else(|| {
        "SETUP: consumer must capture and execute before returning success".to_string()
    })?;
    assert!(
        !owned.exists(),
        "successful consumer guard must perform normal producer-root cleanup"
    );
    assert_eq!(
        fs::read(&saved_executable).map_err(|error| error.to_string())?,
        actual_bytes
    );
    assert_eq!(
        fs::read(&saved_archive).map_err(|error| error.to_string())?,
        actual_archive
    );
    assert_eq!(
        serde_json::from_slice::<Value>(
            &fs::read(&observations_path).map_err(|error| error.to_string())?
        )
        .map_err(|error| error.to_string())?,
        observations
    );
    eprintln!("4510 actual consumer successful cleanup observations: {successful_observations}");
    fixture.guard.finish()
}
// 4510 independent proof: actual consumer owned-root cleanup end.
