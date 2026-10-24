//! `cargo xtask fixtures refresh-identity <typescript|python>` (#9261).
//!
//! A dependency bump to an installed-language fixture (a Dependabot security
//! update, or a deliberate runner move) changes snapshot bytes. The
//! `check-fixture-contracts` bindings then refuse the change until every file
//! digest, snapshot commit/tree id and scripted-corpus candidate identity is
//! re-derived. This command re-derives them with the same code the gate checks
//! with, so the refresh and the verification cannot disagree:
//!
//! - digests come from the bytes on disk;
//! - commit and tree ids come from `installed_journey_rebuilt_ids`, the
//!   manifest's own `construction_recipe` rebuild;
//! - the same ids are rebound in `fixtures/blind_journey_execute/corpus.json`.
//!
//! The command only rewrites values that already exist, in place, so manifest
//! formatting and key order are preserved. It refuses to add or drop a bound
//! file: that is a deliberate fixture change, not a refresh. It does not
//! re-run the frozen journeys; the version words it cannot decide (lockfile
//! notes, SPEC wording, transcripts) are listed for a human instead of being
//! rewritten.

use super::*;

const CORPUS_PATH: &str = "fixtures/blind_journey_execute/corpus.json";

/// One resolved edit: a quoted `old` value replaced by `new` inside a named
/// snapshot's object in the manifest text.
struct SnapshotEdit {
    snapshot: String,
    /// JSON key whose string value changes (`commit`, `tree` or a file path).
    key: String,
    old: String,
    new: String,
}

pub(crate) fn fixtures_refresh_identity(args: &[String]) -> Result<(), String> {
    let [label] = args else {
        return Err("usage: cargo xtask fixtures refresh-identity <typescript|python>".to_string());
    };
    let contract = match label.as_str() {
        "typescript" => &INSTALLED_TYPESCRIPT_CONTRACT,
        "python" => &INSTALLED_PYTHON_CONTRACT,
        other => {
            return Err(format!(
                "fixtures refresh-identity supports typescript and python (their manifests carry \
                 a construction_recipe); `{other}` is not one"
            ));
        }
    };
    for line in refresh_installed_journey(
        contract,
        Path::new(contract.fixture_root),
        Path::new(CORPUS_PATH),
    )? {
        println!("{line}");
    }
    Ok(())
}

/// Re-derive the bindings of the fixture at `root` and the scripted corpus at
/// `corpus_path`, returning the report lines.
fn refresh_installed_journey(
    contract: &InstalledJourneyFixtureContract,
    root: &Path,
    corpus_path: &Path,
) -> Result<Vec<String>, String> {
    let mut report = Vec::new();
    let manifest_path = root.join("manifest.json");
    let manifest_text = read_text_lossy(&manifest_path)?;
    let corpus_text = read_text_lossy(corpus_path)?;
    let manifest: Value = serde_json::from_str(&manifest_text).map_err(|error| {
        format!(
            "{} is not valid JSON: {error}",
            normalize_path(&manifest_path)
        )
    })?;

    let mut edits = digest_edits(contract, root, &manifest)?;
    let mut new_manifest = apply_snapshot_edits(&manifest_text, &edits)?;
    // The rebuild reads the snapshot bytes on disk, so it does not depend on the
    // digests just rewritten; ids are compared with what the manifest records.
    let rebuilt = installed_journey_rebuilt_ids(contract, root, &manifest)?;
    let mut id_edits = Vec::new();
    for (name, commit, tree) in rebuilt {
        for (key, new) in [("commit", commit), ("tree", tree)] {
            let old = snapshot_field(&manifest, &name, key)?;
            if old != new {
                id_edits.push(SnapshotEdit {
                    snapshot: name.clone(),
                    key: key.to_string(),
                    old,
                    new,
                });
            }
        }
    }
    reject_id_collisions(&id_edits)?;
    new_manifest = apply_snapshot_edits(&new_manifest, &id_edits)?;
    let mut new_corpus = corpus_text.clone();
    for edit in &id_edits {
        new_corpus = new_corpus.replace(&format!("\"{}\"", edit.old), &format!("\"{}\"", edit.new));
    }
    let mut runner_note = None;
    let mut stale_mentions = Vec::new();
    if contract.label == "typescript"
        && let Some((rewritten, old, new)) = rebind_vitest_runner(root, &new_manifest, &manifest)?
    {
        new_manifest = rewritten;
        stale_mentions = stale_version_mentions(root, &old)?;
        runner_note = Some((old, new));
    }

    if new_manifest == manifest_text && new_corpus == corpus_text {
        report.push(format!(
            "fixtures refresh-identity {}: digests and ids already current",
            contract.label
        ));
        return Ok(report);
    }
    // Corpus first: an interruption between the writes leaves the manifest
    // stale, so a rerun re-derives the same ids and finishes the job. The other
    // order would leave a refreshed manifest and report "already current".
    std::fs::write(corpus_path, &new_corpus)
        .map_err(|error| format!("write {}: {error}", normalize_path(corpus_path)))?;
    std::fs::write(&manifest_path, &new_manifest)
        .map_err(|error| format!("write {}: {error}", normalize_path(&manifest_path)))?;
    edits.extend(id_edits);
    report.push(format!(
        "fixtures refresh-identity {}: rebound {} manifest value(s)",
        contract.label,
        edits.len()
    ));
    for edit in &edits {
        report.push(format!(
            "  {} {}: {} -> {}",
            edit.snapshot, edit.key, edit.old, edit.new
        ));
    }
    if let Some((old, new)) = &runner_note {
        report.push(format!("  runner_identity: vitest {old} -> vitest {new}"));
        for line in &stale_mentions {
            report.push(format!(
                "  still names vitest {old} (review by hand): {line}"
            ));
        }
    }
    report.push(
        "next: run `cargo xtask check-fixture-contracts` and `cargo xtask blind-journey-execute`, \
         re-run the fixture journeys under the new runner, then bless if they pass"
            .to_string(),
    );
    Ok(report)
}

