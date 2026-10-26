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
//!   with input blobs resolved independently from that expected checkout commit,
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
    fs::create_dir_all(&options.out)
        .map_err(|err| format!("cannot create {}: {err}", options.out.display()))?;
    // A rejected run must not leave a previous run's accepted receipts behind.
    for name in ["junit.xml", "run-context.txt", "shards-summary.txt"] {
        let path = options.out.join(name);
        match fs::remove_file(&path) {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => return Err(format!("cannot clear {}: {err}", path.display())),
        }
    }
    let expected_blobs = expected_input_blobs(Path::new("."), &options.sha);
    let outcome = verify(&options, expected_blobs);
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

fn verify(options: &Options, expected_blobs: Result<Vec<String>, String>) -> Outcome {
    let mut problems = Vec::new();
    let expected_blobs = match expected_blobs {
        Ok(blobs) => Some(blobs),
        Err(why) => {
            problems.push(format!("expected checkout input blobs: {why}"));
            None
        }
    };
    let mut summary = String::from("### Shard cover\n");
    let mut suites = String::new();
    let mut union: Vec<String> = Vec::new();
    let mut total_tests = 0u64;
    let mut first_context = String::new();
    let mut first_environment = String::new();

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
            let environment = environment_of(&context);
            check_environment(&environment, expected_blobs.as_deref(), &tag, &mut problems);
            if shard == 1 {
                first_context = context;
                first_environment = environment;
            } else if environment != first_environment {
                problems.push(format!(
                    "{tag}: run-context tool versions or input blobs differ from shard 1"
                ));
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

/// List lines with terminal colour escapes removed: the hosted job sets
/// `CARGO_TERM_COLOR=always`, which nextest honours when it lists.
fn lines_of(text: &str) -> impl Iterator<Item = String> + '_ {
    text.lines()
        .map(strip_ansi)
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
}

fn strip_ansi(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars();
    while let Some(ch) = chars.next() {
        if ch != '\u{1b}' {
            out.push(ch);
            continue;
        }
        if chars.next() == Some('[') {
            for next in chars.by_ref() {
                if next.is_ascii_alphabetic() {
                    break;
                }
            }
        }
    }
    out
}

/// The archive root moves with the optional report paths a shard also uploads,
/// so files are found by name below the shard's artifact directory. Two members
/// with one name are ambiguous, so neither is trusted.
fn read_named(dir: &Path, name: &str, tag: &str, problems: &mut Vec<String>) -> Option<String> {
    let mut found = Vec::new();
    find_files(dir, name, &mut found);
    match found.as_slice() {
        [path] => match fs::read_to_string(path) {
            Ok(text) => Some(text),
            Err(err) => {
                problems.push(format!("{tag}: cannot read {name}: {err}"));
                None
            }
        },
        [] => {
            problems.push(format!("{tag}: {name} is missing"));
            None
        }
        _ => {
            problems.push(format!(
                "{tag}: {name} is ambiguous ({} members)",
                found.len()
            ));
            None
        }
    }
}

fn find_files(dir: &Path, name: &str, found: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            find_files(&path, name, found);
        } else if path.file_name().and_then(|n| n.to_str()) == Some(name) {
            found.push(path);
        }
    }
}

const PARTITION_KEYS: [&str; 8] = [
    "repository=",
    "checkout_sha=",
    "run_id=",
    "run_attempt=",
    "profile=",
    "shard=",
    "command=",
    "nextest_exit=",
];

/// The tool-version and blob-identity lines, which every shard must share.
fn environment_of(context: &str) -> String {
    context
        .lines()
        .filter(|line| !PARTITION_KEYS.iter().any(|key| line.starts_with(key)))
        .map(|line| format!("{line}\n"))
        .collect()
}

const INPUT_PATHS: [&str; 4] = [
    "Cargo.lock",
    ".config/nextest.toml",
    ".github/workflows/rust-gates.yml",
    ".github/workflows/rust-test-shards.yml",
];
const INPUT_HEADER: &str =
    "input_blobs (Cargo.lock, nextest.toml, rust-gates.yml, rust-test-shards.yml):";

