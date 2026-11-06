//! Pinned Rust fixture history and the public route up to awaiting_edit.
//! No edit, project verification, static comparison or receipt is executed.

use super::super::CommandResult;
use super::{CandidateExecution, archive::InstalledCandidate};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::{Component, Path},
};

// Reuse the existing cross-shell replay decoder unchanged. This is shared
// harness source, not another product command parser or a shell interpreter.
#[path = "../../../../../crates/ripr/tests/common/printed_words.rs"]
mod printed_words;

const FILES: [&str; 3] = ["Cargo.toml", "src/lib.rs", "tests/tier_boundary.rs"];
const TARGET: &str = "tests/tier_boundary.rs";

pub(crate) fn observe(candidate: &InstalledCandidate, parent: &Path) -> Result<Value, String> {
    let result = (|| {
        candidate.revalidate()?;
        fs::create_dir(parent).map_err(|e| format!("exclusive Rust pre-edit root: {e}"))?;
        let execution = CandidateExecution::Qualified(candidate);
        let root = parent.join("selected é");
        let history = prepare_history(candidate, &root)?;
        let foreign = parent.join("foreign decoy");
        fs::create_dir(&foreign).map_err(|e| e.to_string())?;
        execution.fixture_git(
            &foreign,
            &["init", "--quiet", "--template=", "--initial-branch=main"],
            "initialize Rust decoy",
        )?;
        fs::write(
            foreign.join("Cargo.toml"),
            "[package]\nname='decoy'\nversion='0.1.0'\n",
        )
        .map_err(|e| e.to_string())?;
        let before_decoy = inventory(&foreign)?;
        let observed = public_steps(&root, &foreign, parent, |args, context| {
            execution.run(args, &foreign, context)
        })?;
        if inventory(&foreign)? != before_decoy {
            return Err("Rust public route wrote into the foreign decoy".to_string());
        }
        candidate.revalidate()?;
        Ok(
            json!({"schema_version":"installed_rust_pre_edit_preparation.v1", "scope":"preparation_only",
            "history":history,"public_route":observed,"candidate_custody":candidate.custody_json(),
            "decoy_unchanged":true,"repair_edit":"not_run","project_verification":"not_run",
            "static_movement":"not_run","receipt":"not_run","installed_acceptance":"not_established",
            "blind_acceptance":"not_established"}),
        )
    })();
    result.map_err(|error| {
        format!(
            "{error}; Rust pre-edit intended evidence destination: {}",
            parent.display()
        )
    })
}

pub(crate) fn prepare_history(
    candidate: &InstalledCandidate,
    root: &Path,
) -> Result<Value, String> {
    let execution = CandidateExecution::Qualified(candidate);
    history_with(
        root,
        &candidate.rust_journey_manifest()?,
        |snapshot, file| candidate.rust_journey_snapshot_file(snapshot, file),
        |args, date| match date {
            Some(date) => execution.fixture_git_at(root, args, date, "pinned Rust history proof"),
            None => execution.fixture_git(root, args, "pinned Rust history proof"),
        },
    )
}

