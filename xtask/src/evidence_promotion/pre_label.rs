//! Pre-label metadata proof for the published PR's exact target composition (#15536).
use super::report_cache_schema::{
    SEAM_CACHE_SOURCE, check_reports, current_cache_schema_version, read_pins,
};
use super::{EVIDENCE_PROMOTION_HONESTY_CORPUS, parse_json_rejecting_duplicate_keys};
use serde_json::Value;
use std::path::Path;

fn git(root: &Path, args: &[&str]) -> Result<String, String> {
    let mut argv = vec!["--no-replace-objects"];
    argv.extend_from_slice(args);
    let argv = argv.iter().map(|arg| arg.to_string()).collect::<Vec<_>>();
    let output = crate::run::capture_process_output_in("git", &argv, Some(root), &[], &[], &[])
        .map_err(|err| err.message)?;
    String::from_utf8(output).map_err(|err| err.to_string())
}

pub(super) fn check(root: &Path, args: &[String]) -> Result<(), String> {
    let [pr, target] = args else {
        return Err(
            "usage: check-evidence-promotion-honesty --pre-label <PR> <target-branch>".into(),
        );
    };
    let number = pr
        .parse::<u64>()
        .ok()
        .filter(|n| *n != 0)
        .ok_or("expected a positive PR number")?;
    let base_ref = format!("refs/heads/{target}");
    let head_ref = format!("refs/pull/{number}/head");
    let merge_ref = format!("refs/pull/{number}/merge");
    git(root, &["check-ref-format", &base_ref])?;
    let listing = git(
        root,
        &[
            "ls-remote",
            "--refs",
            "origin",
            &base_ref,
            &head_ref,
            &merge_ref,
        ],
    )?;
    let oid = |name: &str| {
        listing.lines().filter_map(|line| line.split_once('\t'))
            .find(|(_, reference)| *reference == name)
            .map(|(sha, _)| sha)
            .filter(|s| super::is_exact_git_commit(s))
            .ok_or_else(|| format!("origin does not advertise {name}; refresh the PR composition before requesting qualification"))
    };
    let (base, head) = (oid(&base_ref)?, oid(&head_ref)?);
    println!("PR #{number} target={target} base={base} head={head}");
    let merge = oid(&merge_ref)?;
    println!("composition={merge}");
    let read_commit = || git(root, &["cat-file", "commit", merge]);
    let commit = match read_commit() {
        Ok(commit) => commit,
        Err(_) => {
            git(
                root,
                &[
                    "fetch",
                    "--no-tags",
                    "--no-write-fetch-head",
                    "--refmap=",
                    "origin",
                    merge,
                ],
            )?;
            read_commit()?
        }
    };
    verify_parents(&commit, base, head)?;
    check_tree(root, merge)
}

fn verify_parents(commit: &str, base: &str, head: &str) -> Result<(), String> {
    let parents: Vec<_> = commit
        .lines()
        .take_while(|line| !line.is_empty())
        .filter_map(|line| line.strip_prefix("parent "))
        .collect();
    if parents == [base, head] {
        Ok(())
    } else {
        Err(format!(
            "stale or unavailable PR composition: expected parents [{base}, {head}], found {parents:?}; refresh before requesting qualification"
        ))
    }
}