fn snapshot_field(manifest: &Value, snapshot: &str, key: &str) -> Result<String, String> {
    manifest
        .get("repository")
        .and_then(|repository| repository.get("snapshots"))
        .and_then(|snapshots| snapshots.get(snapshot))
        .and_then(|snapshot| snapshot.get(key))
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| format!("manifest snapshot `{snapshot}` records no `{key}` string"))
}

/// Digest edits for every bound file whose bytes changed. A file on disk that
/// the manifest does not bind, or a bound file that is gone, is a deliberate
/// fixture change and is refused rather than inserted or dropped here.
fn digest_edits(
    contract: &InstalledJourneyFixtureContract,
    root: &Path,
    manifest: &Value,
) -> Result<Vec<SnapshotEdit>, String> {
    let snapshots = manifest
        .get("repository")
        .and_then(|repository| repository.get("snapshots"))
        .and_then(Value::as_object)
        .ok_or("manifest is missing repository.snapshots")?;
    let mut edits = Vec::new();
    for (name, snapshot) in snapshots {
        let dir = root.join("repository").join(name);
        let bound = snapshot
            .get("files")
            .and_then(Value::as_object)
            .ok_or_else(|| format!("snapshot `{name}` records no files"))?;
        let mut on_disk = Vec::new();
        let mut problems = Vec::new();
        installed_journey_collect_snapshot_files(
            contract,
            &dir,
            Path::new(""),
            &mut on_disk,
            &mut problems,
            name,
        );
        if let Some(problem) = problems.first() {
            return Err(problem.clone());
        }
        if let Some(extra) = on_disk
            .iter()
            .find(|path| !bound.contains_key(path.as_str()))
        {
            return Err(format!(
                "snapshot `{name}` has file `{extra}` the manifest does not bind; adding a file \
                 is a fixture change, not an identity refresh"
            ));
        }
        for (relative, recorded) in bound {
            let path = dir.join(relative);
            let body = std::fs::read(&path).map_err(|error| {
                format!(
                    "snapshot `{name}` bound file `{relative}` cannot be read ({error}); dropping \
                     a file is a fixture change, not an identity refresh"
                )
            })?;
            let current = installed_journey_sha256_hex(&body);
            let recorded = recorded.as_str().ok_or_else(|| {
                format!("snapshot `{name}` file `{relative}` has a non-string digest")
            })?;
            if current != recorded {
                edits.push(SnapshotEdit {
                    snapshot: name.clone(),
                    key: relative.clone(),
                    old: recorded.to_string(),
                    new: current,
                });
            }
        }
    }
    Ok(edits)
}

/// Corpus rebinding replaces ids globally, so two edits where one's new id is
/// another's old id would chain. Git ids make that astronomically unlikely;
/// refusing keeps the replacement exact instead of assuming it.
fn reject_id_collisions(edits: &[SnapshotEdit]) -> Result<(), String> {
    for edit in edits {
        if edits.iter().any(|other| other.old == edit.new) {
            return Err(format!(
                "refresh would chain id `{}` through another snapshot; refusing an ambiguous \
                 rebinding",
                edit.new
            ));
        }
    }
    Ok(())
}

