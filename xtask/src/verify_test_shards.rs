//! `cargo xtask verify-test-shards` (#10507): the verdict on the hosted required
//! test phase when `rust-test-shards.yml` runs it as `--partition count:N/N`
//! jobs.
//!
//! Four green shard jobs are not yet the one unpartitioned `cargo nextest run
//! --workspace --profile ci`. This command accepts the shards only when
//! together they are conservation of that run:
//!
//! - every shard retained a status of `0`, a run context naming this repository,
//!   checkout, run, attempt, profile, partition and command with `nextest_exit=0`,
//!   and a JUnit report that is complete, reports no failure or error, and whose
//!   declared test count equals its testcase elements;
//! - every shard's JUnit testcase identities (`classname name`, the binary id and
//!   test name that `cargo nextest list --message-format oneline` prints) are
//!   exactly its own partition list, so a forged, truncated or substituted
//!   report cannot stand in for the tests the partition selects;
//! - the partition lists are pairwise disjoint and equal to the full workspace
//!   list.
//!
//! It then writes one combined `junit.xml` and `run-context.txt`, the files the
//! unpartitioned step retained, so consumers of the `<artifact>-nextest-<attempt>`
//! evidence read the same shape, plus `shards-summary.txt`. Any problem is a
//! failure; a missing file is a failure, never a pass.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

struct Options {
    dir: PathBuf,
    artifact: String,
    attempt: String,
    repository: String,
    sha: String,
    run_id: String,
    shards: u32,
    out: PathBuf,
}

pub(crate) fn run(args: &[String]) -> Result<(), String> {
    let options = parse_options(args)?;
    let outcome = verify(&options);
    fs::create_dir_all(&options.out)
        .map_err(|err| format!("cannot create {}: {err}", options.out.display()))?;
    write(&options.out.join("shards-summary.txt"), &outcome.summary)?;
    println!("{}", outcome.summary);
    if outcome.problems.is_empty() {
        write(&options.out.join("junit.xml"), &outcome.junit)?;
        write(&options.out.join("run-context.txt"), &outcome.context)?;
        return Ok(());
    }
    for problem in &outcome.problems {
        println!("::error::{problem}");
    }
    Err(format!(
        "verify-test-shards: {} problem(s); the hosted shards are not a complete green run",
        outcome.problems.len()
    ))
}

fn write(path: &Path, text: &str) -> Result<(), String> {
    fs::write(path, text).map_err(|err| format!("cannot write {}: {err}", path.display()))
}

fn parse_options(args: &[String]) -> Result<Options, String> {
    let mut values: BTreeMap<&str, String> = BTreeMap::new();
    let mut iter = args.iter();
    while let Some(flag) = iter.next() {
        let name = match flag.as_str() {
            "--dir" => "dir",
            "--artifact" => "artifact",
            "--attempt" => "attempt",
            "--repository" => "repository",
            "--sha" => "sha",
            "--run-id" => "run-id",
            "--shards" => "shards",
            "--out" => "out",
            other => return Err(format!("verify-test-shards: unknown argument `{other}`")),
        };
        let value = iter
            .next()
            .ok_or_else(|| format!("verify-test-shards: {flag} needs a value"))?;
        values.insert(name, value.clone());
    }
    let take = |name: &str| {
        values
            .get(name)
            .cloned()
            .ok_or_else(|| format!("verify-test-shards: --{name} is required"))
    };
    let shards: u32 = take("shards")?
        .parse()
        .map_err(|err| format!("verify-test-shards: --shards must be a number: {err}"))?;
    if shards == 0 {
        return Err("verify-test-shards: --shards must be at least 1".to_string());
    }
    Ok(Options {
        dir: PathBuf::from(take("dir")?),
        artifact: take("artifact")?,
        attempt: take("attempt")?,
        repository: take("repository")?,
        sha: take("sha")?,
        run_id: take("run-id")?,
        shards,
        out: PathBuf::from(take("out")?),
    })
}

struct Outcome {
    problems: Vec<String>,
    summary: String,
    junit: String,
    context: String,
}

