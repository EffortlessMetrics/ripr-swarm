//! Scripted repairs-disabled builds. The removal controls in a packet are only
//! as honest as the binaries behind them, so this builds the candidate source
//! and one patched copy per repair, and records the source commit, the exact
//! edits and every binary hash in a manifest the harness verifies.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{Value, json};

use super::controls::reported_version;
use super::model::Family;

const DEFAULT_OUT: &str = "target/ripr/termination-qualification/removal-builds";
const BUILD_DEADLINE: Duration = Duration::from_secs(60 * 60);
pub(super) const MANIFEST_SCHEMA: &str = "ripr.termination_removal_builds.v1";

struct Edit {
    file: &'static str,
    old: &'static str,
    new: &'static str,
}

struct Patch {
    id: &'static str,
    families: &'static [Family],
    edits: &'static [Edit],
}

const NAIVE_GLOB: &str = "
fn naive_segments(pattern: &[Vec<char>], path: &[Vec<char>]) -> bool {
    match pattern.split_first() {
        None => path.is_empty(),
        Some((first, rest)) if first.as_slice() == ['*', '*'] => {
            (0..=path.len()).any(|skip| naive_segments(rest, &path[skip..]))
        }
        Some((first, rest)) => path
            .split_first()
            .is_some_and(|(head, tail)| naive_chars(first, head) && naive_segments(rest, tail)),
    }
}

fn naive_chars(pattern: &[char], subject: &[char]) -> bool {
    match pattern.split_first() {
        None => subject.is_empty(),
        Some(('*', rest)) => (0..=subject.len()).any(|skip| naive_chars(rest, &subject[skip..])),
        Some(('?', rest)) => subject
            .split_first()
            .is_some_and(|(_, tail)| naive_chars(rest, tail)),
        Some((c, rest)) => subject
            .split_first()
            .is_some_and(|(head, tail)| head == c && naive_chars(rest, tail)),
    }
}
";

/// Each patch disables one repair and nothing else. The glob patch restores
/// the recursive matcher #4497 replaced, for both consumers' entry points.
const PATCHES: &[Patch] = &[
    Patch {
        id: "rust-nesting-refusal-off",
        families: &[Family::RustNesting],
        edits: &[Edit {
            file: "crates/ripr/src/analysis/syntax/nesting.rs",
            old: "pub(crate) fn rust_nesting_refusal(text: &str) -> Option<String> {\n",
            new: "pub(crate) fn rust_nesting_refusal(text: &str) -> Option<String> {\n    if text.len() < usize::MAX {\n        return None;\n    }\n",
        }],
    },
    Patch {
        id: "python-parse-budget-off",
        families: &[Family::PythonNesting],
        edits: &[
            Edit {
                file: "crates/ripr/src/analysis/language/python/parse_budget.rs",
                old: "const MAX_PYTHON_PARSE_NESTING_DEPTH: usize = 128;",
                new: "const MAX_PYTHON_PARSE_NESTING_DEPTH: usize = usize::MAX;",
            },
            Edit {
                file: "crates/ripr/src/analysis/language/python/parse_budget.rs",
                old: "const MAX_PYTHON_PARSE_TREE_DEPTH: usize = 256;",
                new: "const MAX_PYTHON_PARSE_TREE_DEPTH: usize = usize::MAX;",
            },
        ],
    },
    Patch {
        id: "path-glob-recursive-matcher",
        families: &[Family::PathGlobSuppression, Family::PathGlobGeneratedFile],
        edits: &[
            Edit {
                file: "crates/ripr/src/analysis/path_glob.rs",
                old: "    segments_match(&pattern_segments, &path_segments)\n}\n",
                new: "    naive_segments(&pattern_segments, &path_segments)\n}\n",
            },
            Edit {
                file: "crates/ripr/src/analysis/path_glob.rs",
                old: "    chars_match(&pattern, &segment)\n}\n",
                new: "    naive_chars(&pattern, &segment)\n}\n",
            },
            Edit {
                file: "crates/ripr/src/analysis/path_glob.rs",
                old: "fn normalized_segments(",
                new: "NAIVE\nfn normalized_segments(",
            },
        ],
    },
];

fn edit_text(edit: &Edit) -> String {
    edit.new.replace("NAIVE", NAIVE_GLOB)
}

fn edits_digest(patch: &Patch) -> String {
    let mut bytes = Vec::new();
    for edit in patch.edits {
        for part in [edit.file, edit.old, &edit_text(edit)] {
            bytes.extend_from_slice(part.as_bytes());
            bytes.push(0);
        }
    }
    crate::blind_journey::sha256_hex(&bytes)
}