fn is_blob_id(value: &str) -> bool {
    value.len() == 40 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// Resolve the immutable expected inputs independently of the shard receipts.
/// The workflow supplies its checkout SHA; dirty files, the index and receipt
/// consensus cannot replace the four ordinary files at that commit.
fn expected_input_blobs(root: &Path, sha: &str) -> Result<Vec<String>, String> {
    if !is_blob_id(sha) {
        return Err("--sha must name a full 40-hex checkout commit".to_string());
    }
    let git = |args: Vec<String>| crate::run::run_output_owned_in("git", &args, root);
    let commit = git(vec![
        "--no-replace-objects".to_string(),
        "rev-parse".to_string(),
        "--verify".to_string(),
        format!("{sha}^{{commit}}"),
    ])?;
    if commit.trim() != sha {
        return Err("--sha does not name the expected checkout commit".to_string());
    }
    let mut args = vec![
        "--no-replace-objects".to_string(),
        "ls-tree".to_string(),
        "--full-tree".to_string(),
        "-r".to_string(),
        sha.to_string(),
        "--".to_string(),
    ];
    args.extend(INPUT_PATHS.map(str::to_string));
    let tree = git(args)?;
    parse_input_tree(&tree)
}

fn parse_input_tree(tree: &str) -> Result<Vec<String>, String> {
    let mut blobs = BTreeMap::new();
    for row in tree.lines() {
        let (metadata, path) = row.split_once('\t').ok_or("malformed input tree row")?;
        let fields: Vec<&str> = metadata.split_whitespace().collect();
        if fields.len() != 3
            || !matches!(fields[0], "100644" | "100755")
            || fields[1] != "blob"
            || !is_blob_id(fields[2])
            || !INPUT_PATHS.contains(&path)
            || blobs.insert(path, fields[2]).is_some()
        {
            return Err(format!("expected an ordinary input blob at `{path}`"));
        }
    }
    INPUT_PATHS
        .iter()
        .map(|path| {
            blobs
                .get(path)
                .map(|blob| (*blob).to_string())
                .ok_or_else(|| format!("checkout has no input blob at `{path}`"))
        })
        .collect()
}

/// Require the producer's exact ordered block, anchored to the expected commit.
/// Agreement between shards remains necessary, but is not input authority.
fn check_environment(
    environment: &str,
    expected_blobs: Option<&[String]>,
    tag: &str,
    problems: &mut Vec<String>,
) {
    for tool in ["cargo-nextest ", "rustc ", "cargo "] {
        if !environment.lines().any(|line| line.starts_with(tool)) {
            problems.push(format!(
                "{tag}: run-context has no `{}` version",
                tool.trim()
            ));
        }
    }
    let lines: Vec<&str> = environment.lines().collect();
    let headers: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter_map(|(index, line)| line.starts_with("input_blobs").then_some(index))
        .collect();
    let [header] = headers.as_slice() else {
        problems.push(format!(
            "{tag}: run-context must have one input_blobs header"
        ));
        return;
    };
    if lines[*header] != INPUT_HEADER {
        problems.push(format!(
            "{tag}: run-context input_blobs header is not canonical"
        ));
        return;
    }
    let blobs = &lines[*header + 1..];
    if blobs.len() != INPUT_PATHS.len() || blobs.iter().any(|blob| !is_blob_id(blob)) {
        problems.push(format!(
            "{tag}: run-context must name exactly four ordered input blobs"
        ));
        return;
    }
    if let Some(expected) = expected_blobs {
        for ((path, got), want) in INPUT_PATHS.iter().zip(blobs).zip(expected) {
            if got != want {
                problems.push(format!(
                    "{tag}: input blob `{path}` is `{got}`, expected `{want}`"
                ));
            }
        }
    }
}

fn check_context(
    context: &str,
    options: &Options,
    shard: u32,
    tag: &str,
    problems: &mut Vec<String>,
) {
    // Check the original receipt before environment_of removes partition keys.
    // Every scalar assignment is a singleton, including extension keys and
    // identical duplicates. Tool-version prose and verbose colon metadata are
    // not assignments; values may themselves contain '='.
    let mut values = BTreeMap::new();
    for (name, value) in context.lines().filter_map(|line| line.split_once('=')) {
        let key = name.trim();
        if !key.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
            || !key
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
        {
            continue;
        }
        if name != key {
            problems.push(format!("{tag}: run-context key `{name}` is not canonical"));
        }
        if values.insert(key, value.trim()).is_some() {
            problems.push(format!("{tag}: run-context has duplicate key `{key}`"));
        }
    }
    if values.contains_key("shards") {
        problems.push(format!(
            "{tag}: run-context key `shards` is reserved for the combined receipt"
        ));
    }
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
        match values.get(key).copied() {
            Some(got) if got == want => {}
            Some(got) => problems.push(format!(
                "{tag}: run-context {key} is `{got}`, expected `{want}`"
            )),
            None => problems.push(format!("{tag}: run-context has no {key}")),
        }
    }
}

struct Suite {
    tag: BTreeMap<String, String>,
    cases: u64,
}

struct Parsed {
    root: BTreeMap<String, String>,
    suites: Vec<Suite>,
    body: String,
    identities: Vec<String>,
    forbidden: BTreeSet<String>,
}

/// Elements whose presence means a test did not simply pass.
const OUTCOME_ELEMENTS: [&str; 5] = [
    "failure",
    "error",
    "flakyFailure",
    "rerunFailure",
    "skipped",
];