fn check_tree(root: &Path, tree: &str) -> Result<(), String> {
    let corpus_path = Path::new(EVIDENCE_PROMOTION_HONESTY_CORPUS);
    let dir = corpus_path.with_file_name("reports");
    let directory = super::normalize_path(&dir);
    let listing = git(
        root,
        &[
            "ls-tree",
            "--full-tree",
            "--long",
            "-r",
            "-t",
            "-z",
            tree,
            "--",
            SEAM_CACHE_SOURCE,
            EVIDENCE_PROMOTION_HONESTY_CORPUS,
            &directory,
        ],
    )?;
    let mut input = String::new();
    let mut entries = Vec::new();
    let mut expected_bytes = 0usize;
    for record in listing.split('\0').filter(|s| !s.is_empty()) {
        let (meta, path) = record.split_once('\t').ok_or("malformed Git tree entry")?;
        let direct_report = path
            .strip_prefix(&directory)
            .and_then(|suffix| suffix.strip_prefix('/'))
            .is_some_and(|name| !name.contains('/') && name.ends_with(".json"));
        if path == directory {
            if !meta.starts_with("040000 tree ") {
                return Err(format!("{path} must be a directory"));
            }
            continue;
        }
        if path != SEAM_CACHE_SOURCE && path != EVIDENCE_PROMOTION_HONESTY_CORPUS && !direct_report
        {
            continue;
        }
        let fields: Vec<_> = meta.split_whitespace().collect();
        let [mode, "blob", oid, size] = fields.as_slice() else {
            return Err(format!("{path} must be a regular file"));
        };
        if !matches!(*mode, "100644" | "100755") {
            return Err(format!("{path} must be a regular file"));
        }
        let size = size.parse::<usize>().map_err(|e| format!("{path}: {e}"))?;
        let header = format!("{oid} blob {size}\n");
        expected_bytes = expected_bytes
            .checked_add(header.len() + 1)
            .and_then(|bytes| bytes.checked_add(size))
            .ok_or("Git batch size overflow")?;
        input.push_str(oid);
        input.push('\n');
        entries.push((path, header, size));
    }
    // No checkout or per-report process. Validate each raw response against
    // the OID and byte length from the immutable tree, including repeated OIDs.
    let output = crate::run::capture_bytes_in_dir_with_budget(
        Path::new("git"),
        &[
            "--no-replace-objects".into(),
            "cat-file".into(),
            "--batch".into(),
        ],
        (root, Some(input.as_bytes())),
        &[],
        crate::run::ByteCaptureBudget {
            timeout: std::time::Duration::from_mins(1),
            stdout_bytes: expected_bytes,
            stderr_bytes: 64 * 1024,
        },
        "pre-label report blobs",
    )?;
    if output.timed_out || !output.status.is_some_and(|status| status.success()) {
        return Err(format!(
            "Git report batch failed or timed out: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let mut remaining = output.stdout.as_slice();
    let mut reports = Vec::new();
    let mut corpus = None;
    let mut current = None;
    for (path, header, size) in entries {
        let bytes =
            take_blob(&mut remaining, &header, size).map_err(|error| format!("{path}: {error}"))?;
        let text = std::str::from_utf8(bytes).map_err(|error| format!("{path}: {error}"))?;
        match path {
            SEAM_CACHE_SOURCE => current = Some(current_cache_schema_version(text)?),
            EVIDENCE_PROMOTION_HONESTY_CORPUS => {
                corpus = Some(parse_json_rejecting_duplicate_keys(text)?)
            }
            _ => reports.push((
                path.to_string(),
                serde_json::from_str::<Value>(text).map_err(|e| format!("{path}: {e}"))?,
            )),
        }
    }
    if !remaining.is_empty() {
        return Err("unexpected trailing Git batch output".into());
    }
    let current = current.ok_or_else(|| format!("composition lacks {SEAM_CACHE_SOURCE}"))?;
    let corpus =
        corpus.ok_or_else(|| format!("composition lacks {EVIDENCE_PROMOTION_HONESTY_CORPUS}"))?;
    let mut violations = Vec::new();
    if !corpus.is_object()
        || corpus
            .get("report_cache_schema_pins")
            .is_some_and(|pins| !pins.is_array())
    {
        return Err("invalid honesty corpus or report_cache_schema_pins array".into());
    }
    let pins = read_pins(&corpus, &mut violations);
    check_reports(&reports, &current, &pins, &mut violations);
    if !violations.is_empty() {
        return Err(violations.join("\n"));
    }
    println!(
        "Checked {} reports and {} historical pins at generation {current}",
        reports.len(),
        pins.len()
    );
    Ok(())
}

fn take_blob<'a>(remaining: &mut &'a [u8], header: &str, size: usize) -> Result<&'a [u8], String> {
    let body = remaining
        .strip_prefix(header.as_bytes())
        .ok_or("Git batch object identity, type or size mismatch")?;
    let (bytes, tail) = body.split_at_checked(size).ok_or("truncated Git blob")?;
    *remaining = tail
        .strip_prefix(b"\n")
        .ok_or("missing Git batch terminator")?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batch_binds_each_record_even_for_repeated_oids_and_embedded_newlines() -> Result<(), String>
    {
        let header = "abc blob 3\n";
        let mut data = b"abc blob 3\na\nb\nabc blob 3\na\nb\n".as_slice();
        for _ in 0..2 {
            assert_eq!(take_blob(&mut data, header, 3)?, b"a\nb");
        }
        assert!(data.is_empty());
        let wrong_header = "Git batch object identity, type or size mismatch";
        for (mut bad, expected) in [
            (b"xyz blob 3\na\nb\n".as_slice(), wrong_header),
            (b"abc blob 2\na\n", wrong_header),
            (b"abc blob 3\na", "truncated Git blob"),
            (b"abc blob 3\na\nb!", "missing Git batch terminator"),
        ] {
            let actual = take_blob(&mut bad, header, 3).err();
            if actual.as_deref() != Some(expected) {
                return Err(format!("expected {expected}, got {actual:?}"));
            }
        }
        Ok(())
    }

    #[test]
    fn pre_label_requires_exact_ordered_published_parents() -> Result<(), String> {
        let raw = "tree abc\nparent base\nparent head\n\nparent fake\n";
        verify_parents(raw, "base", "head")?;
        for (commit, base, head) in [
            (raw, "new-base", "head"),
            (raw, "head", "base"),
            ("tree abc\nparent base\n\nparent head", "base", "head"),
        ] {
            if !verify_parents(commit, base, head).is_err_and(|e| e.contains("expected parents")) {
                return Err(format!("expected a parent mismatch for {base} and {head}"));
            }
        }
        Ok(())
    }

    #[test]
    fn published_composition_checks_all_reports_and_reasoned_pins()
    -> Result<(), Box<dyn std::error::Error>> {
        use std::fs;
        let remote = crate::tests::temp_dir("pre-label-remote");
        let local = crate::tests::temp_dir("pre-label-reader");
        let run = |args: &[&str]| git(&remote, args).map(|s| s.trim().to_string());
        run(&["init", "-q"])?;
        run(&["config", "user.name", "fixture"])?;
        run(&["config", "user.email", "fixture@example.invalid"])?;
        git(&local, &["init", "-q"])?;
        git(
            &local,
            &[
                "remote",
                "add",
                "origin",
                remote.to_str().ok_or("non-UTF8 fixture root")?,
            ],
        )?;
        let corpus = Path::new(EVIDENCE_PROMOTION_HONESTY_CORPUS);
        let report_dir = corpus.with_file_name("reports");
        fs::create_dir_all(remote.join(&report_dir))?;
        fs::create_dir_all(
            remote
                .join(SEAM_CACHE_SOURCE)
                .parent()
                .ok_or("missing parent")?,
        )?;
        let stamp = |name: &str, version: &str| {
            fs::write(
                remote.join(&report_dir).join(name),
                serde_json::json!({"source_subject":{"corpus":{"cache_schema_version":version}}})
                    .to_string(),
            )
        };
        let schema = |version: &str| {
            fs::write(
                remote.join(SEAM_CACHE_SOURCE),
                format!("pub(crate) const CACHE_SCHEMA_VERSION: &str = \"{version}\";\n"),
            )
        };
        let pin = |name: &str, reason: &str| {
            fs::write(
                remote.join(corpus),
                serde_json::json!({"report_cache_schema_pins":[{
                    "report": super::super::normalize_path(&report_dir.join(name)),
                    "cache_schema_version": "1.9", "reason":reason
                }]})
                .to_string(),
            )
        };
        let commit = |parents: &[&str]| -> Result<String, String> {
            run(&["add", "-A"])?;
            let tree = run(&["write-tree"])?;
            let mut args = vec!["commit-tree", &tree, "-m", "fixture"];
            for parent in parents {
                args.extend(["-p", *parent]);
            }
            run(&args)
        };
        let publish = |base: &str, head: &str| -> Result<(), String> {
            let tree = run(&["merge-tree", "--write-tree", base, head])?;
            let merge = run(&[
                "commit-tree",
                &tree,
                "-p",
                base,
                "-p",
                head,
                "-m",
                "composition",
            ])?;
            for (name, sha) in [
                ("refs/heads/main", base),
                ("refs/pull/7/head", head),
                ("refs/pull/7/merge", &merge),
            ] {
                run(&["update-ref", name, sha])?;
            }
            Ok(())
        };
        let check = || super::check(&local, &["7".into(), "main".into()]);
        schema("1.10")?;
        stamp("pinned.json", "1.9")?;
        pin("pinned.json", "historical witness")?;
        let old = commit(&[])?;
        stamp("capture.json", "1.10")?;
        let head = commit(&[&old])?;
        publish(&old, &head)?;
        check()?;

        // An unrelated target advance changes the analyzer generation while the
        // branch stays internally consistent. First expose a lagging merge ref.
        fs::remove_file(remote.join(&report_dir).join("capture.json"))?;
        schema("1.11")?;
        let new = commit(&[&old])?;
        run(&["update-ref", "refs/heads/main", &new])?;
        assert!(check().is_err_and(|e| e.contains("expected parents")));
        publish(&new, &head)?;
        assert!(check().is_err_and(|e| e.contains("capture.json: records cache schema `1.10`")));

        // A schema-only PR and an unrelated PR inheriting a stale base must
        // both inspect this unchanged report.
        publish(&head, &new)?;
        assert!(check().is_err_and(|e| e.contains("capture.json: records cache schema")));
        let stale = run(&["rev-parse", "refs/pull/7/merge"])?;
        stamp("capture.json", "1.10")?;
        fs::write(remote.join("unrelated"), "changed")?;
        let unrelated = commit(&[&stale])?;
        publish(&stale, &unrelated)?;
        assert!(check().is_err_and(|e| e.contains("capture.json: records cache schema")));

        stamp("capture.json", "1.11")?;
        let repaired = commit(&[&unrelated])?;
        publish(&stale, &repaired)?;
        check()?;
        let nested = local.join("subdirectory");
        fs::create_dir(&nested)?;
        super::check(&nested, &["7".into(), "main".into()])?;
        for (path, reason, diagnostic) in [
            ("missing.json", "historical", "not a committed"),
            ("pinned.json", " ", "non-empty"),
        ] {
            pin(path, reason)?;
            let bad = commit(&[&repaired])?;
            publish(&stale, &bad)?;
            assert!(check().is_err_and(|e| e.contains(diagnostic)));
        }
        fs::write(remote.join(corpus), r#"{"report_cache_schema_pins":false}"#)?;
        let malformed = commit(&[&repaired])?;
        publish(&stale, &malformed)?;
        assert!(check().is_err_and(|e| e.contains("report_cache_schema_pins array")));
        run(&["update-ref", "-d", "refs/pull/7/merge"])?;
        assert!(check().is_err_and(|e| e.contains("does not advertise refs/pull/7/merge")));
        fs::remove_dir_all(&remote)?;
        fs::remove_dir_all(&local)?;
        Ok(())
    }
}