fn history_with(
    root: &Path,
    manifest: &[u8],
    mut read: impl FnMut(&str, &str) -> Result<Vec<u8>, String>,
    mut git: impl FnMut(&[&str], Option<&str>) -> Result<CommandResult, String>,
) -> Result<Value, String> {
    let manifest: Value =
        serde_json::from_slice(manifest).map_err(|e| format!("Rust fixture manifest: {e}"))?;
    if manifest["schema_version"] != "blind_journey_installed_rust_fixture.v1" {
        return Err("unsupported Rust fixture manifest".to_string());
    }
    fs::create_dir(root).map_err(|e| format!("exclusive pinned Rust root: {e}"))?;
    for dir in ["src", "tests"] {
        fs::create_dir(root.join(dir)).map_err(|e| e.to_string())?;
    }
    git(
        &[
            "init",
            "--quiet",
            "--template=",
            "--initial-branch=main",
            "--object-format=sha1",
        ],
        None,
    )?;
    for (key, value) in [
        ("core.autocrlf", "false"),
        ("core.fsmonitor", "false"),
        ("core.hooksPath", ""),
        ("commit.gpgSign", "false"),
    ] {
        git(&["config", "--local", key, value], None)?;
    }
    let mut records = Vec::new();
    for (snapshot, date, message) in [
        (
            "base",
            "2026-10-01T00:00:00Z",
            "base: exclusive gold-tier boundary with below/above tests",
        ),
        (
            "head",
            "2026-10-01T00:01:00Z",
            "head: move gold-tier boundary to include exactly 20; ignore cargo build output",
        ),
    ] {
        let spec = &manifest["repository"]["snapshots"][snapshot];
        let mut paths = FILES.to_vec();
        if snapshot == "head" {
            paths.push(".gitignore");
        }
        if spec["files"].as_object().map(serde_json::Map::len) != Some(paths.len()) {
            return Err("Rust fixture inventory differs from supported snapshot".to_string());
        }
        for path in paths {
            let bytes = read(snapshot, path)?;
            if spec["files"][path].as_str() != Some(digest(&bytes).as_str()) {
                return Err(format!("Rust fixture digest mismatch: {snapshot}/{path}"));
            }
            fs::write(root.join(path), bytes).map_err(|e| e.to_string())?;
        }
        git(&["add", "--all"], None)?;
        git(
            &["commit", "--quiet", "--no-gpg-sign", "-m", message],
            Some(date),
        )?;
        let head = git(&["rev-parse", "HEAD"], None)?.stdout.trim().to_string();
        let tree = git(&["rev-parse", "HEAD^{tree}"], None)?
            .stdout
            .trim()
            .to_string();
        if spec["commit"].as_str() != Some(head.as_str())
            || spec["tree"].as_str() != Some(tree.as_str())
        {
            return Err(format!(
                "Rust {snapshot} Git identity differs from accepted fixture pins"
            ));
        }
        records.push(json!({"snapshot":snapshot,"commit":head,"tree":tree}));
    }
    Ok(
        json!({"snapshots":records,"pinned_no_ignore_commit":"not_reproduced",
        "limitation":"The retained no-ignore commit recipe omits its commit message and complete metadata; the deleted-ignore worktree control is not that pinned history."}),
    )
}

