//! The published `ripr cache export` schema, checked against bytes the real
//! command wrote.
//!
//! `schemas/ripr/cache-export.schema.json` describes the success report. The
//! report's counts are also held to the export directory it names, so a
//! schema-valid report that misstates what was copied still fails.

use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

#[path = "common/mod.rs"]
mod common;
#[path = "common/published_schema.rs"]
mod published_schema;

use common::fixture_git::fixture_git_ok;

const CACHE_EXPORT_SCHEMA: &str = "schemas/ripr/cache-export.schema.json";

static FIXTURE_COUNTER: AtomicU64 = AtomicU64::new(0);

fn unique(label: &str) -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!(
        "ripr-cache-export-schema-{label}-{}-{stamp}-{}",
        std::process::id(),
        FIXTURE_COUNTER.fetch_add(1, Ordering::Relaxed)
    ))
}

fn write(root: &Path, path: &str, text: &str) -> Result<(), String> {
    std::fs::write(root.join(path), text).map_err(|error| format!("write {path}: {error}"))
}

/// A one-commit Rust package with one function and one test.
fn fixture_repo(label: &str) -> Result<PathBuf, String> {
    let root = unique(label);
    for dir in ["src", "tests"] {
        std::fs::create_dir_all(root.join(dir))
            .map_err(|error| format!("create {dir}: {error}"))?;
    }
    fixture_git_ok(&root, &["init", "-q", "--initial-branch=main"])?;
    fixture_git_ok(&root, &["config", "user.email", "cache-export@t.st"])?;
    fixture_git_ok(&root, &["config", "user.name", "cache-export"])?;
    fixture_git_ok(&root, &["config", "core.autocrlf", "false"])?;
    write(
        &root,
        "Cargo.toml",
        "[package]\nname = \"cache_export_fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )?;
    write(
        &root,
        "src/lib.rs",
        "pub fn over(amount: i32, threshold: i32) -> bool {\n    amount > threshold\n}\n",
    )?;
    write(
        &root,
        "tests/over.rs",
        "use cache_export_fixture::over;\n#[test]\nfn big() {\n    assert!(over(10, 1));\n}\n",
    )?;
    fixture_git_ok(&root, &["add", "-A"])?;
    fixture_git_ok(&root, &["commit", "-q", "-m", "base"])?;
    Ok(root)
}

fn ripr(args: &[&str]) -> Result<Output, String> {
    Command::new(env!("CARGO_BIN_EXE_ripr"))
        .args(args)
        .output()
        .map_err(|error| format!("spawn ripr {args:?}: {error}"))
}

/// Fill the workspace cache through `ripr agent brief --json`, one of the
/// commands that stores the classified inventory `cache export` copies. It
/// writes only to stdout and the workspace cache.
fn populate_cache(root: &Path) -> Result<(), String> {
    let root_arg = root.display().to_string();
    let output = ripr(&[
        "agent", "brief", "--json", "--base", "HEAD", "--root", &root_arg,
    ])?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "ripr agent brief exited {:?}; stderr:\n{}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

/// Run `ripr cache export` into `out` and parse the success report.
fn export_json(root: &Path, out: &Path) -> Result<Value, String> {
    let root_arg = root.display().to_string();
    let out_arg = out.display().to_string();
    let output = ripr(&["cache", "export", "--root", &root_arg, "--out", &out_arg])?;
    if !output.status.success() {
        return Err(format!(
            "ripr cache export exited {:?}; stderr:\n{}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    serde_json::from_slice(&output.stdout).map_err(|error| {
        format!(
            "cache export JSON did not parse: {error}\nstdout:\n{}",
            String::from_utf8_lossy(&output.stdout)
        )
    })
}

/// Whether two `ripr` processes from this build share a cache identity. A
/// clean checkout build is keyed by its commit and a dirty release build by a
/// source digest; a dirty or commit-less dev/test build is keyed per process
/// (`build_identity`), so no later process can reuse, or export, what an
/// earlier one cached. CI builds a clean checkout, so this is `true` there.
fn builds_share_a_cache_identity() -> bool {
    let commit = env!("RIPR_BUILD_COMMIT");
    let dirty = env!("RIPR_BUILD_COMMIT_DIRTY") == "true";
    (!commit.is_empty() && !dirty) || !env!("RIPR_BUILD_SOURCE_DIGEST").is_empty()
}

/// `None` when this build cannot carry a cache from one process to the next
/// (see [`builds_share_a_cache_identity`]); that is refused under CI so the
/// success-path checks can never be bypassed there.
fn success_path_available(test: &str) -> Result<bool, String> {
    if builds_share_a_cache_identity() {
        return Ok(true);
    }
    if std::env::var_os("CI").is_some() {
        return Err(format!(
            "{test}: this CI build keys its cache per process (dirty or commit-less dev build), so cache export cannot succeed; build a clean checkout"
        ));
    }
    println!(
        "{test}: not run; this local build has uncommitted crate sources and keys its cache per process, so a second process cannot export the first one's cache"
    );
    Ok(false)
}

/// A real success report, with the export directory it describes.
fn exported(label: &str) -> Result<(Value, PathBuf), String> {
    let root = fixture_repo(label)?;
    let out = unique(&format!("{label}-out"));
    let report = populate_cache(&root).and_then(|()| export_json(&root, &out));
    let _ = std::fs::remove_dir_all(&root);
    match report {
        Ok(report) => Ok((report, out)),
        Err(error) => {
            let _ = std::fs::remove_dir_all(&out);
            Err(error)
        }
    }
}

fn conforms(document: &Value, schema: &Value) -> Result<(), String> {
    published_schema::check(document, schema, schema, "cache_export")
}

/// The regular files under `dir` and the sum of their lengths.
fn files_on_disk(dir: &Path) -> Result<(u64, u64), String> {
    let mut count = 0;
    let mut bytes = 0;
    let mut pending = vec![dir.to_path_buf()];
    while let Some(next) = pending.pop() {
        let entries = std::fs::read_dir(&next)
            .map_err(|error| format!("read {}: {error}", next.display()))?;
        for entry in entries {
            let entry = entry.map_err(|error| error.to_string())?;
            let kind = entry.file_type().map_err(|error| error.to_string())?;
            if kind.is_dir() {
                pending.push(entry.path());
            } else if kind.is_file() {
                count += 1;
                bytes += entry.metadata().map_err(|error| error.to_string())?.len();
            }
        }
    }
    Ok((count, bytes))
}

#[test]
fn the_checker_covers_every_keyword_the_cache_export_schema_uses() -> Result<(), String> {
    let schema = published_schema::read(CACHE_EXPORT_SCHEMA)?;
    let ignored = published_schema::unsupported_keywords(&schema);
    if ignored.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "the conformance checker ignores these keywords of {CACHE_EXPORT_SCHEMA}, so it would under-check the schema: {ignored:?}"
        ))
    }
}