/// Validate XML before extracting parser-bounded direct-child suites. Keeping
/// each suite's original bytes preserves its properties, stdout and CDATA;
/// tokens inside comments or attributes cannot become extraction boundaries.
fn parse_junit(junit: &str) -> Result<Parsed, String> {
    let document = roxmltree::Document::parse(junit).map_err(|err| err.to_string())?;
    let root = document.root_element();
    if root.tag_name().name() != "testsuites" {
        return Err("expected one `<testsuites>` root".to_string());
    }
    let attributes = |node: roxmltree::Node<'_, '_>| {
        node.attributes()
            .map(|attribute| (attribute.name().to_string(), attribute.value().to_string()))
            .collect()
    };
    let mut parsed = Parsed {
        root: attributes(root),
        suites: Vec::new(),
        body: String::new(),
        identities: Vec::new(),
        forbidden: BTreeSet::new(),
    };
    for node in root.descendants().filter(roxmltree::Node::is_element) {
        // The native nextest vocabulary has no namespaces. A detached suite
        // must not depend on namespace or xml:* declarations at the old root.
        if node.tag_name().namespace().is_some()
            || node
                .attributes()
                .any(|attribute| attribute.namespace().is_some())
        {
            return Err("namespaced JUnit is unsupported".to_string());
        }
        if node == root {
            continue;
        }
        let name = node.tag_name().name();
        let parent = node.parent_element().map(|parent| parent.tag_name().name());
        match (name, parent) {
            ("testsuites", _) => return Err("nested `<testsuites>` root".to_string()),
            ("testsuite", Some("testsuites")) => {
                parsed.suites.push(Suite {
                    tag: attributes(node),
                    cases: node
                        .children()
                        .filter(|child| child.has_tag_name("testcase"))
                        .count() as u64,
                });
                parsed.body.push_str(&junit[node.range()]);
                parsed.body.push('\n');
            }
            ("testsuite", _) => return Err("`<testsuite>` outside `<testsuites>`".to_string()),
            ("testcase", Some("testsuite")) => parsed.identities.push(format!(
                "{} {}",
                node.attribute("classname").unwrap_or_default(),
                node.attribute("name").unwrap_or_default()
            )),
            ("testcase", _) => return Err("`<testcase>` outside a `<testsuite>`".to_string()),
            (other, _) if OUTCOME_ELEMENTS.contains(&other) => {
                parsed.forbidden.insert(other.to_string());
            }
            _ => {}
        }
    }
    Ok(parsed)
}

/// A count attribute that is present must be a number; absent is `None`.
fn count_attribute(tag: &BTreeMap<String, String>, name: &str) -> Result<Option<u64>, String> {
    match tag.get(name) {
        None => Ok(None),
        Some(value) => value
            .parse::<u64>()
            .map(Some)
            .map_err(|err| format!("{name}=\"{value}\" is not a number: {err}")),
    }
}