fn verify(options: &Options) -> Outcome {
    let mut problems = Vec::new();
    let mut summary = String::from("### Shard cover\n");
    let mut suites = String::new();
    let mut union: Vec<String> = Vec::new();
    let mut total_tests = 0u64;
    let mut first_context = String::new();

    for shard in 1..=options.shards {
        let dir = options.dir.join(format!(
            "{}-nextest-shard-{shard}-{}",
            options.artifact, options.attempt
        ));
        let tag = format!("shard {shard}");
        let list = read_named(&dir, &format!("list-{shard}.txt"), &tag, &mut problems);
        let list_lines: BTreeSet<String> = list
            .as_deref()
            .map(|text| lines_of(text).collect())
            .unwrap_or_default();
        if let Some(text) = &list {
            union.extend(lines_of(text));
        }

        let status = read_named(&dir, &format!("shard-{shard}.status"), &tag, &mut problems);
        if let Some(status) = &status
            && status.trim() != "0"
        {
            problems.push(format!("{tag}: nextest status is `{}`", status.trim()));
        }

        if let Some(context) = read_named(&dir, "run-context.txt", &tag, &mut problems) {
            check_context(&context, options, shard, &tag, &mut problems);
            if shard == 1 {
                first_context = context;
            }
        }

        let mut tests = 0u64;
        if let Some(junit) = read_named(&dir, "junit.xml", &tag, &mut problems) {
            match check_junit(&junit, &list_lines, &tag) {
                Ok((count, body)) => {
                    tests = count;
                    suites.push_str(&body);
                }
                Err(found) => problems.extend(found),
            }
        }
        total_tests += tests;
        summary.push_str(&format!(
            "shard {shard}: listed {}, junit tests {tests}\n",
            list_lines.len()
        ));
    }

    let all = options.dir.join(format!(
        "{}-nextest-shard-1-{}",
        options.artifact, options.attempt
    ));
    let all_lines: BTreeSet<String> =
        match read_named(&all, "list-all.txt", "full workspace list", &mut problems) {
            Some(text) => lines_of(&text).collect(),
            None => BTreeSet::new(),
        };
    if all_lines.is_empty() {
        problems.push("the full workspace test list is empty".to_string());
    }
    union.sort();
    let mut duplicated = BTreeSet::new();
    for pair in union.windows(2) {
        if pair[0] == pair[1] {
            duplicated.insert(pair[0].clone());
        }
    }
    let union_set: BTreeSet<String> = union.iter().cloned().collect();
    let missing: Vec<&String> = all_lines.difference(&union_set).collect();
    let extra: Vec<&String> = union_set.difference(&all_lines).collect();
    summary.push_str(&format!(
        "workspace tests listed: {}\njunit tests across shards: {total_tests}\n\
         duplicated across shards: {}\nmissing from shards: {}\nnot in workspace list: {}\n",
        all_lines.len(),
        duplicated.len(),
        missing.len(),
        extra.len()
    ));
    for (what, names) in [
        (
            "duplicated across shards",
            duplicated.iter().collect::<Vec<_>>(),
        ),
        ("missing from shards", missing),
        ("not in workspace list", extra),
    ] {
        if let Some(first) = names.first() {
            problems.push(format!("{} test(s) {what}, e.g. `{first}`", names.len()));
        }
    }
    if total_tests != all_lines.len() as u64 {
        problems.push(format!(
            "shard JUnit reports total {total_tests} tests against {} workspace tests",
            all_lines.len()
        ));
    }

    let junit = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <testsuites name=\"nextest-run\" tests=\"{total_tests}\" failures=\"0\" errors=\"0\">\n\
         {suites}</testsuites>\n"
    );
    let context = combined_context(&first_context, options);
    Outcome {
        problems,
        summary,
        junit,
        context,
    }
}

fn lines_of(text: &str) -> impl Iterator<Item = String> + '_ {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
}

/// The archive root moves with the optional report paths a shard also uploads,
/// so files are found by name below the shard's artifact directory.
fn read_named(dir: &Path, name: &str, tag: &str, problems: &mut Vec<String>) -> Option<String> {
    match find_file(dir, name).map(|path| fs::read_to_string(&path)) {
        Some(Ok(text)) => Some(text),
        Some(Err(err)) => {
            problems.push(format!("{tag}: cannot read {name}: {err}"));
            None
        }
        None => {
            problems.push(format!("{tag}: {name} is missing"));
            None
        }
    }
}

