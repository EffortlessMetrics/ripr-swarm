//! Rebuild-from-recipe check for the installed-language fixture manifests
//! (#9235). `installed_journey_git_identity_wellformed` only proves a recorded
//! commit or tree id looks like a git object id; this module rebuilds every
//! snapshot repository from `repository/<snapshot>/` with the manifest's
//! `construction_recipe` and refuses any recorded id the recipe does not
//! reproduce, so a hand-edited or stale id cannot pass `check-fixture-contracts`.
//!
//! The rebuild is offline and bounded: one `git init` per fixture in a scratch
//! directory, with the user and system git configuration disabled so a signing
//! key, hook or line-ending setting on the host cannot change the object ids.

use super::*;
use serde::de::{Deserializer, MapAccess, Visitor};
use std::fmt;

/// Snapshot names in the order the manifest lists them. `serde_json::Value`
/// objects sort their keys, but the recipe numbers variant commits by listed
/// order, so the order is read from the manifest text itself.
struct OrderedSnapshotNames(Vec<String>);

impl<'de> serde::Deserialize<'de> for OrderedSnapshotNames {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct NamesVisitor;
        impl<'de> Visitor<'de> for NamesVisitor {
            type Value = OrderedSnapshotNames;
            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a snapshot object")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut names = Vec::new();
                while let Some((name, _)) = map.next_entry::<String, serde::de::IgnoredAny>()? {
                    names.push(name);
                }
                Ok(OrderedSnapshotNames(names))
            }
        }
        deserializer.deserialize_map(NamesVisitor)
    }
}

#[derive(serde::Deserialize)]
struct OrderedRepository {
    snapshots: OrderedSnapshotNames,
}

#[derive(serde::Deserialize)]
struct OrderedManifest {
    repository: OrderedRepository,
}

/// One recipe commit step: the pinned date and the commit message.
struct RecipeCommit {
    date: String,
    message: String,
}

fn recipe_commits(recipe: &[&str]) -> Vec<RecipeCommit> {
    recipe
        .iter()
        .filter_map(|step| {
            let date = step
                .split("GIT_AUTHOR_DATE=")
                .nth(1)?
                .split_whitespace()
                .next()?;
            let message = step.split("git commit -m '").nth(1)?.split('\'').next()?;
            Some(RecipeCommit {
                date: date.to_string(),
                message: message.to_string(),
            })
        })
        .collect()
}

/// Hermetic git invocation: no user or system configuration, no signing, no
/// hooks, so the object ids depend only on the recipe and the snapshot bytes.
fn recipe_git(dir: &Path, args: &[&str], dates: Option<&str>) -> Result<String, String> {
    recipe_git_inheriting(dir, args, dates, &[])
}

/// `recipe_git` with extra variables the child would inherit from its parent;
/// the regression test uses it to prove the repository selectors are stripped.
fn recipe_git_inheriting(
    dir: &Path,
    args: &[&str],
    dates: Option<&str>,
    inherited: &[(&str, &str)],
) -> Result<String, String> {
    let mut full: Vec<String> = [
        "-c",
        "commit.gpgsign=false",
        "-c",
        "tag.gpgsign=false",
        "-c",
        "core.autocrlf=false",
        "-c",
        "core.hooksPath=/dev/null",
        "-c",
        "core.excludesFile=/dev/null",
        "-c",
        "core.fileMode=true",
    ]
    .iter()
    .map(|arg| arg.to_string())
    .collect();
    full.extend(args.iter().map(|arg| arg.to_string()));
    // Host identity, repository-redirecting and ignore-file settings must not
    // reach the rebuild: an inherited `GIT_AUTHOR_*` changes every commit id,
    // `GIT_INDEX_FILE` (set inside git hooks) redirects the index, and the
    // default `$XDG_CONFIG_HOME/git/ignore` would drop snapshot files.
    let scratch_home = dir.to_string_lossy().into_owned();
    let mut envs = vec![
        ("HOME", scratch_home.as_str()),
        ("XDG_CONFIG_HOME", scratch_home.as_str()),
        ("GIT_CONFIG_GLOBAL", "/dev/null"),
        ("GIT_CONFIG_NOSYSTEM", "1"),
        ("GIT_TERMINAL_PROMPT", "0"),
    ];
    if let Some(date) = dates {
        envs.push(("GIT_AUTHOR_DATE", date));
        envs.push(("GIT_COMMITTER_DATE", date));
    }
    const INHERITED_GIT_ENV: [&str; 11] = [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
        "GIT_COMMON_DIR",
        "GIT_NAMESPACE",
        "GIT_AUTHOR_NAME",
        "GIT_AUTHOR_EMAIL",
        "GIT_COMMITTER_NAME",
        "GIT_COMMITTER_EMAIL",
    ];
    let output = crate::run::capture_process_output_in(
        "git",
        &full,
        Some(dir),
        inherited,
        &INHERITED_GIT_ENV,
        &envs,
    )
    .map_err(|error| error.message)?;
    Ok(String::from_utf8_lossy(&output).trim().to_string())
}

