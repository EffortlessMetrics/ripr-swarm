//! Records which commit this `ripr` binary was built from.
//!
//! Emits `RIPR_BUILD_COMMIT` (a full commit id, or empty when unknown) and
//! `RIPR_BUILD_COMMIT_DIRTY` (`true` or `false`) for `ripr --version` and
//! `ripr doctor`. Sources, in order:
//!
//! 1. `.cargo_vcs_info.json`, which `cargo package` writes into every packaged
//!    crate. A crates.io install or an unpacked `.crate` has no `.git`, so this
//!    is its only commit record.
//! 2. The enclosing Git checkout, but only when it tracks this manifest. A
//!    crate unpacked inside some unrelated repository must not borrow that
//!    repository's HEAD.
//!
//! Anything else leaves the commit empty: unknown is reported as unknown,
//! never guessed. Both variables are always emitted so an ambient environment
//! variable of the same name cannot stand in for the record.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[path = "src/build_commit_record.rs"]
mod build_commit_record;
#[path = "src/source_digest.rs"]
mod source_digest;

use build_commit_record::{is_full_commit_id, parse_cargo_vcs_info};
use source_digest::{CRATE_SOURCES, WORKSPACE_INPUTS};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    let manifest_dir = std::env::var_os("CARGO_MANIFEST_DIR");
    let identity = manifest_dir.as_deref().map(Path::new).and_then(|dir| {
        let vcs_info = dir.join(".cargo_vcs_info.json");
        if vcs_info.is_file() {
            println!("cargo:rerun-if-changed=.cargo_vcs_info.json");
            fs::read_to_string(vcs_info)
                .ok()
                .and_then(|text| parse_cargo_vcs_info(&text))
        } else {
            checkout_identity(dir)
        }
    });
    let (commit, dirty) = identity.unwrap_or_default();
    // A commit identifies a clean build's code. A dirty or commit-less build
    // is identified by its sources instead, so persisted analysis caches never
    // serve one such build's results to another. Baking that digest changes a
    // compile-time environment variable on every source edit, which defeats
    // sccache for dev and test builds, so it is opt-in: only a build that will
    // read or write persisted analysis caches sets `RIPR_PERSISTED_CACHE=1`.
    // Without it the digest stays empty and `build_identity` uses a
    // per-process identity, so nothing persisted is reused.
    let persisted_cache = std::env::var("RIPR_PERSISTED_CACHE").ok();
    let source_digest = if source_digest::should_bake(persisted_cache.as_deref(), &commit, dirty) {
        manifest_dir
            .as_deref()
            .map(Path::new)
            .and_then(|dir| {
                let files = source_digest::source_files(dir)?;
                for file in &files {
                    println!("cargo:rerun-if-changed={}", file.display());
                }
                let root = source_digest::workspace_root(dir);
                source_digest::digest_files(&root, &files)
            })
            .unwrap_or_default()
    } else {
        String::new()
    };
    println!("cargo:rustc-env=RIPR_BUILD_COMMIT={commit}");
    println!("cargo:rustc-env=RIPR_BUILD_COMMIT_DIRTY={dirty}");
    println!("cargo:rustc-env=RIPR_BUILD_SOURCE_DIGEST={source_digest}");
}

fn checkout_identity(dir: &Path) -> Option<(String, bool)> {
    git(dir, &["ls-files", "--error-unmatch", "--", "Cargo.toml"])?;
    let commit = git(dir, &["rev-parse", "--verify", "HEAD"])?;
    if !is_full_commit_id(&commit) {
        return None;
    }
    // Re-run when HEAD moves and when an input the dirty flag covers changes.
    // A detached commit rewrites HEAD; a branch commit rewrites that branch's
    // loose ref (created in its directory when the branch was packed-only);
    // pack-refs rewrites packed-refs; a reftable repository rewrites its
    // table directory. Other branches are not watched, so creating or
    // deleting them does not rebuild ripr. Only existing paths are emitted:
    // Cargo treats a missing path as always changed.
    let mut watched: Vec<PathBuf> = ["HEAD", "packed-refs", "reftable"]
        .into_iter()
        .filter_map(|name| git(dir, &["rev-parse", "--git-path", name]))
        .map(|path| dir.join(path))
        .collect();
    if let Some(branch) = git(dir, &["symbolic-ref", "-q", "HEAD"])
        && let Some(path) = git(dir, &["rev-parse", "--git-path", &branch])
    {
        let path = dir.join(path);
        watched.extend(if path.exists() {
            Some(path)
        } else {
            path.parent().map(Path::to_path_buf)
        });
    }
    watched.extend(CRATE_SOURCES.iter().map(|source| dir.join(source)));
    let top = git(dir, &["rev-parse", "--show-toplevel"]).map(PathBuf::from);
    watched.extend(
        top.iter()
            .flat_map(|top| WORKSPACE_INPUTS.iter().map(move |input| top.join(input))),
    );
    for path in watched.iter().filter(|path| path.exists()) {
        println!("cargo:rerun-if-changed={}", path.display());
    }

    let top_inputs: Vec<String> = WORKSPACE_INPUTS
        .iter()
        .map(|input| format!(":(top){input}"))
        .collect();
    let mut status = vec![
        "--no-optional-locks",
        "status",
        "--porcelain",
        "--untracked-files=normal",
        "--",
    ];
    status.extend(CRATE_SOURCES);
    status.extend(top_inputs.iter().map(String::as_str));
    let dirty = !git(dir, &status)?.is_empty();
    Some((commit, dirty))
}

/// Run `git` in `dir` and return its trimmed stdout, or `None` when it cannot
/// run or exits nonzero. Inherited repository overrides (set inside Git hooks)
/// are cleared so the query describes this checkout.
fn git(dir: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout)
        .ok()
        .map(|text| text.trim().to_string())
}
