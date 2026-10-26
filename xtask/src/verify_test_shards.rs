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
//! A partial "re-run failed jobs" uploads fresh artifacts only for the re-run
//! shards (#10605), so each shard's evidence is read from its latest attempt at
//! or before the verifying attempt; the run-context check binds the chosen
//! artifact to this repository, checkout, run, partition and its own attempt,
//! so a shard accepted from an earlier attempt is still exactly this run's
//! work on this checkout. The combined receipt discloses the attempts used.
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
    let outcome = verify(&options);
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
    let attempt = take("attempt")?;
    if attempt.parse::<u32>().is_err() {
        return Err(format!(
            "verify-test-shards: --attempt must be a number, got {attempt:?}"
        ));
    }
    Ok(Options {
        dir: PathBuf::from(take("dir")?),
        artifact: take("artifact")?,
        attempt,
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
    let mut first_environment = String::new();
    let mut used_attempts = BTreeSet::new();

    for shard in 1..=options.shards {
        let tag = format!("shard {shard}");
        let Some((dir, shard_attempt)) = select_shard_dir(options, shard) else {
            problems.push(format!(
                "{tag}: artifact is missing for attempts 1..={}",
                options.attempt
            ));
            summary.push_str(&format!("{tag}: no artifact\n"));
            continue;
        };
        used_attempts.insert(shard_attempt);
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
            check_context(&context, options, shard, shard_attempt, &tag, &mut problems);
            let environment = environment_of(&context);
            check_environment(&environment, &tag, &mut problems);
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
            "shard {shard}: listed {}, junit tests {tests} (attempt {shard_attempt})\n",
            list_lines.len()
        ));
    }

    let all_dir = select_shard_dir(options, 1).map_or_else(
        || {
            options.dir.join(format!(
                "{}-nextest-shard-1-{}",
                options.artifact, options.attempt
            ))
        },
        |(dir, _attempt)| dir,
    );
    let all_lines: BTreeSet<String> = match read_named(
        &all_dir,
        "list-all.txt",
        "full workspace list",
        &mut problems,
    ) {
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
    let context = combined_context(&first_context, options, &used_attempts);
    Outcome {
        problems,
        summary,
        junit,
        context,
    }
}

/// A partial "re-run failed jobs" uploads fresh artifacts only for the
/// re-run shards (#10605), so a shard's evidence is its latest artifact for
/// this run at or before the verifying attempt. Suffixes that are not
/// attempts, or name a future attempt, are not evidence.
fn select_shard_dir(options: &Options, shard: u32) -> Option<(PathBuf, u32)> {
    let current: u32 = options.attempt.parse().ok()?;
    let prefix = format!("{}-nextest-shard-{shard}-", options.artifact);
    let mut best: Option<(PathBuf, u32)> = None;
    let entries = fs::read_dir(&options.dir).ok()?;
    for entry in entries.filter_map(Result::ok) {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let Some(suffix) = name.strip_prefix(&prefix) else {
            continue;
        };
        let Ok(attempt) = suffix.parse::<u32>() else {
            continue;
        };
        if attempt == 0 || attempt > current {
            continue;
        }
        if best
            .as_ref()
            .is_none_or(|(_, best_attempt)| attempt > *best_attempt)
        {
            best = Some((entry.path(), attempt));
        }
    }
    best
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

fn context_value<'a>(context: &'a str, key: &str) -> Option<&'a str> {
    context
        .lines()
        .filter_map(|line| line.split_once('='))
        .filter(|(name, _)| *name == key)
        .map(|(_, value)| value.trim())
        .next_back()
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

/// A run context without tool versions and the four input blobs would not say
/// what produced the results, so it is not a receipt.
fn check_environment(environment: &str, tag: &str, problems: &mut Vec<String>) {
    for tool in ["cargo-nextest ", "rustc ", "cargo "] {
        if !environment.lines().any(|line| line.starts_with(tool)) {
            problems.push(format!(
                "{tag}: run-context has no `{}` version",
                tool.trim()
            ));
        }
    }
    let blobs = environment
        .lines()
        .skip_while(|line| !line.starts_with("input_blobs"))
        .skip(1)
        .filter(|line| line.len() == 40 && line.bytes().all(|b| b.is_ascii_hexdigit()))
        .count();
    if blobs < 4 {
        problems.push(format!(
            "{tag}: run-context names {blobs} input blobs, expected 4"
        ));
    }
}