fn find_file(dir: &Path, name: &str) -> Option<PathBuf> {
    let mut pending = vec![dir.to_path_buf()];
    while let Some(next) = pending.pop() {
        let Ok(entries) = fs::read_dir(&next) else {
            continue;
        };
        let mut entries: Vec<_> = entries.filter_map(Result::ok).collect();
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if path.file_name().and_then(|n| n.to_str()) == Some(name) {
                return Some(path);
            }
        }
    }
    None
}

fn context_value<'a>(context: &'a str, key: &str) -> Option<&'a str> {
    context
        .lines()
        .filter_map(|line| line.split_once('='))
        .filter(|(name, _)| *name == key)
        .map(|(_, value)| value.trim())
        .next_back()
}

fn check_context(
    context: &str,
    options: &Options,
    shard: u32,
    tag: &str,
    problems: &mut Vec<String>,
) {
    let command = format!(
        "cargo nextest run --workspace --profile ci --partition count:{shard}/{}",
        options.shards
    );
    let expected = [
        ("repository", options.repository.clone()),
        ("checkout_sha", options.sha.clone()),
        ("run_id", options.run_id.clone()),
        ("run_attempt", options.attempt.clone()),
        ("profile", "ci".to_string()),
        ("shard", format!("{shard}/{}", options.shards)),
        ("command", command),
        ("nextest_exit", "0".to_string()),
    ];
    for (key, want) in expected {
        match context_value(context, key) {
            Some(got) if got == want => {}
            Some(got) => problems.push(format!(
                "{tag}: run-context {key} is `{got}`, expected `{want}`"
            )),
            None => problems.push(format!("{tag}: run-context has no {key}")),
        }
    }
}

/// Returns the declared test count and the report's `<testsuite>` elements, or
/// every reason the report is not an acceptable record of the partition.
fn check_junit(
    junit: &str,
    listed: &BTreeSet<String>,
    tag: &str,
) -> Result<(u64, String), Vec<String>> {
    let mut problems = Vec::new();
    let Some(root_start) = junit.find("<testsuites") else {
        return Err(vec![format!(
            "{tag}: junit.xml has no <testsuites> element"
        )]);
    };
    let Some(root_end) = tag_end(junit, root_start) else {
        return Err(vec![format!(
            "{tag}: junit.xml <testsuites> tag is not closed"
        )]);
    };
    let Some(close) = junit.rfind("</testsuites>") else {
        return Err(vec![format!("{tag}: junit.xml is not a complete report")]);
    };
    if close < root_end || !junit[close + "</testsuites>".len()..].trim().is_empty() {
        return Err(vec![format!("{tag}: junit.xml is not a complete report")]);
    }
    let root = &junit[root_start..=root_end];
    let number = |name: &str| attribute(root, name).and_then(|value| value.parse::<u64>().ok());
    let declared = number("tests");
    if declared.is_none_or(|count| count == 0) {
        problems.push(format!("{tag}: junit.xml declares no tests"));
    }
    for name in ["failures", "errors"] {
        if number(name).is_some_and(|count| count != 0) {
            problems.push(format!("{tag}: junit.xml reports {name}"));
        }
    }
    let body = &junit[root_end + 1..close];
    for element in ["<failure", "<error", "<flakyFailure", "<rerunFailure"] {
        if body.contains(element) {
            problems.push(format!("{tag}: junit.xml contains a {element}> element"));
        }
    }
    let mut identities = BTreeSet::new();
    let mut cases = 0u64;
    let mut cursor = 0;
    while let Some(offset) = body[cursor..].find("<testcase ") {
        let start = cursor + offset;
        let Some(end) = tag_end(body, start) else {
            problems.push(format!(
                "{tag}: junit.xml has an unterminated <testcase> tag"
            ));
            break;
        };
        let element = &body[start..=end];
        cases += 1;
        let identity = format!(
            "{} {}",
            attribute(element, "classname").unwrap_or_default(),
            attribute(element, "name").unwrap_or_default()
        );
        if !identities.insert(identity.clone()) {
            problems.push(format!("{tag}: junit.xml repeats testcase `{identity}`"));
        }
        cursor = end + 1;
    }
    if declared != Some(cases) {
        problems.push(format!(
            "{tag}: junit.xml declares {} tests but holds {cases} testcases",
            declared.map_or_else(|| "no".to_string(), |count| count.to_string())
        ));
    }
    if &identities != listed {
        let absent = listed.difference(&identities).count();
        let unlisted = identities.difference(listed).count();
        let example = listed
            .difference(&identities)
            .chain(identities.difference(listed))
            .next()
            .map_or(String::new(), |name| format!(", e.g. `{name}`"));
        problems.push(format!(
            "{tag}: junit.xml testcases are not the partition list ({absent} listed but not reported, {unlisted} reported but not listed{example})"
        ));
    }
    if problems.is_empty() {
        Ok((cases, format!("{}\n", body.trim_matches('\n'))))
    } else {
        Err(problems)
    }
}