fn public_steps(
    root: &Path,
    foreign: &Path,
    evidence: &Path,
    mut run: impl FnMut(&[String], &str) -> Result<CommandResult, String>,
) -> Result<Value, String> {
    let root_text = root.to_str().ok_or("Rust fixture root is not UTF-8")?;
    let mut commands = Vec::new();
    let doctor = invoke(
        &mut run,
        evidence,
        &mut commands,
        vec![
            "doctor".into(),
            "--root".into(),
            root_text.into(),
            "--json".into(),
        ],
        "Rust history doctor",
    )?;
    super::rust_entrypoint::validate_doctor(&doctor.stdout, doctor.status, root_text)?;
    let pilot = invoke(
        &mut run,
        evidence,
        &mut commands,
        vec![
            "pilot".into(),
            "--root".into(),
            root_text.into(),
            "--format".into(),
            "json".into(),
        ],
        "Rust public pilot",
    )?;
    if !pilot.success {
        return Err("Rust pilot did not complete".into());
    }
    let summary: Value = serde_json::from_str(&pilot.stdout).map_err(|e| e.to_string())?;
    if summary["root"] != root_text
        || summary["status"] != "complete"
        || summary["actionable_seams_total"].as_u64() != Some(1)
    {
        return Err("Rust pilot did not select one complete root-bound subject".into());
    }
    let seams = summary["top_actionable_seams"]
        .as_array()
        .ok_or("Rust pilot subjects missing")?;
    if seams.len() != 1 {
        return Err("Rust pilot subject cardinality changed".into());
    }
    let seam = seams[0]["seam_id"]
        .as_str()
        .ok_or("Rust canonical seam ID missing")?;
    let packet_path = bound_emitted_path(
        summary["next"]["inspect_packet"]
            .as_str()
            .ok_or("Rust pilot packet route missing")?,
        root,
        "target/ripr/pilot/agent-seam-packets.json",
    )?;
    let packet = json_file(&packet_path)?;
    selected_packet(&packet, seam)?;
    let printed = summary["next"]["repair_command"]
        .as_str()
        .ok_or("Rust pilot repair command missing")?;
    let argv = repair_argv(printed, root_text, seam)?;
    let attempts = root.join("target/ripr/repair-attempts");
    if attempts.exists() {
        return Err("Rust pre-edit root already contains attempts".into());
    }
    let ignore = root.join(".gitignore");
    let ignore_bytes = fs::read(&ignore).map_err(|e| e.to_string())?;
    fs::remove_file(&ignore).map_err(|e| e.to_string())?;
    let negative = invoke(
        &mut run,
        evidence,
        &mut commands,
        argv.clone(),
        "Rust deleted-ignore refusal",
    );
    // Restore the sole deliberate fault even when the owned command fails.
    fs::write(&ignore, ignore_bytes).map_err(|e| e.to_string())?;
    let negative = negative?;
    let refused_store = inventory(&attempts)?;
    let expected_store = BTreeMap::from([(".before.lock".to_string(), digest(b""))]);
    if negative.status != Some(2)
        || !negative.stderr.contains("before the test edit")
        || !negative.stderr.contains("Nothing was started")
        || refused_store != expected_store
    {
        return Err(format!(
            "Rust no-ignore worktree did not refuse before starting an attempt: status={:?}, stderr={:?}, store={refused_store:?}",
            negative.status, negative.stderr
        ));
    }
    let before = invoke(
        &mut run,
        evidence,
        &mut commands,
        argv,
        "Rust emitted repair before",
    )?;
    if !before.success {
        return Err("Rust emitted repair-before command failed".into());
    }
    let literal_next = before
        .stderr
        .lines()
        .find_map(|line| line.strip_prefix("ripr: attempt next command: "))
        .ok_or("Rust before did not advertise its next command")?;
    let next = printed_words::printed_words(literal_next)?;
    if next.len() != 6
        || next[0] != "ripr"
        || next[1] != "continue"
        || next[2] != "--root"
        || next[3] != root_text
        || next[4] != "--attempt"
    {
        return Err("unsupported Rust printed continuation".into());
    }
    let id = &next[5];
    if id
        .strip_prefix("repair-attempt-")
        .is_none_or(|suffix| suffix.is_empty() || !suffix.bytes().all(|b| b.is_ascii_hexdigit()))
    {
        return Err("unsafe Rust attempt ID".into());
    }
    let advertised = before
        .stderr
        .lines()
        .find_map(|line| line.strip_prefix("ripr: attempt manifest: "))
        .ok_or("Rust before did not advertise its attempt manifest")?;
    let attempt_path = bound_emitted_path(
        advertised,
        root,
        &format!("target/ripr/repair-attempts/{id}/attempt.json"),
    )?;
    let attempt = json_file(&attempt_path)?;
    if attempt["state"] != "awaiting_edit"
        || attempt["root"] != root_text
        || attempt["seam_id"] != seam
        || attempt["repository_head"] != "18a45fa5cd718f85dc60fcb27495b24dbd9715e7"
    {
        return Err("Rust before attempt has wrong state/root/subject/history".into());
    }
    let artifacts = validate_attempt_artifacts(root, &attempt)?;
    selected_packet(
        &json_file(&root.join("target/ripr/workflow/agent-packet.json"))?,
        seam,
    )?;
    if !root
        .join("target/ripr/workflow/before.repo-exposure.json")
        .is_file()
    {
        return Err("Rust advertised before producer did not write its snapshot".into());
    }
    Ok(
        json!({"state":"awaiting_edit","seam_id":seam,"emitted_before":printed,"emitted_next":literal_next,
        "commands":commands,"producer_artifacts":artifacts,"attempt_manifest":artifact(&attempt_path)?,
        "no_ignore_control":{"scope":"deleted-ignore worktree, not pinned no-ignore commit","exit":negative.status,"state":"refused_before_edit","operational_writes":refused_store},
        "foreign_cwd":foreign,"test_edit":"not_run","project_verification":"not_run","static_movement":"not_run","receipt":"not_run"}),
    )
}

fn validate_attempt_artifacts(root: &Path, attempt: &Value) -> Result<Vec<Value>, String> {
    let mut artifacts = Vec::new();
    for row in attempt["artifacts"]
        .as_array()
        .ok_or("Rust before artifact inventory absent")?
    {
        let relative = row["path"].as_str().ok_or("Rust artifact path absent")?;
        if !Path::new(relative)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
        {
            return Err("unsafe Rust producer artifact path".into());
        }
        let bytes = plain_bytes(&root.join(relative))?;
        if row["bytes"].as_u64() != Some(bytes.len() as u64)
            || row["sha256"] != format!("sha256:{}", digest(&bytes))
        {
            return Err("Rust producer artifact digest/length differs".into());
        }
        artifacts.push(row.clone());
    }
    for role in [
        "before_snapshot",
        "agent_packet",
        "edit_cage_baseline",
        "workflow_manifest",
    ] {
        if artifacts.iter().filter(|row| row["role"] == role).count() != 1 {
            return Err(format!(
                "Rust before missing unique producer artifact {role}"
            ));
        }
    }
    Ok(artifacts)
}