fn clear_worktree(dir: &Path) -> Result<(), String> {
    for entry in std::fs::read_dir(dir).map_err(|error| format!("read scratch dir: {error}"))? {
        let entry = entry.map_err(|error| format!("read scratch entry: {error}"))?;
        if entry.file_name() == ".git" {
            continue;
        }
        let path = entry.path();
        let removed = if path.is_dir() {
            std::fs::remove_dir_all(&path)
        } else {
            std::fs::remove_file(&path)
        };
        removed.map_err(|error| format!("clear {}: {error}", path.display()))?;
    }
    Ok(())
}

pub(crate) fn copy_tree(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::create_dir_all(to).map_err(|error| format!("create {}: {error}", to.display()))?;
    for entry in
        std::fs::read_dir(from).map_err(|error| format!("read {}: {error}", from.display()))?
    {
        let entry = entry.map_err(|error| format!("read {}: {error}", from.display()))?;
        if entry.file_name() == ".git" {
            return Err(format!(
                "snapshot path {} is git metadata, which would redirect the scratch repository",
                entry.path().display()
            ));
        }
        let target = to.join(entry.file_name());
        let kind = entry
            .file_type()
            .map_err(|error| format!("inspect {}: {error}", entry.path().display()))?;
        if kind.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else if kind.is_file() {
            std::fs::copy(entry.path(), &target)
                .map_err(|error| format!("copy {}: {error}", entry.path().display()))?;
        } else {
            return Err(format!("{} is not a regular file", entry.path().display()));
        }
    }
    Ok(())
}

fn commit_snapshot(
    scratch: &Path,
    snapshot_dir: &Path,
    commit: &RecipeCommit,
) -> Result<(String, String), String> {
    clear_worktree(scratch)?;
    copy_tree(snapshot_dir, scratch)?;
    recipe_git(scratch, &["add", "-A"], None)?;
    recipe_git(
        scratch,
        &["commit", "-q", "-m", &commit.message],
        Some(&commit.date),
    )?;
    Ok((
        recipe_git(scratch, &["rev-parse", "HEAD"], None)?,
        recipe_git(scratch, &["rev-parse", "HEAD^{tree}"], None)?,
    ))
}

struct ScratchRepository(PathBuf);

impl Drop for ScratchRepository {
    fn drop(&mut self) {
        if let Ok(()) = std::fs::remove_dir_all(&self.0) {}
    }
}