/// Byte span of the `{...}` value that follows the first `"key"` at or after
/// `from`, honoring JSON strings so braces inside file names cannot unbalance it.
fn object_span(text: &str, key: &str, from: usize) -> Option<(usize, usize)> {
    let needle = format!("\"{key}\"");
    let key_at = from + text.get(from..)?.find(&needle)?;
    let open = key_at + text.get(key_at..)?.find('{')?;
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for (offset, byte) in text.as_bytes().iter().enumerate().skip(open) {
        if in_string {
            match (escaped, byte) {
                (true, _) => escaped = false,
                (false, b'\\') => escaped = true,
                (false, b'"') => in_string = false,
                _ => {}
            }
            continue;
        }
        match byte {
            b'"' => in_string = true,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some((open, offset + 1));
                }
            }
            _ => {}
        }
    }
    None
}

/// Replace each edit's quoted value inside its own snapshot object, so a digest
/// shared by two snapshots changes only where its file changed.
fn apply_snapshot_edits(text: &str, edits: &[SnapshotEdit]) -> Result<String, String> {
    let mut text = text.to_string();
    for edit in edits {
        let (snapshots_start, _) = object_span(&text, "snapshots", 0)
            .ok_or("manifest text has no `snapshots` object to edit")?;
        let (start, end) = object_span(&text, &edit.snapshot, snapshots_start)
            .ok_or_else(|| format!("manifest text has no `{}` snapshot object", edit.snapshot))?;
        let region = &text[start..end];
        let old = format!("\"{}\": \"{}\"", edit.key, edit.old);
        let new = format!("\"{}\": \"{}\"", edit.key, edit.new);
        if region.matches(&old).count() != 1 {
            return Err(format!(
                "snapshot `{}` does not contain exactly one `{}` entry in the expected \
                 `\"key\": \"value\"` layout; refresh needs the pretty-printed manifest form",
                edit.snapshot, edit.key
            ));
        }
        let replaced = region.replacen(&old, &new, 1);
        text.replace_range(start..end, &replaced);
    }
    Ok(text)
}

/// The vitest version the head lockfile selects, written back into
/// `environment.runner_identity` when it moved. Returns the rewritten text and
/// the old and new versions.
fn rebind_vitest_runner(
    root: &Path,
    text: &str,
    manifest: &Value,
) -> Result<Option<(String, String, String)>, String> {
    let lock_path = root.join("repository/head/package-lock.json");
    let lock: Value = serde_json::from_str(&read_text_lossy(&lock_path)?)
        .map_err(|error| format!("{} is not valid JSON: {error}", normalize_path(&lock_path)))?;
    let Some(locked) = lock
        .get("packages")
        .and_then(|packages| packages.get("node_modules/vitest"))
        .and_then(|package| package.get("version"))
        .and_then(Value::as_str)
    else {
        return Ok(None);
    };
    let identity = manifest
        .get("environment")
        .and_then(|environment| environment.get("runner_identity"))
        .and_then(Value::as_str)
        .ok_or("manifest records no environment.runner_identity")?;
    let Some(recorded) = identity
        .strip_prefix("vitest ")
        .and_then(|rest| rest.split_whitespace().next())
    else {
        return Ok(None);
    };
    if recorded == locked {
        return Ok(None);
    }
    let old = format!("\"runner_identity\": \"{identity}\"");
    let new = format!(
        "\"runner_identity\": \"{}\"",
        identity.replacen(recorded, locked, 1)
    );
    if text.matches(&old).count() != 1 {
        return Err("manifest runner_identity is not in the expected layout".to_string());
    }
    Ok(Some((
        text.replacen(&old, &new, 1),
        recorded.to_string(),
        locked.to_string(),
    )))
}

/// Lines in the fixture's own manifest and SPEC that still name the old runner
/// version. They are prose a human has to judge (some are dated observations).
fn stale_version_mentions(root: &Path, old: &str) -> Result<Vec<String>, String> {
    let mut found = Vec::new();
    for name in ["manifest.json", "SPEC.md"] {
        let path = root.join(name);
        for (index, line) in read_text_lossy(&path)?.lines().enumerate() {
            if line.contains(&format!("vitest {old}")) {
                found.push(format!("{}:{}", normalize_path(&path), index + 1));
            }
        }
    }
    Ok(found)
}

#[cfg(test)]
mod installed_journey_refresh_tests {
    use super::*;