pub(crate) fn run(args: &[String]) -> Result<(), String> {
    let mut source = PathBuf::from(".");
    let mut out = PathBuf::from(DEFAULT_OUT);
    let mut release = true;
    let mut index = 0;
    while index < args.len() {
        let value = args.get(index + 1);
        match (args[index].as_str(), value) {
            ("--source", Some(value)) => source = PathBuf::from(value),
            ("--out", Some(value)) => out = PathBuf::from(value),
            ("--profile", Some(value)) if value == "release" => release = true,
            ("--profile", Some(value)) if value == "dev" => release = false,
            _ => return Err(usage().to_string()),
        }
        index += 2;
    }
    if out.exists() && fs::read_dir(&out).is_ok_and(|mut entries| entries.next().is_some()) {
        return Err(format!(
            "{} is not empty; refusing to reuse an owned build directory",
            out.display()
        ));
    }
    fs::create_dir_all(&out)
        .map_err(|error| format!("failed to create {}: {error}", out.display()))?;
    let out = fs::canonicalize(&out)
        .map_err(|error| format!("failed to resolve {}: {error}", out.display()))?;
    let source = fs::canonicalize(&source)
        .map_err(|error| format!("failed to resolve {}: {error}", source.display()))?;
    let worktree = out.join("src");
    let result = build_all(&source, &out, &worktree, release);
    // The detached worktree is scratch; the binaries and manifest are the output.
    let _ = tool(
        "git",
        &["worktree", "remove", "--force", &path_text(&worktree)],
        &source,
    );
    let manifest = result?;
    let path = out.join("manifest.json");
    let text = serde_json::to_string_pretty(&manifest)
        .map_err(|error| format!("failed to render the manifest: {error}"))?;
    fs::write(&path, format!("{text}\n"))
        .map_err(|error| format!("failed to write {}: {error}", path.display()))?;
    println!(
        "termination-removal-builds: wrote {} ({} controls)",
        path.display(),
        PATCHES.len()
    );
    Ok(())
}

fn build_all(source: &Path, out: &Path, worktree: &Path, release: bool) -> Result<Value, String> {
    let commit = tool("git", &["rev-parse", "--verify", "HEAD"], source)?
        .trim()
        .to_string();
    tool(
        "git",
        &["worktree", "add", "--detach", &path_text(worktree), "HEAD"],
        source,
    )?;
    let target_dir = out.join("target");
    let profile_dir = if release { "release" } else { "debug" };
    let mut build_args = vec!["build", "-p", "ripr", "--locked"];
    if release {
        build_args.push("--release");
    }
    let built = target_dir.join(profile_dir).join("ripr");
    let build = |label: &str| -> Result<PathBuf, String> {
        tool_env(
            "cargo",
            &build_args,
            worktree,
            &[("CARGO_TARGET_DIR", &path_text(&target_dir))],
        )?;
        let dir = out.join(label);
        fs::create_dir_all(&dir)
            .map_err(|error| format!("failed to create {}: {error}", dir.display()))?;
        let copy = dir.join("ripr");
        fs::copy(&built, &copy)
            .map_err(|error| format!("failed to copy {}: {error}", built.display()))?;
        Ok(copy)
    };

    let candidate = build("candidate")?;
    let candidate_sha = hash(&candidate)?;
    let candidate_version = reported_version(&candidate)?;
    let mut controls = Vec::new();
    for patch in PATCHES {
        for edit in patch.edits {
            let path = worktree.join(edit.file);
            let text = fs::read_to_string(&path)
                .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
            if text.matches(edit.old).count() != 1 {
                return Err(format!(
                    "patch {} no longer applies: `{}` must occur exactly once in {}; \
                     the repair moved, so update PATCHES",
                    patch.id,
                    edit.old.trim(),
                    edit.file
                ));
            }
            fs::write(&path, text.replacen(edit.old, &edit_text(edit), 1))
                .map_err(|error| format!("failed to write {}: {error}", path.display()))?;
        }
        let binary = build(patch.id)?;
        tool("git", &["checkout", "--", "."], worktree)?;
        let version = reported_version(&binary)?;
        // The build script marks uncommitted edits, so a patched build of the
        // same commit reports the candidate's version with `-dirty`.
        let expected =
            candidate_version.replacen(&format!("({commit})"), &format!("({commit}-dirty)"), 1);
        if version != expected {
            return Err(format!(
                "patched build {} reports `{version}`, expected `{expected}`",
                patch.id
            ));
        }
        controls.push(json!({
            "id": patch.id,
            "families": patch.families.iter().map(|family| family.as_str()).collect::<Vec<_>>(),
            "binary": format!("{}/ripr", patch.id),
            "sha256": hash(&binary)?,
            "version": version,
            "edits_sha256": edits_digest(patch),
            "edited_files": patch.edits.iter().map(|edit| edit.file).collect::<Vec<_>>(),
        }));
    }
    Ok(json!({
        "schema": MANIFEST_SCHEMA,
        "source_commit": commit,
        "profile": profile_dir,
        "candidate": {
            "binary": "candidate/ripr",
            "sha256": candidate_sha,
            "version": candidate_version,
        },
        "controls": controls,
    }))
}