/// Rebuild the snapshot repository from the manifest recipe and return the
/// `(snapshot, commit, tree)` ids it reproduces. The error is a complete
/// violation message. Only fixtures whose recipe is the structured
/// base/head(/variant) form are rebuilt.
pub(crate) fn installed_journey_rebuilt_ids(
    contract: &InstalledJourneyFixtureContract,
    root: &Path,
    manifest: &Value,
) -> Result<Vec<(String, String, String)>, String> {
    let label = contract.label;
    let repository = manifest.get("repository");
    let recipe: Vec<&str> = repository
        .and_then(|repository| repository.get("construction_recipe"))
        .and_then(Value::as_array)
        .map(|steps| steps.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let commits = recipe_commits(&recipe);
    let [base_commit, head_commit] = commits.as_slice() else {
        return Err(format!(
            "blind journey installed {label} construction_recipe must name exactly the base and \
             head commit steps (date and message), found {}",
            commits.len()
        ));
    };
    if repository
        .and_then(|repository| repository.get("snapshots"))
        .and_then(Value::as_object)
        .is_none()
    {
        return Err(format!(
            "blind journey installed {label} manifest is missing repository.snapshots, so no \
             recorded id can be rebuilt from the recipe"
        ));
    }
    let manifest_text = match std::fs::read_to_string(root.join("manifest.json")) {
        Ok(text) => text,
        Err(error) => {
            return Err(format!(
                "blind journey installed {label} manifest cannot be re-read for the recipe \
                 rebuild: {error}"
            ));
        }
    };
    let ordered: OrderedManifest = match serde_json::from_str(&manifest_text) {
        Ok(ordered) => ordered,
        Err(error) => {
            return Err(format!(
                "blind journey installed {label} manifest snapshot order cannot be read: {error}"
            ));
        }
    };
    let variants: Vec<&str> = ordered
        .repository
        .snapshots
        .0
        .iter()
        .map(String::as_str)
        .filter(|name| *name != "base" && *name != "head")
        .collect();
    let base_day = base_commit.date.split('T').next().unwrap_or_default();
    if !variants.is_empty() {
        // The variant commits are rebuilt by the derivation documented in the
        // recipe's prose step; refuse a recipe that no longer states it.
        let states_derivation = recipe.iter().any(|step| {
            step.contains("branch variant/<name>")
                && step.contains("'variant: <name>'")
                && step.contains(&format!("{base_day}T00:0N:00Z"))
                && step.contains("increments by one")
                && step.contains("listed under `repository.snapshots`")
        });
        if !states_derivation {
            return Err(format!(
                "blind journey installed {label} construction_recipe must state the variant \
                 branch, message, date and snapshot-order derivation the rebuild applies to its \
                 {} variant snapshots",
                variants.len()
            ));
        }
    }

    let scratch = std::env::temp_dir().join(format!(
        "ripr-installed-{label}-recipe-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos())
            .unwrap_or_default()
    ));
    let rebuilt = (|| -> Result<Vec<(String, String, String)>, String> {
        std::fs::create_dir_all(&scratch)
            .map_err(|error| format!("create scratch repository: {error}"))?;
        recipe_git(&scratch, &["init", "-q", "-b", "main", "."], None)?;
        recipe_git(&scratch, &["config", "user.name", "ripr fixture"], None)?;
        recipe_git(
            &scratch,
            &["config", "user.email", "fixture@example.invalid"],
            None,
        )?;
        let snapshot_dir = |name: &str| root.join("repository").join(name);
        let mut rebuilt = Vec::new();
        let (commit, tree) = commit_snapshot(&scratch, &snapshot_dir("base"), base_commit)?;
        let base_id = commit.clone();
        rebuilt.push(("base".to_string(), commit, tree));
        let (commit, tree) = commit_snapshot(&scratch, &snapshot_dir("head"), head_commit)?;
        rebuilt.push(("head".to_string(), commit, tree));
        for (index, name) in variants.iter().enumerate() {
            let suffix = name.strip_prefix("variant-").ok_or_else(|| {
                format!("snapshot `{name}` is neither base, head nor a `variant-` snapshot")
            })?;
            recipe_git(
                &scratch,
                &[
                    "checkout",
                    "-q",
                    "-b",
                    &format!("variant/{suffix}"),
                    &base_id,
                ],
                None,
            )?;
            let variant = RecipeCommit {
                date: format!("{base_day}T00:{:02}:00Z", index + 2),
                message: format!("variant: {suffix}"),
            };
            let (commit, tree) = commit_snapshot(&scratch, &snapshot_dir(name), &variant)?;
            rebuilt.push((name.to_string(), commit, tree));
        }
        Ok(rebuilt)
    })();
    let _guard = ScratchRepository(scratch);
    let rebuilt = match rebuilt {
        Ok(rebuilt) => rebuilt,
        Err(error) => {
            return Err(format!(
                "blind journey installed {label} snapshot repository cannot be rebuilt from its \
                 construction_recipe: {error}"
            ));
        }
    };
    Ok(rebuilt)
}

/// Report every recorded commit or tree id the recipe does not reproduce. The
/// contract opts in with `rebuild_from_recipe`, so a prose-only recipe is never
/// silently skipped.
pub(crate) fn installed_journey_recipe_rebuild_violations(
    contract: &InstalledJourneyFixtureContract,
    root: &Path,
    manifest: &Value,
) -> Vec<String> {
    let label = contract.label;
    let rebuilt = match installed_journey_rebuilt_ids(contract, root, manifest) {
        Ok(rebuilt) => rebuilt,
        Err(violation) => return vec![violation],
    };
    let snapshots = manifest
        .get("repository")
        .and_then(|repository| repository.get("snapshots"))
        .and_then(Value::as_object);
    let mut violations = Vec::new();
    for (name, commit, tree) in rebuilt {
        let Some(snapshot) = snapshots.and_then(|snapshots| snapshots.get(&name)) else {
            continue;
        };
        for (kind, rebuilt_id) in [("commit", commit), ("tree", tree)] {
            let recorded = snapshot
                .get(kind)
                .and_then(Value::as_str)
                .unwrap_or_default();
            if recorded != rebuilt_id {
                violations.push(format!(
                    "blind journey installed {label} snapshot `{name}` records {kind} \
                     `{recorded}` but its construction_recipe reproduces `{rebuilt_id}`"
                ));
            }
        }
    }
    violations
}

#[cfg(test)]
mod installed_journey_recipe_tests {
    use super::*;

    fn fixture_root(contract: &InstalledJourneyFixtureContract) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join(contract.fixture_root)
    }

    fn committed_manifest(contract: &InstalledJourneyFixtureContract) -> Result<Value, String> {
        read_json_value(&fixture_root(contract).join("manifest.json"))
    }

    /// A scratch copy of one committed fixture whose manifest and snapshot
    /// bytes the negative experiments may tamper with.
    fn scratch_fixture(
        contract: &InstalledJourneyFixtureContract,
        label: &str,
    ) -> Result<PathBuf, String> {
        let root = std::env::temp_dir().join(format!(
            "ripr-recipe-rebuild-{label}-{}-{}",
            std::process::id(),
            contract.label
        ));
        if root.exists() {
            std::fs::remove_dir_all(&root)
                .map_err(|error| format!("clear scratch fixture: {error}"))?;
        }
        copy_tree(&fixture_root(contract), &root)?;
        Ok(root)
    }

    #[test]
    fn committed_fixtures_reproduce_their_recorded_ids_from_the_recipe() -> Result<(), String> {
        for contract in [&INSTALLED_PYTHON_CONTRACT, &INSTALLED_TYPESCRIPT_CONTRACT] {
            let manifest = committed_manifest(contract)?;
            let violations = installed_journey_recipe_rebuild_violations(
                contract,
                &fixture_root(contract),
                &manifest,
            );
            if !violations.is_empty() {
                return Err(format!(
                    "the committed {} fixture must reproduce from its recipe, got: {violations:?}",
                    contract.label
                ));
            }
        }
        Ok(())
    }

    #[test]
    fn a_tampered_recorded_commit_id_is_refused() -> Result<(), String> {
        let contract = &INSTALLED_PYTHON_CONTRACT;
        let root = scratch_fixture(contract, "commit")?;
        let _cleanup = ScratchRepository(root.clone());
        let manifest_path = root.join("manifest.json");
        let text = std::fs::read_to_string(&manifest_path)
            .map_err(|error| format!("read manifest: {error}"))?;
        let manifest = committed_manifest(contract)?;
        let recorded = manifest["repository"]["snapshots"]["head"]["commit"]
            .as_str()
            .ok_or("head commit missing")?
            .to_string();
        let forged = "0".repeat(40);
        std::fs::write(&manifest_path, text.replace(&recorded, &forged))
            .map_err(|error| format!("write manifest: {error}"))?;
        let tampered: Value = read_json_value(&manifest_path)?;
        let violations = installed_journey_recipe_rebuild_violations(contract, &root, &tampered);
        if !violations.iter().any(|violation| {
            violation.contains("snapshot `head` records commit")
                && violation.contains(&forged)
                && violation.contains(&recorded)
        }) {
            return Err(format!(
                "a well-formed but forged commit id must be refused, got: {violations:?}"
            ));
        }
        if violations
            .iter()
            .any(|violation| violation.contains("records tree"))
        {
            return Err(format!(
                "only the forged commit may be reported, got: {violations:?}"
            ));
        }
        Ok(())
    }

    #[test]
    fn a_tampered_snapshot_byte_is_refused_by_the_rebuilt_tree() -> Result<(), String> {
        let contract = &INSTALLED_TYPESCRIPT_CONTRACT;
        let root = scratch_fixture(contract, "byte")?;
        let _cleanup = ScratchRepository(root.clone());
        let source = root.join("repository/head/src/pricing.ts");
        let mut bytes = std::fs::read(&source).map_err(|error| format!("read source: {error}"))?;
        bytes.extend_from_slice(b"\n// tampered\n");
        std::fs::write(&source, bytes).map_err(|error| format!("write source: {error}"))?;
        let manifest = committed_manifest(contract)?;
        let violations = installed_journey_recipe_rebuild_violations(contract, &root, &manifest);
        for kind in ["commit", "tree"] {
            if !violations
                .iter()
                .any(|violation| violation.contains(&format!("snapshot `head` records {kind}")))
            {
                return Err(format!(
                    "a changed snapshot byte must change the rebuilt head {kind}, got: \
                     {violations:?}"
                ));
            }
        }
        if violations
            .iter()
            .any(|violation| violation.contains("snapshot `base`"))
        {
            return Err(format!(
                "the untouched base snapshot must still reproduce, got: {violations:?}"
            ));
        }
        Ok(())
    }

    #[test]
    fn a_recipe_without_the_base_and_head_steps_fails_closed() -> Result<(), String> {
        let contract = &INSTALLED_PYTHON_CONTRACT;
        let mut manifest = committed_manifest(contract)?;
        manifest["repository"]["construction_recipe"] = serde_json::json!(["git init -b main ."]);
        let violations = installed_journey_recipe_rebuild_violations(
            contract,
            &fixture_root(contract),
            &manifest,
        );
        if !violations
            .iter()
            .any(|violation| violation.contains("must name exactly the base and head commit steps"))
        {
            return Err(format!(
                "an unstructured recipe must not be silently skipped, got: {violations:?}"
            ));
        }
        Ok(())
    }

    #[test]
    fn inherited_repository_selectors_do_not_redirect_the_rebuild() -> Result<(), String> {
        let contract = &INSTALLED_PYTHON_CONTRACT;
        let root = scratch_fixture(contract, "selectors")?;
        let _cleanup = ScratchRepository(root.clone());
        let foreign = root.join("foreign");
        let scratch = root.join("scratch");
        for dir in [&foreign, &scratch] {
            std::fs::create_dir_all(dir).map_err(|error| format!("create {dir:?}: {error}"))?;
            recipe_git(dir, &["init", "-q", "-b", "main", "."], None)?;
        }
        let foreign_git = foreign.join(".git").to_string_lossy().into_owned();
        let seen = recipe_git_inheriting(
            &scratch,
            &["rev-parse", "--absolute-git-dir"],
            None,
            &[("GIT_DIR", foreign_git.as_str())],
        )?;
        if seen.contains("foreign") || !seen.ends_with("scratch/.git") {
            return Err(format!(
                "an inherited GIT_DIR must not redirect the rebuild, git used `{seen}`"
            ));
        }
        Ok(())
    }

    #[test]
    fn snapshot_git_metadata_is_refused() -> Result<(), String> {
        let contract = &INSTALLED_PYTHON_CONTRACT;
        let root = scratch_fixture(contract, "dotgit")?;
        let _cleanup = ScratchRepository(root.clone());
        std::fs::create_dir_all(root.join("repository/base/.git"))
            .map_err(|error| format!("plant .git: {error}"))?;
        let manifest = committed_manifest(contract)?;
        let violations = installed_journey_recipe_rebuild_violations(contract, &root, &manifest);
        if !violations
            .iter()
            .any(|violation| violation.contains("git metadata"))
        {
            return Err(format!(
                "a snapshot carrying .git must be refused, got: {violations:?}"
            ));
        }
        Ok(())
    }

    #[test]
    fn a_recipe_that_drops_the_variant_derivation_is_refused() -> Result<(), String> {
        let contract = &INSTALLED_TYPESCRIPT_CONTRACT;
        let mut manifest = committed_manifest(contract)?;
        let steps = manifest["repository"]["construction_recipe"]
            .as_array_mut()
            .ok_or("construction_recipe is not an array")?;
        steps.retain(|step| {
            !step
                .as_str()
                .is_some_and(|text| text.contains("variant/<name>"))
        });
        let violations = installed_journey_recipe_rebuild_violations(
            contract,
            &fixture_root(contract),
            &manifest,
        );
        if !violations
            .iter()
            .any(|violation| violation.contains("must state the variant"))
        {
            return Err(format!(
                "a recipe without the variant step must be refused, got: {violations:?}"
            ));
        }
        Ok(())
    }
}