/// A real export conforms, and its counts describe the directory it names:
/// `copied_files` and `logical_file_bytes` equal the regular files and bytes
/// under `cache_dir`, and the copy holds at least one file per counted entry.
#[test]
fn cache_export_json_conforms_and_counts_what_it_copied() -> Result<(), String> {
    if !success_path_available("cache_export_json_conforms_and_counts_what_it_copied")? {
        return Ok(());
    }
    let schema = published_schema::read(CACHE_EXPORT_SCHEMA)?;
    let (report, out) = exported("conforms")?;
    let checked = (|| {
        conforms(&report, &schema)?;
        let named = PathBuf::from(report["cache_dir"].as_str().unwrap_or_default());
        let real = std::fs::canonicalize(&out).map_err(|error| error.to_string())?;
        let (files, bytes) = files_on_disk(&out)?;
        let entries = report["classified_entries"].as_u64().unwrap_or(0)
            + report["file_fact_entries"].as_u64().unwrap_or(0);
        let observed = json!([
            named == real,
            report["copied_files"] == json!(files),
            report["logical_file_bytes"] == json!(bytes),
            files >= entries,
        ]);
        if observed == json!([true, true, true, true]) {
            Ok(())
        } else {
            Err(format!(
                "report {report} does not describe {} ({files} files, {bytes} bytes): {observed}",
                out.display()
            ))
        }
    })();
    let _ = std::fs::remove_dir_all(&out);
    checked
}

/// A failed export writes no JSON: the schema describes success stdout only.
#[test]
fn cache_export_without_a_cache_writes_no_json() -> Result<(), String> {
    let root = fixture_repo("no-cache")?;
    let out = unique("no-cache-out");
    let root_arg = root.display().to_string();
    let out_arg = out.display().to_string();
    let output = ripr(&["cache", "export", "--root", &root_arg, "--out", &out_arg]);
    let _ = std::fs::remove_dir_all(&root);
    let _ = std::fs::remove_dir_all(&out);
    let output = output?;
    let observed = json!([output.status.code(), output.stdout.is_empty()]);
    if observed == json!([2, true]) {
        Ok(())
    } else {
        Err(format!(
            "expected exit 2 with empty stdout, got {observed}; stdout:\n{}",
            String::from_utf8_lossy(&output.stdout)
        ))
    }
}

/// The negative experiment: the schema rejects a real report once one
/// contract term is broken.
#[test]
fn published_cache_export_schema_rejects_documents_it_forbids() -> Result<(), String> {
    type Mutation = fn(&mut Value);
    if !success_path_available("published_cache_export_schema_rejects_documents_it_forbids")? {
        return Ok(());
    }
    let schema = published_schema::read(CACHE_EXPORT_SCHEMA)?;
    let (report, out) = exported("negative")?;
    let _ = std::fs::remove_dir_all(&out);
    conforms(&report, &schema)?;
    let cases: [(&str, Mutation); 10] = [
        ("unknown field", |doc| doc["status"] = json!("ok")),
        ("another version", |doc| {
            doc["schema_version"] = json!("0.2")
        }),
        ("no classified entry", |doc| {
            doc["classified_entries"] = json!(0)
        }),
        ("more classified families than exist", |doc| {
            doc["classified_entries"] = json!(3)
        }),
        ("a classified count past the signed 64-bit range", |doc| {
            doc["classified_entries"] = json!(u64::MAX)
        }),
        ("a negative count", |doc| {
            doc["file_fact_entries"] = json!(-1)
        }),
        ("a count as a string", |doc| {
            doc["missing_file_facts"] = json!("0")
        }),
        ("no bytes copied", |doc| {
            doc["logical_file_bytes"] = json!(0)
        }),
        ("an empty cache_dir", |doc| doc["cache_dir"] = json!("")),
        ("a missing field", |doc| {
            if let Some(object) = doc.as_object_mut() {
                object.remove("copied_files");
            }
        }),
    ];
    for (label, mutate) in cases {
        let mut document = report.clone();
        mutate(&mut document);
        if document == report {
            return Err(format!("{label}: the mutation changed nothing"));
        }
        match conforms(&document, &schema) {
            Ok(()) => {
                return Err(format!(
                    "the published cache export schema accepted a document the contract forbids: {label}"
                ));
            }
            Err(error) => println!("rejected as expected ({label}): {error}"),
        }
    }
    Ok(())
}