fn invoke(
    run: &mut impl FnMut(&[String], &str) -> Result<CommandResult, String>,
    evidence: &Path,
    commands: &mut Vec<Value>,
    argv: Vec<String>,
    context: &str,
) -> Result<CommandResult, String> {
    let result = run(&argv, context)?;
    let record = json!({"argv":argv,"context":context,"exit":result.status,"stdout":result.stdout,"stderr":result.stderr});
    fs::write(
        evidence.join(format!("public-command-{}.json", commands.len())),
        serde_json::to_vec_pretty(&record).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    commands.push(record);
    Ok(result)
}
fn repair_argv(command: &str, root: &str, seam: &str) -> Result<Vec<String>, String> {
    let words = printed_words::printed_words(command)?;
    if words
        != [
            "ripr",
            "agent",
            "repair",
            "--root",
            root,
            "--seam-id",
            seam,
            "--phase",
            "before",
        ]
    {
        return Err("unsupported or misbound Rust emitted repair command".into());
    }
    Ok(words.into_iter().skip(1).collect())
}
fn selected_packet(value: &Value, seam: &str) -> Result<(), String> {
    let packets = value["packets"]
        .as_array()
        .ok_or("Rust packet subjects missing")?;
    if packets.len() != 1
        || packets[0]["seam_id"] != seam
        || packets[0]["allowed_edit_surface"] != json!([TARGET])
        || packets[0]["changed_expression"] != "quantity >= 20"
    {
        return Err("Rust selected packet is not the bounded fixture subject".into());
    }
    Ok(())
}
fn bound_emitted_path(
    path: &str,
    root: &Path,
    relative: &str,
) -> Result<std::path::PathBuf, String> {
    let actual = Path::new(path);
    let expected = root.join(relative);
    if actual != expected {
        return Err("Rust emitted artifact path escaped its selected producer route".into());
    }
    Ok(actual.to_path_buf())
}
fn plain_bytes(path: &Path) -> Result<Vec<u8>, String> {
    let meta = fs::symlink_metadata(path)
        .map_err(|e| format!("Rust producer artifact {}: {e}", path.display()))?;
    if !meta.is_file() || meta.len() > 16 * 1024 * 1024 {
        return Err("Rust producer artifact is not an ordinary bounded file".into());
    }
    fs::read(path).map_err(|e| e.to_string())
}
fn json_file(path: &Path) -> Result<Value, String> {
    serde_json::from_slice(&plain_bytes(path)?).map_err(|e| e.to_string())
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn artifact(path: &Path) -> Result<Value, String> {
    let bytes = plain_bytes(path)?;
    Ok(json!({"path":path,"bytes":bytes.len(),"sha256":digest(&bytes)}))
}
fn inventory(root: &Path) -> Result<BTreeMap<String, String>, String> {
    let mut result = BTreeMap::new();
    for entry in fs::read_dir(root).map_err(|e| e.to_string())? {
        let path = entry.map_err(|e| e.to_string())?.path();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or("non-UTF8 Rust decoy path")?
            .to_string();
        if fs::symlink_metadata(&path)
            .map_err(|e| e.to_string())?
            .is_dir()
        {
            for (child, hash) in inventory(&path)? {
                result.insert(format!("{name}/{child}"), hash);
            }
        } else {
            result.insert(name, digest(&plain_bytes(&path)?));
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    #[test]
    fn printed_before_refuses_wrong_root_subject_and_extra_operations() -> Result<(), String> {
        let good = "ripr agent repair --root 'root é' --seam-id item --phase before";
        repair_argv(good, "root é", "item")?;
        for command in [
            good.replace("root é", "decoy"),
            good.replace("item", "other"),
            format!("{good} ; echo forged"),
        ] {
            if repair_argv(&command, "root é", "item").is_ok() {
                return Err("misbound printed command accepted".into());
            }
        }
        Ok(())
    }

    #[test]
    fn workspace_real_rust_pre_edit_route_uses_pinned_history_and_product_artifacts()
    -> Result<(), String> {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let parent = std::env::temp_dir().join(format!(
            "rust-pre-edit-workspace-{}-{stamp}",
            std::process::id()
        ));
        fs::create_dir_all(&parent).map_err(|e| e.to_string())?;
        let result: Result<(), String> = (|| {
            let root = parent.join("selected é");
            let foreign = parent.join("foreign decoy");
            fs::create_dir(&foreign).map_err(|e| e.to_string())?;
            let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../fixtures/blind_journey_installed_rust");
            let git = |args: &[&str], date: Option<&str>| -> Result<CommandResult, String> {
                let args = args.iter().map(|s| s.to_string()).collect::<Vec<_>>();
                let mut env = vec![
                    ("GIT_AUTHOR_NAME", "ripr fixture"),
                    ("GIT_COMMITTER_NAME", "ripr fixture"),
                    ("GIT_AUTHOR_EMAIL", "fixture@example.invalid"),
                    ("GIT_COMMITTER_EMAIL", "fixture@example.invalid"),
                ];
                if let Some(d) = date {
                    env.extend([("GIT_AUTHOR_DATE", d), ("GIT_COMMITTER_DATE", d)]);
                }
                capture(Path::new("git"), &args, &root, &env)
            };
            let history = history_with(
                &root,
                &fs::read(fixture.join("manifest.json")).map_err(|e| e.to_string())?,
                |s, p| {
                    fs::read(fixture.join("repository").join(s).join(p)).map_err(|e| e.to_string())
                },
                git,
            )?;
            if history["snapshots"].as_array().map(Vec::len) != Some(2) {
                return Err("pinned history subjects lost".into());
            }
            capture(
                Path::new("git"),
                &["init".into(), "--quiet".into(), "--template=".into()],
                &foreign,
                &[],
            )?;
            fs::write(
                foreign.join("Cargo.toml"),
                "[package]\nname='decoy'\nversion='0.1.0'\n",
            )
            .map_err(|e| e.to_string())?;
            let decoy = inventory(&foreign)?;
            let binary = std::path::PathBuf::from(crate::tests::ripr_worktree_binary()?);
            let observed = public_steps(&root, &foreign, &parent, |args, _| {
                capture(&binary, args, &foreign, &[])
            })?;
            if inventory(&foreign)? != decoy
                || observed["state"] != "awaiting_edit"
                || observed["no_ignore_control"]["state"] != "refused_before_edit"
            {
                return Err("workspace pre-edit protocol failed its controls".into());
            }
            let attempt = json_file(Path::new(
                observed["attempt_manifest"]["path"]
                    .as_str()
                    .ok_or("missing observed manifest path")?,
            ))?;
            let snapshot = attempt["artifacts"]
                .as_array()
                .ok_or("missing observed artifacts")?
                .iter()
                .find(|row| row["role"] == "before_snapshot")
                .and_then(|row| row["path"].as_str())
                .ok_or("missing before snapshot path")?;
            let snapshot = root.join(snapshot);
            let original = plain_bytes(&snapshot)?;
            fs::remove_file(&snapshot).map_err(|e| e.to_string())?;
            let removed = validate_attempt_artifacts(&root, &attempt);
            fs::write(&snapshot, &original).map_err(|e| e.to_string())?;
            if !removed.is_err_and(|error| error.contains("Rust producer artifact")) {
                return Err("removed producer snapshot did not refuse at its consumer".into());
            }
            let mut changed = original.clone();
            changed.push(b' ');
            fs::write(&snapshot, changed).map_err(|e| e.to_string())?;
            let corrupted = validate_attempt_artifacts(&root, &attempt);
            fs::write(&snapshot, original).map_err(|e| e.to_string())?;
            if !corrupted.is_err_and(|error| error.contains("digest/length differs")) {
                return Err("changed producer snapshot did not refuse at its consumer".into());
            }
            validate_attempt_artifacts(&root, &attempt)?;
            eprintln!(
                "real producer-artifact controls: removed snapshot refused; changed snapshot refused; original restored and revalidated"
            );
            eprintln!(
                "workspace-only Rust pre-edit proof: {}",
                json!({"binary":binary,"history":history,"observation":observed})
            );
            Ok(())
        })();
        match result {
            Err(error) => Err(format!(
                "{error}; failed workspace proof retained at {}",
                parent.display()
            )),
            Ok(()) if std::env::var("RIPR_4510_RETAIN_PROOF_FIXTURES").as_deref() == Ok("1") => {
                eprintln!(
                    "workspace Rust pre-edit proof retained at {}",
                    parent.display()
                );
                Ok(())
            }
            Ok(()) => fs::remove_dir_all(&parent).map_err(|e| e.to_string()),
        }
    }

    fn capture(
        program: &Path,
        args: &[String],
        cwd: &Path,
        env: &[(&str, &str)],
    ) -> Result<CommandResult, String> {
        let output = crate::run::capture_bytes_in_dir_with_timeout(
            program,
            args,
            cwd,
            env,
            &["GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE"],
            Duration::from_mins(2),
            "workspace-only Rust pre-edit protocol",
        )?;
        if output.timed_out {
            return Err("workspace Rust pre-edit process timed out".into());
        }
        let status = output
            .status
            .ok_or("workspace Rust process status missing")?;
        Ok(CommandResult {
            status: status.code(),
            success: status.success(),
            stdout: String::from_utf8(output.stdout).map_err(|e| e.to_string())?,
            stderr: String::from_utf8(output.stderr).map_err(|e| e.to_string())?,
        })
    }
}