/// One verified manifest entry, ready to bind a removal control.
pub(super) struct ManifestControl {
    pub(super) id: String,
    pub(super) families: Vec<Family>,
    pub(super) path: PathBuf,
    pub(super) sha256: String,
    pub(super) edits_sha256: String,
}

pub(super) struct Manifest {
    pub(super) source_commit: String,
    pub(super) controls: Vec<ManifestControl>,
}

pub(super) fn load_manifest(path: &Path) -> Result<Manifest, String> {
    let text = fs::read_to_string(path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    let root: Value = serde_json::from_str(&text)
        .map_err(|error| format!("{} is not JSON: {error}", path.display()))?;
    if root.get("schema").and_then(Value::as_str) != Some(MANIFEST_SCHEMA) {
        return Err(format!(
            "{} is not a {MANIFEST_SCHEMA} manifest",
            path.display()
        ));
    }
    let field = |value: &Value, name: &str| -> Result<String, String> {
        value
            .get(name)
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| format!("manifest field `{name}` is missing"))
    };
    let base = path.parent().unwrap_or_else(|| Path::new("."));
    let mut controls = Vec::new();
    for entry in root
        .get("controls")
        .and_then(Value::as_array)
        .ok_or("manifest has no controls")?
    {
        let mut families = Vec::new();
        for name in entry
            .get("families")
            .and_then(Value::as_array)
            .ok_or("manifest control has no families")?
        {
            let name = name.as_str().unwrap_or_default();
            families.push(
                Family::ALL
                    .iter()
                    .copied()
                    .find(|family| family.as_str() == name)
                    .ok_or_else(|| format!("manifest names unknown family `{name}`"))?,
            );
        }
        controls.push(ManifestControl {
            id: field(entry, "id")?,
            families,
            path: base.join(field(entry, "binary")?),
            sha256: field(entry, "sha256")?,
            edits_sha256: field(entry, "edits_sha256")?,
        });
    }
    Ok(Manifest {
        source_commit: field(&root, "source_commit")?,
        controls,
    })
}

/// A version line with the uncommitted-edits marker removed, so a patched
/// build of a commit compares equal to the clean build of that commit.
pub(super) fn without_dirty(version: &str) -> String {
    version.replacen("-dirty)", ")", 1)
}

/// The commit a `ripr --version` line was built from: `ripr 0.11.0 (<sha>)`.
pub(super) fn version_commit(version: &str) -> Option<&str> {
    let (_, tail) = version.rsplit_once('(')?;
    tail.strip_suffix(')').filter(|sha| !sha.is_empty())
}

fn hash(path: &Path) -> Result<String, String> {
    fs::read(path)
        .map(|bytes| crate::blind_journey::sha256_hex(&bytes))
        .map_err(|error| format!("failed to read {}: {error}", path.display()))
}

fn path_text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn tool(program: &str, args: &[&str], cwd: &Path) -> Result<String, String> {
    tool_env(program, args, cwd, &[])
}

fn tool_env(
    program: &str,
    args: &[&str],
    cwd: &Path,
    envs: &[(&str, &str)],
) -> Result<String, String> {
    let owned: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let output = crate::run::capture_bytes_in_dir_with_timeout(
        Path::new(program),
        &owned,
        cwd,
        envs,
        &[],
        BUILD_DEADLINE,
        "termination removal build",
    )?;
    if output.timed_out || !output.status.is_some_and(|status| status.success()) {
        return Err(format!(
            "`{program} {}` failed in {}: {}",
            args.join(" "),
            cwd.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn usage() -> &'static str {
    "usage: cargo xtask termination-removal-builds [--source <repo>] [--out <dir>] [--profile release|dev]"
}