/// Index of the `>` closing the tag that starts at `start`, skipping quoted
/// attribute values.
fn tag_end(text: &str, start: usize) -> Option<usize> {
    let mut quote: Option<char> = None;
    for (offset, ch) in text[start..].char_indices() {
        match (quote, ch) {
            (Some(open), c) if c == open => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(ch),
            (None, '>') => return Some(start + offset),
            _ => {}
        }
    }
    None
}

/// The unescaped value of ` name="..."` in one start tag.
fn attribute(tag: &str, name: &str) -> Option<String> {
    let needle = format!(" {name}=\"");
    let from = tag.find(&needle)? + needle.len();
    let len = tag[from..].find('"')?;
    Some(unescape(&tag[from..from + len]))
}

fn unescape(value: &str) -> String {
    value
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// The unpartitioned step's `run-context.txt` shape, from shard 1's tool
/// versions and blob identities, with the partition-specific lines replaced.
fn combined_context(first: &str, options: &Options) -> String {
    let mut out = format!(
        "repository={}\ncheckout_sha={}\nrun_id={}\nrun_attempt={}\nprofile=ci\n\
         shards={}/{} verified\ncommand=cargo nextest run --workspace --profile ci \
         (as {} partitions)\n",
        options.repository,
        options.sha,
        options.run_id,
        options.attempt,
        options.shards,
        options.shards,
        options.shards
    );
    for line in first.lines() {
        let partition_specific = [
            "repository=",
            "checkout_sha=",
            "run_id=",
            "run_attempt=",
            "profile=",
            "shard=",
            "command=",
            "nextest_exit=",
        ]
        .iter()
        .any(|prefix| line.starts_with(prefix));
        if !partition_specific {
            out.push_str(line);
            out.push('\n');
        }
    }
    out.push_str("nextest_exit=0\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHA: &str = "abc123";

    fn options(dir: &Path, shards: u32) -> Options {
        Options {
            dir: dir.join("in"),
            artifact: "art".to_string(),
            attempt: "1".to_string(),
            repository: "o/r".to_string(),
            sha: SHA.to_string(),
            run_id: "7".to_string(),
            shards,
            out: dir.join("out"),
        }
    }

    fn case(identity: &str) -> String {
        let (class, name) = identity.split_once(' ').unwrap_or((identity, ""));
        format!("<testcase name=\"{name}\" classname=\"{class}\" time=\"0.1\"/>\n")
    }

    fn junit(identities: &[&str]) -> String {
        let cases: String = identities.iter().map(|id| case(id)).collect();
        format!(
            "<?xml version=\"1.0\"?>\n<testsuites name=\"nextest-run\" tests=\"{}\" failures=\"0\" errors=\"0\">\n<testsuite name=\"b\" tests=\"{}\">\n{cases}</testsuite>\n</testsuites>\n",
            identities.len(),
            identities.len()
        )
    }

    fn context(shard: u32, shards: u32) -> String {
        format!(
            "repository=o/r\ncheckout_sha={SHA}\nrun_id=7\nrun_attempt=1\nprofile=ci\nshard={shard}/{shards}\ncommand=cargo nextest run --workspace --profile ci --partition count:{shard}/{shards}\ncargo-nextest 0.9\ninput_blobs:\nabcd\nnextest_exit=0\n"
        )
    }

    /// Lays out two good shards; `nested` mimics an archive rooted above
    /// `nextest/ci`, the other layout `upload-artifact` can produce.
    fn lay_out(root: &Path, nested: bool) -> Result<(), String> {
        let shards: [(u32, &[&str]); 2] = [(1, &["b one", "b two"]), (2, &["b three"])];
        for (shard, ids) in shards {
            let mut dir = root.join("in").join(format!("art-nextest-shard-{shard}-1"));
            if nested {
                dir = dir.join("nextest/ci");
            }
            fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            let put = |name: &str, text: String| {
                fs::write(dir.join(name), text).map_err(|e| e.to_string())
            };
            put(&format!("shard-{shard}.status"), "0\n".to_string())?;
            put("run-context.txt", context(shard, 2))?;
            put("junit.xml", junit(ids))?;
            put(&format!("list-{shard}.txt"), ids.join("\n") + "\n")?;
            if shard == 1 {
                put("list-all.txt", "b one\nb three\nb two\n".to_string())?;
            }
        }
        Ok(())
    }

    fn shard_dir(root: &Path, shard: u32) -> PathBuf {
        root.join("in").join(format!("art-nextest-shard-{shard}-1"))
    }

    fn problems_for(
        label: &str,
        nested: bool,
        tweak: impl Fn(&Path),
    ) -> Result<Vec<String>, String> {
        let root = crate::tests::temp_dir(&format!("verify-shards-{label}"));
        lay_out(&root, nested)?;
        tweak(&root);
        let outcome = verify(&options(&root, 2));
        let _ = fs::remove_dir_all(&root);
        Ok(outcome.problems)
    }

    #[test]
    fn good_shards_pass_in_either_archive_layout() -> Result<(), String> {
        for nested in [false, true] {
            let problems = problems_for("good", nested, |_| {})?;
            assert!(problems.is_empty(), "nested={nested}: {problems:?}");
        }
        Ok(())
    }

    #[test]
    fn combined_artifacts_keep_the_unpartitioned_shape() -> Result<(), String> {
        let root = crate::tests::temp_dir("verify-shards-combined");
        lay_out(&root, false)?;
        let outcome = verify(&options(&root, 2));
        let _ = fs::remove_dir_all(&root);
        assert!(outcome.problems.is_empty(), "{:?}", outcome.problems);
        assert!(
            outcome
                .junit
                .contains("tests=\"3\" failures=\"0\" errors=\"0\"")
        );
        assert!(outcome.junit.contains("name=\"three\""));
        assert!(outcome.context.contains(&format!("checkout_sha={SHA}")));
        assert!(outcome.context.contains("nextest_exit=0"));
        assert!(outcome.context.contains("cargo-nextest 0.9"));
        assert!(!outcome.context.contains("shard=1/2"));
        Ok(())
    }

    fn rewrite(root: &Path, shard: u32, name: &str, text: &str) {
        let _ = fs::write(shard_dir(root, shard).join(name), text);
    }

    #[test]
    fn missing_or_unproven_shard_evidence_fails() -> Result<(), String> {
        type Tweak = Box<dyn Fn(&Path)>;
        let cases: Vec<(&str, Tweak, &str)> = vec![
            (
                "no-shard",
                Box::new(|r| {
                    let _ = fs::remove_dir_all(shard_dir(r, 2));
                }),
                "missing",
            ),
            (
                "no-status",
                Box::new(|r| {
                    let _ = fs::remove_file(shard_dir(r, 2).join("shard-2.status"));
                }),
                "shard-2.status is missing",
            ),
            (
                "no-xml",
                Box::new(|r| {
                    let _ = fs::remove_file(shard_dir(r, 2).join("junit.xml"));
                }),
                "junit.xml is missing",
            ),
            (
                "no-context",
                Box::new(|r| {
                    let _ = fs::remove_file(shard_dir(r, 2).join("run-context.txt"));
                }),
                "run-context.txt is missing",
            ),
            (
                "red-status",
                Box::new(|r| rewrite(r, 1, "shard-1.status", "1\n")),
                "nextest status",
            ),
            (
                "stale-sha",
                Box::new(|r| rewrite(r, 2, "run-context.txt", &context(2, 2).replace(SHA, "old"))),
                "checkout_sha",
            ),
            (
                "wrong-run",
                Box::new(|r| {
                    rewrite(
                        r,
                        2,
                        "run-context.txt",
                        &context(2, 2).replace("run_id=7", "run_id=6"),
                    )
                }),
                "run_id",
            ),
            (
                "wrong-attempt",
                Box::new(|r| {
                    rewrite(
                        r,
                        2,
                        "run-context.txt",
                        &context(2, 2).replace("run_attempt=1", "run_attempt=2"),
                    )
                }),
                "run_attempt",
            ),
            (
                "wrong-profile",
                Box::new(|r| {
                    rewrite(
                        r,
                        2,
                        "run-context.txt",
                        &context(2, 2).replace("profile=ci", "profile=default"),
                    )
                }),
                "profile",
            ),
            (
                "wrong-shard",
                Box::new(|r| rewrite(r, 2, "run-context.txt", &context(1, 2))),
                "shard",
            ),
            (
                "exit-nonzero",
                Box::new(|r| {
                    rewrite(
                        r,
                        2,
                        "run-context.txt",
                        &context(2, 2).replace("nextest_exit=0", "nextest_exit=100"),
                    )
                }),
                "nextest_exit",
            ),
            (
                "unrelated-case",
                Box::new(|r| rewrite(r, 1, "junit.xml", &junit(&["x unrelated"]))),
                "not the partition list",
            ),
            (
                "truncated-xml",
                Box::new(|r| rewrite(r, 2, "junit.xml", "<testsuites tests=\"1\"")),
                "not closed",
            ),
            (
                "failures-attr",
                Box::new(|r| {
                    rewrite(
                        r,
                        2,
                        "junit.xml",
                        &junit(&["b three"]).replace("failures=\"0\"", "failures=\"1\""),
                    )
                }),
                "reports failures",
            ),
            (
                "failure-element",
                Box::new(|r| {
                    rewrite(
                        r,
                        2,
                        "junit.xml",
                        &junit(&["b three"]).replace("time=\"0.1\"/>", "><failure/></testcase>"),
                    )
                }),
                "<failure",
            ),
            (
                "count-lies",
                Box::new(|r| {
                    rewrite(
                        r,
                        1,
                        "junit.xml",
                        &junit(&["b one", "b two"]).replace(
                            "testsuites name=\"nextest-run\" tests=\"2\"",
                            "testsuites name=\"nextest-run\" tests=\"5\"",
                        ),
                    )
                }),
                "holds",
            ),
            (
                "duplicate-case",
                Box::new(|r| rewrite(r, 1, "junit.xml", &junit(&["b one", "b one"]))),
                "repeats",
            ),
            (
                "overlap",
                Box::new(|r| rewrite(r, 2, "list-2.txt", "b one\nb three\n")),
                "duplicated",
            ),
            (
                "dropped-test",
                Box::new(|r| rewrite(r, 1, "list-all.txt", "b one\nb three\nb two\nb four\n")),
                "missing from shards",
            ),
            (
                "empty-all",
                Box::new(|r| rewrite(r, 1, "list-all.txt", "")),
                "empty",
            ),
        ];
        for (label, tweak, expect) in cases {
            let problems = problems_for(label, false, tweak)?;
            assert!(
                problems.iter().any(|p| p.contains(expect)),
                "{label}: expected `{expect}` in {problems:?}"
            );
        }
        Ok(())
    }

    #[test]
    fn xml_escapes_in_identities_are_resolved() {
        let tag = "<testcase name=\"a &lt;b&gt; &amp; c\" classname=\"x::y/z\" time=\"1\"";
        assert_eq!(attribute(tag, "name").as_deref(), Some("a <b> & c"));
        assert_eq!(attribute(tag, "classname").as_deref(), Some("x::y/z"));
        assert_eq!(attribute(tag, "missing"), None);
    }

    #[test]
    fn options_require_every_flag() {
        assert!(parse_options(&[]).is_err());
        assert!(parse_options(&["--bogus".to_string(), "x".to_string()]).is_err());
    }
}