    const MANIFEST: &str = r#"{
 "repository": {
  "snapshots": {
   "base": {
    "commit": "aaaa",
    "files": {
     "package.json": "d1"
    }
   },
   "head": {
    "commit": "bbbb",
    "files": {
     "package.json": "d1"
    }
   }
  }
 }
}
"#;

    fn edit(snapshot: &str, key: &str, old: &str, new: &str) -> SnapshotEdit {
        SnapshotEdit {
            snapshot: snapshot.to_string(),
            key: key.to_string(),
            old: old.to_string(),
            new: new.to_string(),
        }
    }

    #[test]
    fn a_shared_digest_changes_only_in_the_snapshot_whose_file_changed() -> Result<(), String> {
        let edited = apply_snapshot_edits(MANIFEST, &[edit("head", "package.json", "d1", "d2")])?;
        assert!(edited.contains("\"head\": {\n    \"commit\": \"bbbb\",\n    \"files\": {\n     \"package.json\": \"d2\""));
        assert!(edited.contains("\"base\": {\n    \"commit\": \"aaaa\",\n    \"files\": {\n     \"package.json\": \"d1\""));
        Ok(())
    }

    #[test]
    fn an_edit_whose_old_value_is_not_recorded_is_refused() {
        let expected = "snapshot `base` does not contain exactly one `package.json` entry in the expected \
                       `\"key\": \"value\"` layout; refresh needs the pretty-printed manifest form";
        let error = apply_snapshot_edits(MANIFEST, &[edit("base", "package.json", "zz", "d2")]);
        assert_eq!(error.map(|_| ()), Err(expected.to_string()));
    }

    #[test]
    fn a_missing_snapshot_is_refused() {
        let expected = "manifest text has no `nope` snapshot object";
        let error = apply_snapshot_edits(MANIFEST, &[edit("nope", "commit", "aaaa", "cccc")]);
        assert_eq!(error.map(|_| ()), Err(expected.to_string()));
    }

    #[test]
    fn a_chained_id_rebinding_is_refused() {
        let edits = [
            edit("base", "commit", "aaaa", "bbbb"),
            edit("head", "commit", "bbbb", "cccc"),
        ];
        assert_eq!(
            reject_id_collisions(&edits),
            Err(
                "refresh would chain id `bbbb` through another snapshot; refusing an ambiguous \
                 rebinding"
                    .to_string()
            )
        );
        assert_eq!(reject_id_collisions(&edits[..1]), Ok(()));
    }

    #[test]
    fn braces_inside_a_file_name_do_not_unbalance_the_span() {
        let text = r#"{"snapshots": {"base": {"files": {"a{b.txt": "d"}}, "head": {}}}"#;
        let (start, end) = object_span(text, "base", 0).unwrap_or((0, 0));
        assert_eq!(&text[start..end], r#"{"files": {"a{b.txt": "d"}}"#);
    }

    /// Scratch copy of one committed fixture plus the scripted corpus.
    fn scratch(
        contract: &InstalledJourneyFixtureContract,
        test: &str,
    ) -> Result<(PathBuf, PathBuf), String> {
        let base = std::env::temp_dir().join(format!(
            "ripr-refresh-identity-{}-{}-{test}",
            std::process::id(),
            contract.label
        ));
        if base.exists() {
            std::fs::remove_dir_all(&base).map_err(|error| format!("clear scratch: {error}"))?;
        }
        let root = base.join("fixture");
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        copy_tree(&manifest_dir.join(contract.fixture_root), &root)?;
        let corpus = base.join("corpus.json");
        std::fs::copy(manifest_dir.join(CORPUS_PATH), &corpus)
            .map_err(|error| format!("copy corpus: {error}"))?;
        Ok((root, corpus))
    }

    fn violations(contract: &InstalledJourneyFixtureContract, root: &Path) -> Vec<String> {
        let Ok(text) = std::fs::read_to_string(root.join("manifest.json")) else {
            return vec!["manifest unreadable".to_string()];
        };
        let Ok(manifest) = serde_json::from_str::<Value>(&text) else {
            return vec!["manifest invalid".to_string()];
        };
        let mut found = installed_journey_manifest_violations(contract, root, &manifest);
        found.extend(installed_journey_recipe_rebuild_violations(
            contract, root, &manifest,
        ));
        found
    }

    #[test]
    fn a_bumped_snapshot_is_refused_until_refreshed_and_then_passes() -> Result<(), String> {
        let contract = &INSTALLED_TYPESCRIPT_CONTRACT;
        let (root, corpus) = scratch(contract, "bump")?;
        let head_file = root.join("repository/head/src/pricing.ts");
        let mut body = std::fs::read_to_string(&head_file).map_err(|error| error.to_string())?;
        body.push_str("// bumped\n");
        std::fs::write(&head_file, body).map_err(|error| error.to_string())?;
        assert!(
            !violations(contract, &root).is_empty(),
            "a changed snapshot byte must trip the bindings before the refresh"
        );
        let before = std::fs::read_to_string(&corpus).map_err(|error| error.to_string())?;

        let report = refresh_installed_journey(contract, &root, &corpus)?;
        assert!(
            report
                .iter()
                .any(|line| line.contains("head src/pricing.ts"))
        );
        assert_eq!(violations(contract, &root), Vec::<String>::new());

        let after = std::fs::read_to_string(&corpus).map_err(|error| error.to_string())?;
        assert_ne!(before, after, "the corpus must be rebound to the new head");
        let head_commit = {
            let manifest: Value = serde_json::from_str(
                &std::fs::read_to_string(root.join("manifest.json"))
                    .map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?;
            snapshot_field(&manifest, "head", "commit")?
        };
        assert!(after.contains(&head_commit));

        let again = refresh_installed_journey(contract, &root, &corpus)?;
        assert!(
            again[0].contains("already current"),
            "refresh is idempotent"
        );
        Ok(())
    }

    #[test]
    fn an_unbound_new_file_is_refused_not_inserted() -> Result<(), String> {
        let contract = &INSTALLED_TYPESCRIPT_CONTRACT;
        let (root, corpus) = scratch(contract, "unbound")?;
        std::fs::write(root.join("repository/head/extra.txt"), "new")
            .map_err(|error| error.to_string())?;
        let error = refresh_installed_journey(contract, &root, &corpus)
            .err()
            .ok_or("an unbound file must be refused")?;
        assert!(error.contains("extra.txt"), "{error}");
        Ok(())
    }

    #[test]
    fn a_moved_vitest_lock_rebinds_the_runner_identity() -> Result<(), String> {
        let contract = &INSTALLED_TYPESCRIPT_CONTRACT;
        let (root, corpus) = scratch(contract, "runner")?;
        let lock = root.join("repository/head/package-lock.json");
        let text = std::fs::read_to_string(&lock).map_err(|error| error.to_string())?;
        let manifest: Value = serde_json::from_str(
            &std::fs::read_to_string(root.join("manifest.json")).map_err(|e| e.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        let recorded = manifest["environment"]["runner_identity"]
            .as_str()
            .and_then(|identity| identity.strip_prefix("vitest "))
            .and_then(|rest| rest.split_whitespace().next())
            .ok_or("runner_identity does not name vitest")?
            .to_string();
        let moved = text.replacen(
            &format!("\"node_modules/vitest\": {{\n      \"version\": \"{recorded}\""),
            "\"node_modules/vitest\": {\n      \"version\": \"9.9.9\"",
            1,
        );
        assert_ne!(
            text, moved,
            "the lockfile layout changed; update this fixture edit"
        );
        std::fs::write(&lock, moved).map_err(|error| error.to_string())?;
        let report = refresh_installed_journey(contract, &root, &corpus)?;
        assert!(report.iter().any(|line| line.contains("-> vitest 9.9.9")));
        let rewritten =
            std::fs::read_to_string(root.join("manifest.json")).map_err(|e| e.to_string())?;
        assert!(rewritten.contains("\"runner_identity\": \"vitest 9.9.9 via"));
        Ok(())
    }

    #[test]
    fn a_bumped_python_snapshot_is_refreshed_to_a_passing_state() -> Result<(), String> {
        let contract = &INSTALLED_PYTHON_CONTRACT;
        let (root, corpus) = scratch(contract, "python")?;
        let manifest: Value = serde_json::from_str(
            &std::fs::read_to_string(root.join("manifest.json")).map_err(|e| e.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        let file = manifest["repository"]["snapshots"]["head"]["files"]
            .as_object()
            .and_then(|files| files.keys().find(|name| name.ends_with(".py")))
            .ok_or("python head snapshot binds no .py file")?
            .clone();
        let path = root.join("repository/head").join(&file);
        let mut body = std::fs::read_to_string(&path).map_err(|error| error.to_string())?;
        body.push_str("# bumped\n");
        std::fs::write(&path, body).map_err(|error| error.to_string())?;
        assert!(!violations(contract, &root).is_empty());
        refresh_installed_journey(contract, &root, &corpus)?;
        assert_eq!(violations(contract, &root), Vec::<String>::new());
        Ok(())
    }
}