fn check_context(
    context: &str,
    options: &Options,
    shard: u32,
    attempt: u32,
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
        ("run_attempt", attempt.to_string()),
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

struct Suite {
    tag: String,
    cases: u64,
}

struct Parsed {
    root: String,
    suites: Vec<Suite>,
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

/// Walks the report as XML: comments, CDATA and declarations are skipped, tags
/// must nest and close, `testsuite` sits in the one `testsuites` root and
/// `testcase` in a `testsuite` with no testcase inside another.
fn parse_junit(junit: &str) -> Result<Parsed, String> {
    let mut stack: Vec<String> = Vec::new();
    let mut parsed = Parsed {
        root: String::new(),
        suites: Vec::new(),
        identities: Vec::new(),
        forbidden: BTreeSet::new(),
    };
    let mut roots = 0;
    let mut pos = 0;
    while let Some(offset) = junit[pos..].find('<') {
        let start = pos + offset;
        let rest = &junit[start..];
        if rest.starts_with("<!--") {
            let end = rest.find("-->").ok_or("unterminated comment")?;
            pos = start + end + 3;
            continue;
        }
        if rest.starts_with("<![CDATA[") {
            let end = rest.find("]]>").ok_or("unterminated CDATA")?;
            pos = start + end + 3;
            continue;
        }
        if rest.starts_with("<?") || rest.starts_with("<!") {
            let end = rest.find('>').ok_or("unterminated declaration")?;
            pos = start + end + 1;
            continue;
        }
        let end = tag_end(junit, start).ok_or("unterminated tag")?;
        let text = &junit[start..=end];
        pos = end + 1;
        if let Some(closing) = text.strip_prefix("</") {
            let name = closing.trim_end_matches('>').trim();
            if stack.pop().as_deref() != Some(name) {
                return Err(format!("`</{name}>` does not close the open element"));
            }
            continue;
        }
        let self_closing = text.ends_with("/>");
        let name: String = text[1..]
            .chars()
            .take_while(|c| !c.is_whitespace() && *c != '>' && *c != '/')
            .collect();
        let parent = stack.last().map(String::as_str);
        match (name.as_str(), parent) {
            ("testsuites", None) => {
                roots += 1;
                parsed.root = text.to_string();
            }
            ("testsuites", _) | (_, None) => {
                return Err(format!("`<{name}>` outside the report root"));
            }
            ("testsuite", Some("testsuites")) => parsed.suites.push(Suite {
                tag: text.to_string(),
                cases: 0,
            }),
            ("testsuite", _) => return Err("`<testsuite>` outside `<testsuites>`".to_string()),
            ("testcase", Some("testsuite")) => {
                if let Some(suite) = parsed.suites.last_mut() {
                    suite.cases += 1;
                }
                parsed.identities.push(format!(
                    "{} {}",
                    attribute(text, "classname").unwrap_or_default(),
                    attribute(text, "name").unwrap_or_default()
                ));
            }
            ("testcase", _) => return Err("`<testcase>` outside a `<testsuite>`".to_string()),
            (other, _) if OUTCOME_ELEMENTS.contains(&other) => {
                parsed.forbidden.insert(other.to_string());
            }
            _ => {}
        }
        if !self_closing {
            stack.push(name);
        }
    }
    if !stack.is_empty() {
        return Err(format!("`<{}>` is never closed", stack.join(">, <")));
    }
    if roots != 1 {
        return Err(format!("expected one `<testsuites>` root, found {roots}"));
    }
    Ok(parsed)
}

/// A count attribute that is present must be a number; absent is `None`.
fn count_attribute(tag: &str, name: &str) -> Result<Option<u64>, String> {
    match attribute(tag, name) {
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
    let start = junit.find("<testsuite ").unwrap_or(0);
    let close = junit.rfind("</testsuites>").unwrap_or(junit.len());
    Ok((
        cases,
        format!("{}\n", junit[start..close].trim_matches('\n')),
    ))
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
/// `verified_attempts` discloses when the accepted shards span more than one
/// run attempt (a partial re-run, #10605): `run_attempt` stays the verifying
/// attempt, and the line names the attempts the shard evidence came from.
fn combined_context(first: &str, options: &Options, used_attempts: &BTreeSet<u32>) -> String {
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
    if used_attempts.len() > 1 {
        let attempts: Vec<String> = used_attempts.iter().map(u32::to_string).collect();
        out.push_str(&format!("verified_attempts={}\n", attempts.join(",")));
    }
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
        options_attempt(dir, shards, "1")
    }

    fn options_attempt(dir: &Path, shards: u32, attempt: &str) -> Options {
        Options {
            dir: dir.join("in"),
            artifact: "art".to_string(),
            attempt: attempt.to_string(),
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
        context_attempt(shard, shards, 1)
    }

    fn context_attempt(shard: u32, shards: u32, attempt: u32) -> String {
        format!(
            "repository=o/r\ncheckout_sha={SHA}\nrun_id=7\nrun_attempt={attempt}\nprofile=ci\nshard={shard}/{shards}\ncommand=cargo nextest run --workspace --profile ci --partition count:{shard}/{shards}\ncargo-nextest 0.9\nrustc 1.95.0\ncargo 1.95.0\ninput_blobs (a, b, c, d):\n{BLOB}\n{BLOB}\n{BLOB}\n{BLOB}\nnextest_exit=0\n"
        )
    }

    /// Lays out one shard attempt; `nested` mimics an archive rooted above
    /// `nextest/ci`, the other layout `upload-artifact` can produce.
    fn lay_out_shard(
        root: &Path,
        nested: bool,
        shard: u32,
        shards: u32,
        attempt: u32,
        ids: &[&str],
    ) -> Result<(), String> {
        let mut dir = root
            .join("in")
            .join(format!("art-nextest-shard-{shard}-{attempt}"));
        if nested {
            dir = dir.join("nextest/ci");
        }
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let put =
            |name: &str, text: String| fs::write(dir.join(name), text).map_err(|e| e.to_string());
        put(&format!("shard-{shard}.status"), "0\n".to_string())?;
        put("run-context.txt", context_attempt(shard, shards, attempt))?;
        put("junit.xml", junit(ids))?;
        put(&format!("list-{shard}.txt"), ids.join("\n") + "\n")?;
        if shard == 1 {
            put("list-all.txt", "b one\nb three\nb two\n".to_string())?;
        }
        Ok(())
    }

    /// Lays out two good shards; `nested` mimics an archive rooted above
    /// `nextest/ci`, the other layout `upload-artifact` can produce.
    fn lay_out(root: &Path, nested: bool) -> Result<(), String> {
        let shards: [(u32, &[&str]); 2] = [(1, &["b one", "b two"]), (2, &["b three"])];
        for (shard, ids) in shards {
            lay_out_shard(root, nested, shard, 2, 1, ids)?;
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

    /// A partial "re-run failed jobs" uploads fresh artifacts only for the
    /// re-run shards (#10605): shard 1 keeps its attempt-1 evidence while
    /// shard 2 is read from attempt 2, and the combined receipt discloses
    /// both attempts.
    #[test]
    fn partial_rerun_reads_each_shards_latest_attempt() -> Result<(), String> {
        let root = crate::tests::temp_dir("verify-shards-partial-rerun");
        // Attempt 1: shard 1 green, shard 2 red (its status is overwritten by
        // the attempt-2 artifact below, which selection must prefer).
        lay_out_shard(&root, false, 1, 2, 1, &["b one", "b two"])?;
        lay_out_shard(&root, false, 2, 2, 1, &["b three"])?;
        let _ = fs::write(shard_dir(&root, 2).join("shard-2.status"), "1\n");
        lay_out_shard(&root, false, 2, 2, 2, &["b three"])?;
        let outcome = verify(&options_attempt(&root, 2, "2"));
        assert!(outcome.problems.is_empty(), "{:?}", outcome.problems);
        assert!(
            outcome
                .summary
                .contains("shard 1: listed 2, junit tests 2 (attempt 1)"),
            "{}",
            outcome.summary
        );
        assert!(
            outcome
                .summary
                .contains("shard 2: listed 1, junit tests 1 (attempt 2)"),
            "{}",
            outcome.summary
        );
        assert!(
            outcome.context.contains("verified_attempts=1,2"),
            "{}",
            outcome.context
        );
        let _ = fs::remove_dir_all(&root);
        Ok(())
    }

    /// An artifact from an attempt after the verifying attempt is not
    /// evidence: it cannot exist for this run, so the shard reads as missing
    /// rather than passing on a future artifact.
    #[test]
    fn a_future_attempt_is_not_evidence() -> Result<(), String> {
        let root = crate::tests::temp_dir("verify-shards-future-attempt");
        lay_out_shard(&root, false, 1, 2, 1, &["b one", "b two"])?;
        lay_out_shard(&root, false, 2, 2, 3, &["b three"])?;
        let outcome = verify(&options_attempt(&root, 2, "2"));
        assert!(
            outcome
                .problems
                .iter()
                .any(|problem| problem.contains("artifact is missing")),
            "{:?}",
            outcome.problems
        );
        let _ = fs::remove_dir_all(&root);
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
        let root = crate::tests::temp_dir("verify-shards-supersede");
        lay_out(&root, false)?;
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
            SHA,
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