/// Returns the number of testcases and the report's `<testsuite>` elements, or
/// every reason the report is not an acceptable record of the partition.
fn check_junit(
    junit: &str,
    listed: &BTreeSet<String>,
    tag: &str,
) -> Result<(u64, String), Vec<String>> {
    let parsed = match parse_junit(junit) {
        Ok(parsed) => parsed,
        Err(why) => {
            return Err(vec![format!(
                "{tag}: junit.xml is not a complete report: {why}"
            )]);
        }
    };
    let mut problems = Vec::new();
    let cases = parsed.identities.len() as u64;
    match count_attribute(&parsed.root, "tests") {
        Ok(Some(declared)) if declared == cases && cases > 0 => {}
        Ok(declared) => problems.push(format!(
            "{tag}: junit.xml declares {} tests but holds {cases} testcases",
            declared.map_or_else(|| "no".to_string(), |count| count.to_string())
        )),
        Err(why) => problems.push(format!("{tag}: junit.xml root {why}")),
    }
    for name in ["failures", "errors", "skipped"] {
        match count_attribute(&parsed.root, name) {
            Ok(Some(0) | None) => {}
            Ok(Some(count)) => problems.push(format!("{tag}: junit.xml reports {count} {name}")),
            Err(why) => problems.push(format!("{tag}: junit.xml root {why}")),
        }
    }
    for suite in &parsed.suites {
        match count_attribute(&suite.tag, "tests") {
            Ok(Some(declared)) if declared == suite.cases => {}
            Ok(declared) => problems.push(format!(
                "{tag}: a testsuite declares {} tests but holds {} testcases",
                declared.map_or_else(|| "no".to_string(), |count| count.to_string()),
                suite.cases
            )),
            Err(why) => problems.push(format!("{tag}: a testsuite {why}")),
        }
        for name in ["failures", "errors", "skipped"] {
            match count_attribute(&suite.tag, name) {
                Ok(Some(0) | None) => {}
                Ok(Some(count)) => {
                    problems.push(format!("{tag}: a testsuite reports {count} {name}"));
                }
                Err(why) => problems.push(format!("{tag}: a testsuite {why}")),
            }
        }
    }
    for element in &parsed.forbidden {
        problems.push(format!("{tag}: junit.xml contains a <{element}> element"));
    }
    let mut identities = BTreeSet::new();
    for identity in &parsed.identities {
        if !identities.insert(identity.clone()) {
            problems.push(format!("{tag}: junit.xml repeats testcase `{identity}`"));
        }
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
    if !problems.is_empty() {
        return Err(problems);
    }
    Ok((cases, parsed.body))
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
    out.push_str(&environment_of(first));
    out.push_str("nextest_exit=0\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHA: &str = "abc123";
    const BLOB: &str = "0123456789abcdef0123456789abcdef01234567";

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
            "repository=o/r\ncheckout_sha={SHA}\nrun_id=7\nrun_attempt=1\nprofile=ci\nshard={shard}/{shards}\ncommand=cargo nextest run --workspace --profile ci --partition count:{shard}/{shards}\ncargo-nextest 0.9\nrustc 1.95.0\ncargo 1.95.0\n{INPUT_HEADER}\n{BLOB}\n{BLOB}\n{BLOB}\n{BLOB}\nnextest_exit=0\n"
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
        let outcome = verify(&options(&root, 2), Ok(vec![BLOB.to_string(); 4]));
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
        let outcome = verify(&options(&root, 2), Ok(vec![BLOB.to_string(); 4]));
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
                "not a complete report",
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
                "reports 1 failures",
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
    fn a_rejected_run_clears_the_previous_accepted_receipts() -> Result<(), String> {
        let _cwd_guard = crate::acquire_test_cwd_read_guard();
        let root = crate::tests::temp_dir("verify-shards-supersede");
        lay_out(&root, false)?;
        let sha = crate::run::run_output_owned_in(
            "git",
            &["rev-parse".to_string(), "HEAD".to_string()],
            Path::new("."),
        )?
        .trim()
        .to_string();
        let blobs = expected_input_blobs(Path::new("."), &sha)?.join("\n") + "\n";
        for shard in 1..=2 {
            rewrite(
                &root,
                shard,
                "run-context.txt",
                &context(shard, 2)
                    .replace(SHA, &sha)
                    .replace(&format!("{BLOB}\n").repeat(4), &blobs),
            );
        }
        let args: Vec<String> = [
            "--dir",
            "in",
            "--artifact",
            "art",
            "--attempt",
            "1",
            "--repository",
            "o/r",
            "--sha",
            &sha,
            "--run-id",
            "7",
            "--shards",
            "2",
            "--out",
            "out",
        ]
        .iter()
        .map(|arg| {
            if matches!(*arg, "in" | "out") {
                root.join(arg).display().to_string()
            } else {
                (*arg).to_string()
            }
        })
        .collect();
        run(&args)?;
        assert!(root.join("out/junit.xml").exists());
        assert!(root.join("out/run-context.txt").exists());
        rewrite(&root, 1, "shard-1.status", "100\n");
        assert!(run(&args).is_err());
        assert!(!root.join("out/junit.xml").exists());
        assert!(!root.join("out/run-context.txt").exists());
        rewrite(&root, 1, "shard-1.status", "0\n");
        run(&args)?;
        let mut unknown_commit = args.clone();
        for value in &mut unknown_commit {
            if value == &sha {
                *value = "f".repeat(40);
            }
        }
        assert!(run(&unknown_commit).is_err());
        let left = ["junit.xml", "run-context.txt"]
            .iter()
            .filter(|name| root.join("out").join(name).exists())
            .count();
        let _ = fs::remove_dir_all(&root);
        assert_eq!(left, 0, "a rejected run left accepted receipts behind");
        Ok(())
    }

    #[test]
    fn coloured_list_lines_match_their_plain_testcases() {
        let coloured = "\u{1b}[35;1mb\u{1b}[0m \u{1b}[34mone\u{1b}[0m\nb two\n";
        let lines: Vec<String> = lines_of(coloured).collect();
        assert_eq!(lines, ["b one", "b two"]);
    }

    #[test]
    fn xml_escapes_in_identities_are_resolved() -> Result<(), String> {
        let xml = junit(&["b a &lt;b&gt; &amp; c &#x31;"]);
        let parsed = parse_junit(&xml)?;
        assert_eq!(parsed.identities, ["b a <b> & c 1"]);
        Ok(())
    }

    #[test]
    fn options_require_every_flag() {
        assert!(parse_options(&[]).is_err());
        assert!(parse_options(&["--bogus".to_string(), "x".to_string()]).is_err());
    }
}

#[cfg(test)]
#[path = "verify_test_shards/tests.rs"]
mod contract_tests;
