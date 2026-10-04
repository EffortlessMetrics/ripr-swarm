//! `cargo xtask pilot-ranking`: a checked-in answer key for `ripr pilot`'s
//! top recommendations.
//!
//! `benchmarks/pilot_ranking/manifest.json` pins five small crates to exact
//! commits, and `benchmarks/pilot_ranking/labels/<id>.json` holds the outcome
//! of every viable cargo-mutants mutant at each pin. `score` runs pilot on
//! fetched checkouts and judges each top pick with the mutation spot check's
//! judge (`mutation_spot_check::pilot`), so a ranking change is measured in
//! seconds instead of a multi-hour cargo-mutants run. Labels belong to mutant
//! locations, not seams, so seams pilot did not rank before are judged too.
//!
//! - `check` (offline) validates the manifest and every label file.
//! - `fetch --allow-network` shallow-fetches each pinned commit under
//!   `target/ripr/pilot-ranking/checkouts`.
//! - `label` rebuilds one label file from a cargo-mutants `mutants.out`
//!   directory, after checking every mutant diff against the pinned checkout.
//! - `score` writes `target/ripr/reports/pilot-ranking.{json,md}`: precision
//!   and function diversity of pilot's top 5 and top 10, pooled and per crate.
//!   `cargo xtask dx-scoreboard --boards ranking --ingest` gates it.

use super::mutation_spot_check::{self, pilot};
use crate::run::run_output_owned_with_timeout;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

const DEFAULT_MANIFEST: &str = "benchmarks/pilot_ranking/manifest.json";
const DEFAULT_ROOT: &str = "target/ripr/pilot-ranking/checkouts";
const SCRATCH: &str = "target/ripr/pilot-ranking/runs";
const MANIFEST_SCHEMA_VERSION: &str = "ripr-pilot-ranking-corpus-v1";
const LABELS_SCHEMA_VERSION: &str = "ripr-pilot-ranking-labels-v1";
pub(crate) const RECEIPT_SCHEMA_VERSION: &str = "ripr-pilot-ranking-v1";
/// Written inside `.git` by `fetch`, so only checkouts this command created
/// are ever replaced.
const OWNER_MARKER: &str = "ripr-pilot-ranking-checkout";
const GIT_TIMEOUT: Duration = Duration::from_mins(10);

const USAGE: &str = "usage: cargo xtask pilot-ranking check [--manifest <path>]
       cargo xtask pilot-ranking fetch --allow-network [--manifest <path>] [--root <dir>] [--repo <id>]...
       cargo xtask pilot-ranking label --repo <id>=<checkout> --mutants-out <id>=<mutants.out dir> [--mutants-arg <arg>]... [--manifest <path>]
       cargo xtask pilot-ranking score [--manifest <path>] [--root <dir>] [--repo <id>]... [--ripr <binary>]

check validates the pinned manifest and its label files without network.
fetch shallow-fetches each pinned commit into <root>/<id> (default target/ripr/pilot-ranking/checkouts).
label rewrites benchmarks/pilot_ranking/labels/<id>.json from one cargo-mutants run on the pinned checkout.
score runs `ripr pilot` on each checkout (default binary: a fresh release build), judges its top picks
against the labels, and writes target/ripr/reports/pilot-ranking.{json,md}.";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema_version: String,
    corpus_version: String,
    description: String,
    label_rule: String,
    limits: Vec<String>,
    max_seams: usize,
    top_k: Vec<usize>,
    repos: Vec<RepoEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RepoEntry {
    id: String,
    url: String,
    revision: String,
    license: String,
    labels: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct LabelFile {
    schema_version: String,
    repo: String,
    revision: String,
    cargo_mutants_version: String,
    cargo_mutants_args: Vec<String>,
    /// Mutants with no caught/missed outcome (`timeout`, `unviable`), counted
    /// so the label set's coverage of the run stays visible.
    unlabeled: BTreeMap<String, u64>,
    mutants: Vec<Label>,
}

/// One viable mutant's location and outcome: the facts the judge reads from
/// cargo-mutants' `mutants.json` and `outcomes.json`.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Label {
    name: String,
    genre: String,
    file: String,
    line: u64,
    column: u64,
    /// First and last line of the function cargo-mutants says contains the
    /// mutant, when it reports one.
    function: Option<[u64; 2]>,
    outcome: String,
}

#[derive(Debug, Default)]
struct Options {
    manifest: Option<PathBuf>,
    root: Option<PathBuf>,
    repos: Vec<String>,
    allow_network: bool,
    ripr: Option<PathBuf>,
    label_checkout: Option<(String, PathBuf)>,
    label_mutants_out: Option<(String, PathBuf)>,
    /// Arguments the labeled cargo-mutants run used, recorded in the label
    /// file (the run's outcomes.json does not record them).
    label_mutants_args: Vec<String>,
}

pub(crate) fn pilot_ranking(args: &[String]) -> Result<(), String> {
    let Some((command, rest)) = args.split_first() else {
        return Err(USAGE.to_string());
    };
    if matches!(command.as_str(), "--help" | "-h" | "help") {
        println!("{USAGE}");
        return Ok(());
    }
    let options = parse_options(command, rest)?;
    let manifest_path = options
        .manifest
        .clone()
        .unwrap_or_else(|| PathBuf::from(DEFAULT_MANIFEST));
    let manifest = load_manifest(&manifest_path)?;
    let labels_dir = manifest_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default();
    match command.as_str() {
        "check" => {
            let labels = load_all_labels(&manifest, &labels_dir)?;
            let total: usize = labels.iter().map(|labels| labels.mutants.len()).sum();
            println!(
                "pilot-ranking: corpus {} ok: {} repositories, {total} labeled mutants",
                manifest.corpus_version,
                manifest.repos.len()
            );
            Ok(())
        }
        "fetch" => fetch(&manifest, &options),
        "label" => label(&manifest, &labels_dir, &options),
        "score" => score(&manifest, &labels_dir, &options),
        other => Err(format!(
            "unknown pilot-ranking subcommand `{other}`\n{USAGE}"
        )),
    }
}

fn parse_options(command: &str, args: &[String]) -> Result<Options, String> {
    let mut options = Options::default();
    let mut index = 0;
    while index < args.len() {
        let flag = args[index].as_str();
        let value = || {
            args.get(index + 1)
                .cloned()
                .ok_or_else(|| format!("{flag} needs a value\n{USAGE}"))
        };
        match flag {
            "--manifest" => options.manifest = Some(PathBuf::from(value()?)),
            "--root" => options.root = Some(PathBuf::from(value()?)),
            "--ripr" => options.ripr = Some(PathBuf::from(value()?)),
            "--allow-network" => {
                options.allow_network = true;
                index += 1;
                continue;
            }
            "--repo" if command == "label" => {
                options.label_checkout = Some(named_path(&value()?, flag)?);
            }
            "--repo" => options.repos.push(value()?),
            "--mutants-out" => options.label_mutants_out = Some(named_path(&value()?, flag)?),
            "--mutants-arg" => options.label_mutants_args.push(value()?),
            _ => return Err(format!("unknown pilot-ranking option `{flag}`\n{USAGE}")),
        }
        index += 2;
    }
    if command == "fetch" && !options.allow_network {
        return Err(
            "pilot-ranking fetch clones external repositories; pass --allow-network to opt in"
                .to_string(),
        );
    }
    Ok(options)
}

fn named_path(value: &str, flag: &str) -> Result<(String, PathBuf), String> {
    match value.split_once('=') {
        Some((name, path)) if !name.is_empty() && !path.is_empty() => {
            Ok((name.to_string(), PathBuf::from(path)))
        }
        _ => Err(format!("{flag} expects <id>=<path>, got `{value}`")),
    }
}

fn load_manifest(path: &Path) -> Result<Manifest, String> {
    let text = fs::read_to_string(path)
        .map_err(|err| format!("read pilot-ranking manifest {}: {err}", path.display()))?;
    let manifest: Manifest = serde_json::from_str(&text)
        .map_err(|err| format!("parse pilot-ranking manifest {}: {err}", path.display()))?;
    validate_manifest(&manifest).map_err(|err| format!("{}: {err}", path.display()))?;
    Ok(manifest)
}

fn validate_manifest(manifest: &Manifest) -> Result<(), String> {
    if manifest.schema_version != MANIFEST_SCHEMA_VERSION {
        return Err(format!(
            "schema_version must be `{MANIFEST_SCHEMA_VERSION}`, got `{}`",
            manifest.schema_version
        ));
    }
    for (field, text) in [
        ("corpus_version", &manifest.corpus_version),
        ("description", &manifest.description),
        ("label_rule", &manifest.label_rule),
    ] {
        if text.trim().is_empty() {
            return Err(format!("{field} must not be empty"));
        }
    }
    if manifest.limits.is_empty() {
        return Err("limits must state at least one claim limit".to_string());
    }
    if manifest.max_seams == 0 {
        return Err("max_seams must be positive".to_string());
    }
    let ascending = manifest.top_k.windows(2).all(|pair| pair[0] < pair[1]);
    if manifest.top_k.is_empty()
        || !ascending
        || manifest
            .top_k
            .iter()
            .any(|k| *k == 0 || *k > manifest.max_seams)
    {
        return Err(format!(
            "top_k must be ascending positive cutoffs no larger than max_seams {}",
            manifest.max_seams
        ));
    }
    if manifest.repos.is_empty() {
        return Err("repos must not be empty".to_string());
    }
    let mut ids = BTreeSet::new();
    for repo in &manifest.repos {
        if !is_repo_id(&repo.id) {
            return Err(format!(
                "repo id `{}` must be lowercase letters, digits and dashes",
                repo.id
            ));
        }
        if !ids.insert(repo.id.as_str()) {
            return Err(format!("duplicate repo id `{}`", repo.id));
        }
        if !is_full_sha(&repo.revision) {
            return Err(format!(
                "repo `{}` must pin a full 40-character lowercase commit sha",
                repo.id
            ));
        }
        if !repo.url.starts_with("https://") || repo.license.trim().is_empty() {
            return Err(format!(
                "repo `{}` needs an https url and a license",
                repo.id
            ));
        }
        if repo.labels != format!("labels/{}.json", repo.id) {
            return Err(format!(
                "repo `{}` labels must be `labels/{}.json`",
                repo.id, repo.id
            ));
        }
    }
    Ok(())
}

fn is_repo_id(id: &str) -> bool {
    !id.is_empty()
        && !id.starts_with('-')
        && id
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-')
}

fn is_full_sha(value: &str) -> bool {
    value.len() == 40
        && value
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

fn select<'a>(manifest: &'a Manifest, only: &[String]) -> Result<Vec<&'a RepoEntry>, String> {
    for id in only {
        if !manifest.repos.iter().any(|repo| &repo.id == id) {
            return Err(format!(
                "--repo `{id}` is not in the pilot-ranking manifest"
            ));
        }
    }
    Ok(manifest
        .repos
        .iter()
        .filter(|repo| only.is_empty() || only.contains(&repo.id))
        .collect())
}

fn load_all_labels(manifest: &Manifest, labels_dir: &Path) -> Result<Vec<LabelFile>, String> {
    manifest
        .repos
        .iter()
        .map(|repo| load_labels(repo, labels_dir))
        .collect()
}

fn load_labels(repo: &RepoEntry, labels_dir: &Path) -> Result<LabelFile, String> {
    let path = labels_dir.join(&repo.labels);
    let text = fs::read_to_string(&path).map_err(|err| {
        format!(
            "read labels {}: {err}; rebuild them with `cargo xtask pilot-ranking label`",
            path.display()
        )
    })?;
    let labels: LabelFile = serde_json::from_str(&text)
        .map_err(|err| format!("parse labels {}: {err}", path.display()))?;
    validate_labels(repo, &labels).map_err(|err| format!("{}: {err}", path.display()))?;
    Ok(labels)
}

fn validate_labels(repo: &RepoEntry, labels: &LabelFile) -> Result<(), String> {
    if labels.schema_version != LABELS_SCHEMA_VERSION {
        return Err(format!(
            "schema_version must be `{LABELS_SCHEMA_VERSION}`, got `{}`",
            labels.schema_version
        ));
    }
    if labels.repo != repo.id || labels.revision != repo.revision {
        return Err(format!(
            "labels are for {}@{}, but the manifest pins {}@{}; relabel at the pinned revision",
            labels.repo, labels.revision, repo.id, repo.revision
        ));
    }
    if labels.cargo_mutants_version.trim().is_empty() {
        return Err("cargo_mutants_version must not be empty".to_string());
    }
    if labels.mutants.is_empty() {
        return Err("mutants must not be empty".to_string());
    }
    let mut names = BTreeSet::new();
    for mutant in &labels.mutants {
        if !matches!(mutant.outcome.as_str(), "caught" | "missed") {
            return Err(format!(
                "mutant `{}` has outcome `{}`; only caught and missed are labels",
                mutant.name, mutant.outcome
            ));
        }
        if mutant.line == 0 || mutant.file.is_empty() || mutant.genre.is_empty() {
            return Err(format!(
                "mutant `{}` needs a file, a genre and a 1-based line",
                mutant.name
            ));
        }
        if let Some([start, end]) = mutant.function
            && (start == 0 || start > end)
        {
            return Err(format!(
                "mutant `{}` has function lines {start}..{end}",
                mutant.name
            ));
        }
        if !names.insert(mutant.name.as_str()) {
            return Err(format!("duplicate mutant `{}`", mutant.name));
        }
    }
    if !labels
        .mutants
        .windows(2)
        .all(|pair| label_key(&pair[0]) < label_key(&pair[1]))
    {
        return Err(
            "mutants must be sorted by file, line, column and name; rebuild with `cargo xtask pilot-ranking label`"
                .to_string(),
        );
    }
    Ok(())
}

fn label_key(label: &Label) -> (&str, u64, u64, &str) {
    (&label.file, label.line, label.column, &label.name)
}

// ---------------------------------------------------------------- fetch ----

fn fetch(manifest: &Manifest, options: &Options) -> Result<(), String> {
    let root = options
        .root
        .clone()
        .unwrap_or_else(|| PathBuf::from(DEFAULT_ROOT));
    fs::create_dir_all(&root).map_err(|err| format!("create {}: {err}", root.display()))?;
    let mut failures = Vec::new();
    for repo in select(manifest, &options.repos)? {
        let dir = root.join(&repo.id);
        match materialize(repo, &dir) {
            Ok(reused) => eprintln!(
                "pilot-ranking: {} @ {} {}",
                repo.id,
                &repo.revision[..12],
                if reused { "(reused)" } else { "(fetched)" }
            ),
            Err(err) => {
                eprintln!("pilot-ranking: {} failed: {err}", repo.id);
                failures.push(format!("{}: {err}", repo.id));
            }
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "pilot-ranking fetch failed for {} repo(s):\n  - {}",
            failures.len(),
            failures.join("\n  - ")
        ))
    }
}

fn materialize(repo: &RepoEntry, dir: &Path) -> Result<bool, String> {
    if checkout_problem(repo, dir).is_none() {
        return Ok(true);
    }
    // Only an empty directory or one an earlier fetch created may be replaced;
    // a user's own clone at the same path is never deleted.
    let replaceable = !dir.exists()
        || fs::read_dir(dir).is_ok_and(|mut entries| entries.next().is_none())
        || dir.join(".git").join(OWNER_MARKER).is_file();
    if !replaceable {
        return Err(format!(
            "{} exists and was not created by pilot-ranking fetch; move it or pass a different --root",
            dir.display()
        ));
    }
    if dir.exists() {
        fs::remove_dir_all(dir).map_err(|err| format!("remove {}: {err}", dir.display()))?;
    }
    fs::create_dir_all(dir).map_err(|err| format!("create {}: {err}", dir.display()))?;
    git(dir, &["init", "--quiet"])?;
    // Mark the directory as ours before any network step, so a fetch that
    // fails halfway can be retried instead of refusing its own leftovers.
    // The recheck below still keeps a bad checkout from being reused.
    let marker = dir.join(".git").join(OWNER_MARKER);
    fs::write(&marker, format!("{}\n", repo.url))
        .map_err(|err| format!("write {}: {err}", marker.display()))?;
    git(dir, &["remote", "add", "origin", &repo.url])?;
    git(
        dir,
        &[
            "fetch",
            "--quiet",
            "--depth",
            "1",
            "--no-tags",
            "origin",
            &repo.revision,
        ],
    )?;
    git(dir, &["checkout", "--quiet", "--detach", "FETCH_HEAD"])?;
    if let Some(problem) = checkout_problem(repo, dir) {
        return Err(problem);
    }
    Ok(false)
}

/// Why `dir` is not the pinned subject, or `None` when it is. pilot reads
/// the working tree, so a tracked edit or an added source file would be
/// scored under the pin's name. Ignored files (a build's `target/`) are fine.
fn checkout_problem(repo: &RepoEntry, dir: &Path) -> Option<String> {
    if !dir.join(".git").exists() {
        return Some(format!("{} is not a git checkout", dir.display()));
    }
    match git(dir, &["rev-parse", "HEAD"]) {
        Ok(head) if head == repo.revision => {}
        Ok(head) => {
            return Some(format!(
                "checkout HEAD {head} does not match pinned revision {}",
                repo.revision
            ));
        }
        Err(err) => return Some(err),
    }
    match git(dir, &["status", "--porcelain", "--untracked-files=normal"]) {
        Ok(status) if status.is_empty() => None,
        Ok(_) => Some("the working tree has modified or untracked files".to_string()),
        Err(err) => Some(err),
    }
}

fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let mut owned = vec!["-C".to_string(), dir.to_string_lossy().into_owned()];
    owned.extend(args.iter().map(|arg| (*arg).to_string()));
    let context = format!("pilot-ranking git {}", args.first().copied().unwrap_or(""));
    run_output_owned_with_timeout("git", &owned, GIT_TIMEOUT, &context)
        .map(|stdout| stdout.trim().to_string())
}

// ---------------------------------------------------------------- label ----

fn label(manifest: &Manifest, labels_dir: &Path, options: &Options) -> Result<(), String> {
    let (Some((id, checkout)), Some((out_id, mutants_out))) =
        (&options.label_checkout, &options.label_mutants_out)
    else {
        return Err(format!(
            "label needs --repo <id>=<checkout> and --mutants-out <id>=<dir>\n{USAGE}"
        ));
    };
    if id != out_id {
        return Err(format!(
            "--repo names `{id}` but --mutants-out names `{out_id}`"
        ));
    }
    let repo = select(manifest, std::slice::from_ref(id))?
        .into_iter()
        .next()
        .ok_or_else(|| format!("`{id}` is not in the manifest"))?;
    if let Some(problem) = checkout_problem(repo, checkout) {
        return Err(format!("--repo {id}: {problem}"));
    }
    let read = |file: &str| mutation_spot_check::read_json(&mutants_out.join(file));
    let mutants = read("mutants.json")?;
    let outcomes = read("outcomes.json")?;
    mutation_spot_check::require_mutants_match_checkout(id, checkout, &repo.revision, &mutants)?;
    let mut labels = labels_from_mutants_out(repo, &mutants, &outcomes)?;
    labels.cargo_mutants_args = options.label_mutants_args.clone();
    let path = labels_dir.join(&repo.labels);
    fs::write(&path, render_labels(&labels)?)
        .map_err(|err| format!("write {}: {err}", path.display()))?;
    println!(
        "pilot-ranking: wrote {} ({} labeled mutants)",
        path.display(),
        labels.mutants.len()
    );
    Ok(())
}

/// Join `mutants.json` to `outcomes.json` by mutant name. Every mutant must
/// have exactly one outcome, so a truncated or mismatched run is refused
/// rather than labeled partially.
fn labels_from_mutants_out(
    repo: &RepoEntry,
    mutants: &Value,
    outcomes: &Value,
) -> Result<LabelFile, String> {
    let mut results: BTreeMap<&str, &str> = BTreeMap::new();
    for record in outcomes
        .get("outcomes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        // The baseline run is a scenario without a mutant.
        let Some(name) = record
            .pointer("/scenario/Mutant/name")
            .and_then(Value::as_str)
        else {
            continue;
        };
        let summary = record
            .get("summary")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("outcome for `{name}` has no summary"))?;
        if results.insert(name, summary).is_some() {
            return Err(format!("outcomes.json has two outcomes for `{name}`"));
        }
    }
    let records = mutants
        .as_array()
        .filter(|records| !records.is_empty())
        .ok_or("mutants.json is not a non-empty array")?;
    let mut labels = Vec::new();
    let mut unlabeled: BTreeMap<String, u64> = BTreeMap::new();
    for record in records {
        let text = |pointer: &str| record.pointer(pointer).and_then(Value::as_str);
        let number = |pointer: &str| record.pointer(pointer).and_then(Value::as_u64);
        let name = text("/name").ok_or("a mutant in mutants.json has no name")?;
        let summary = results
            .remove(name)
            .ok_or_else(|| format!("mutant `{name}` has no outcome in outcomes.json"))?;
        let outcome = match summary {
            "CaughtMutant" => "caught",
            "MissedMutant" => "missed",
            "Timeout" => {
                *unlabeled.entry("timeout".to_string()).or_default() += 1;
                continue;
            }
            "Unviable" => {
                *unlabeled.entry("unviable".to_string()).or_default() += 1;
                continue;
            }
            other => return Err(format!("mutant `{name}` has unexpected outcome `{other}`")),
        };
        labels.push(Label {
            name: name.to_string(),
            genre: text("/genre").unwrap_or_default().to_string(),
            file: text("/file")
                .ok_or_else(|| format!("mutant `{name}` has no file"))?
                .to_string(),
            line: number("/span/start/line")
                .ok_or_else(|| format!("mutant `{name}` has no start line"))?,
            column: number("/span/start/column").unwrap_or(0),
            function: number("/function/span/start/line")
                .zip(number("/function/span/end/line"))
                .map(|(start, end)| [start, end]),
            outcome: outcome.to_string(),
        });
    }
    if let Some(name) = results.keys().next() {
        return Err(format!(
            "outcomes.json has `{name}`, which mutants.json does not list"
        ));
    }
    labels.sort_by(|a, b| label_key(a).cmp(&label_key(b)));
    let labels = LabelFile {
        schema_version: LABELS_SCHEMA_VERSION.to_string(),
        repo: repo.id.clone(),
        revision: repo.revision.clone(),
        cargo_mutants_version: outcomes
            .get("cargo_mutants_version")
            .and_then(Value::as_str)
            .ok_or("outcomes.json has no cargo_mutants_version")?
            .to_string(),
        cargo_mutants_args: Vec::new(),
        unlabeled,
        mutants: labels,
    };
    validate_labels(repo, &labels)?;
    Ok(labels)
}

/// One mutant per line keeps label diffs reviewable when a pin moves.
fn render_labels(labels: &LabelFile) -> Result<String, String> {
    let header = json!({
        "schema_version": labels.schema_version,
        "repo": labels.repo,
        "revision": labels.revision,
        "cargo_mutants_version": labels.cargo_mutants_version,
        "cargo_mutants_args": labels.cargo_mutants_args,
        "unlabeled": labels.unlabeled,
    });
    let header =
        serde_json::to_string_pretty(&header).map_err(|err| format!("render labels: {err}"))?;
    // Reopen the header object exactly once to append the mutant list.
    let mut out = header
        .strip_suffix("\n}")
        .ok_or("render labels: header is not a JSON object")?
        .to_string();
    out.push_str(",\n  \"mutants\": [\n");
    for (index, mutant) in labels.mutants.iter().enumerate() {
        let line = serde_json::to_string(mutant).map_err(|err| format!("render label: {err}"))?;
        out.push_str("    ");
        out.push_str(&line);
        out.push_str(if index + 1 < labels.mutants.len() {
            ",\n"
        } else {
            "\n"
        });
    }
    out.push_str("  ]\n}\n");
    Ok(out)
}

/// The labels in the `mutants.json` and `outcomes.json` shapes the shared
/// judge reads.
fn judge_inputs(labels: &LabelFile) -> (Value, Value) {
    let mutants = labels
        .mutants
        .iter()
        .map(|label| {
            let mut record = json!({
                "name": label.name,
                "genre": label.genre,
                "file": label.file,
                "span": {"start": {"line": label.line, "column": label.column}},
            });
            if let Some([start, end]) = label.function {
                record["function"] =
                    json!({"span": {"start": {"line": start}, "end": {"line": end}}});
            }
            record
        })
        .collect::<Vec<_>>();
    let outcomes = labels
        .mutants
        .iter()
        .map(|label| {
            json!({
                "scenario": {"Mutant": {"name": label.name}},
                "summary": if label.outcome == "missed" { "MissedMutant" } else { "CaughtMutant" },
            })
        })
        .collect::<Vec<_>>();
    (Value::Array(mutants), json!({"outcomes": outcomes}))
}

// ---------------------------------------------------------------- score ----

fn score(manifest: &Manifest, labels_dir: &Path, options: &Options) -> Result<(), String> {
    let root = options
        .root
        .clone()
        .unwrap_or_else(|| PathBuf::from(DEFAULT_ROOT));
    let binary = match &options.ripr {
        Some(path) => path.clone(),
        None => {
            crate::run::run("cargo", &["build", "-p", "ripr", "--release", "--quiet"])?;
            std::env::var_os("CARGO_TARGET_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("target"))
                .join("release")
                .join(format!("ripr{}", std::env::consts::EXE_SUFFIX))
        }
    };
    let scratch = PathBuf::from(SCRATCH);
    if scratch.exists() {
        fs::remove_dir_all(&scratch)
            .map_err(|err| format!("clear {}: {err}", scratch.display()))?;
    }
    fs::create_dir_all(&scratch).map_err(|err| format!("create {}: {err}", scratch.display()))?;
    let selected = select(manifest, &options.repos)?;
    let mut repos = Vec::new();
    for repo in &manifest.repos {
        // A --repo subset is a different population than the pooled
        // baseline, so the crates left out count as unavailable and the
        // receipt is incomplete rather than comparable.
        if !selected.iter().any(|chosen| chosen.id == repo.id) {
            repos.push(RepoScore {
                id: repo.id.clone(),
                revision: repo.revision.clone(),
                judged: Err("not selected by --repo".to_string()),
            });
            continue;
        }
        let labels = load_labels(repo, labels_dir)?;
        let checkout = root.join(&repo.id);
        let judged = match checkout_problem(repo, &checkout) {
            Some(problem) => Err(format!(
                "{problem}; run `cargo xtask pilot-ranking fetch --allow-network --repo {}`",
                repo.id
            )),
            None => score_repo(&binary, &scratch, manifest, repo, &checkout, &labels),
        };
        if let Err(reason) = &judged {
            eprintln!("pilot-ranking: {} unavailable: {reason}", repo.id);
        }
        repos.push(RepoScore {
            id: repo.id.clone(),
            revision: repo.revision.clone(),
            judged,
        });
    }
    let report = build_report(manifest, &binary, &repos);
    let json_text = serde_json::to_string_pretty(&report)
        .map_err(|err| format!("serialize pilot-ranking report: {err}"))?;
    crate::write_report("pilot-ranking.json", &format!("{json_text}\n"))?;
    crate::write_report("pilot-ranking.md", &markdown(&report))?;
    println!("Wrote target/ripr/reports/pilot-ranking.json");
    println!("Wrote target/ripr/reports/pilot-ranking.md");
    Ok(())
}

struct RepoScore {
    id: String,
    revision: String,
    /// Judged recommendations in pilot's rank order, or why there are none.
    judged: Result<Vec<Value>, String>,
}

fn score_repo(
    binary: &Path,
    scratch: &Path,
    manifest: &Manifest,
    repo: &RepoEntry,
    checkout: &Path,
    labels: &LabelFile,
) -> Result<Vec<Value>, String> {
    let (top, exposure) =
        pilot::run_pilot(binary, scratch, &repo.id, checkout, manifest.max_seams)?;
    let (mutants, outcomes) = judge_inputs(labels);
    let judged = pilot::judge_recommendations(
        &top,
        &mutants,
        &outcomes,
        &mutation_spot_check::seam_expressions(&exposure),
        &|file, line| {
            let index = usize::try_from(line).ok()?.checked_sub(1)?;
            fs::read_to_string(checkout.join(file))
                .ok()?
                .lines()
                .nth(index)
                .map(str::to_string)
        },
    );
    // The judge keeps location and verdict; diversity also needs the owner.
    Ok(judged
        .into_iter()
        .zip(&top)
        .map(|(mut row, seam)| {
            row["owner"] = seam.get("owner").cloned().unwrap_or(Value::Null);
            row
        })
        .collect())
}

/// Confirmed, refuted and unscored picks plus distinct functions within the
/// first `k` recommendations of each judged list.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct Cut {
    picks: usize,
    confirmed: usize,
    refuted: usize,
    unscored: usize,
    distinct_functions: usize,
}

impl Cut {
    fn of(judged: &[Value], k: usize) -> Self {
        let mut cut = Cut::default();
        let mut owners = BTreeSet::new();
        for row in judged.iter().take(k) {
            cut.picks += 1;
            match row.get("verdict").and_then(Value::as_str) {
                Some("confirmed") => cut.confirmed += 1,
                Some("refuted") => cut.refuted += 1,
                _ => cut.unscored += 1,
            }
            // A pick without an owner counts as its own function, so a
            // missing field can only understate repetition, never hide it.
            let owner = row
                .get("owner")
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| {
                    format!(
                        "{}:{}",
                        row.get("file").and_then(Value::as_str).unwrap_or(""),
                        row.get("line").and_then(Value::as_u64).unwrap_or(0)
                    )
                });
            owners.insert(owner);
        }
        cut.distinct_functions = owners.len();
        cut
    }

    fn add(self, other: Self) -> Self {
        Cut {
            picks: self.picks + other.picks,
            confirmed: self.confirmed + other.confirmed,
            refuted: self.refuted + other.refuted,
            unscored: self.unscored + other.unscored,
            distinct_functions: self.distinct_functions + other.distinct_functions,
        }
    }

    fn to_json(self) -> Value {
        let ratio = |numerator: usize, denominator: usize| {
            if denominator == 0 {
                Value::Null
            } else {
                json!(numerator as f64 / denominator as f64)
            }
        };
        json!({
            "picks": self.picks,
            "confirmed": self.confirmed,
            "refuted": self.refuted,
            "unscored": self.unscored,
            "precision": ratio(self.confirmed, self.confirmed + self.refuted),
            "scored_share": ratio(self.confirmed + self.refuted, self.picks),
            "distinct_functions": self.distinct_functions,
            "distinct_function_share": ratio(self.distinct_functions, self.picks),
        })
    }
}

fn build_report(manifest: &Manifest, binary: &Path, repos: &[RepoScore]) -> Value {
    let mut pooled: BTreeMap<usize, Cut> = BTreeMap::new();
    let mut by_tier: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
    let mut repo_rows = Vec::new();
    for repo in repos {
        match &repo.judged {
            Ok(judged) => {
                let mut cuts = serde_json::Map::new();
                for &k in &manifest.top_k {
                    let cut = Cut::of(judged, k);
                    let total = pooled.entry(k).or_default();
                    *total = total.add(cut);
                    cuts.insert(format!("top{k}"), cut.to_json());
                }
                for row in judged {
                    let field = |key: &str| row.get(key).and_then(Value::as_str).unwrap_or("");
                    *by_tier
                        .entry(field("tier").to_string())
                        .or_default()
                        .entry(field("verdict").to_string())
                        .or_default() += 1;
                }
                repo_rows.push(json!({
                    "id": repo.id,
                    "revision": repo.revision,
                    "status": "scored",
                    "cuts": cuts,
                    "recommendations": judged,
                }));
            }
            Err(reason) => repo_rows.push(json!({
                "id": repo.id,
                "revision": repo.revision,
                "status": "unavailable",
                "reason": reason,
            })),
        }
    }
    let unavailable = repos.iter().filter(|repo| repo.judged.is_err()).count();
    json!({
        "schema_version": RECEIPT_SCHEMA_VERSION,
        "corpus_version": manifest.corpus_version,
        "status": if unavailable == 0 { "complete" } else { "incomplete" },
        "ripr_binary": binary.to_string_lossy(),
        "max_seams": manifest.max_seams,
        "top_k": manifest.top_k,
        "claim_boundary": pilot::CLAIM_BOUNDARY,
        "repos_total": repos.len(),
        "unavailable_repos": unavailable,
        "pooled": pooled
            .into_iter()
            .map(|(k, cut)| (format!("top{k}"), cut.to_json()))
            .collect::<serde_json::Map<_, _>>(),
        "by_tier": by_tier,
        "repos": repo_rows,
    })
}

fn markdown(report: &Value) -> String {
    let percent = |value: &Value| {
        value
            .as_f64()
            .map_or_else(|| "n/a".to_string(), |rate| format!("{:.1}%", rate * 100.0))
    };
    let mut out = format!(
        "# Pilot ranking answer key\n\nCorpus {}, status {}. {}\n\nPrecision is confirmed over confirmed plus refuted. Scored share is the picks a labeled mutant could judge. Distinct functions counts each pick's owning function once.\n\n| Cut | Picks | Confirmed | Refuted | Unscored | Precision | Scored share | Distinct functions |\n| --- | --- | --- | --- | --- | --- | --- | --- |\n",
        report["corpus_version"].as_str().unwrap_or(""),
        report["status"].as_str().unwrap_or(""),
        report["claim_boundary"].as_str().unwrap_or(""),
    );
    let row = |label: &str, cut: &Value| {
        format!(
            "| {label} | {} | {} | {} | {} | {} | {} | {} |\n",
            cut["picks"],
            cut["confirmed"],
            cut["refuted"],
            cut["unscored"],
            percent(&cut["precision"]),
            percent(&cut["scored_share"]),
            cut["distinct_functions"],
        )
    };
    // JSON objects sort their keys, so `top10` would precede `top5`; follow
    // the manifest's cutoff order instead.
    let cut_names = report["top_k"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_u64)
        .map(|k| format!("top{k}"))
        .collect::<Vec<_>>();
    for cut_name in &cut_names {
        out.push_str(&row(
            &format!("all, {cut_name}"),
            &report["pooled"][cut_name],
        ));
    }
    for repo in report["repos"].as_array().into_iter().flatten() {
        let id = repo["id"].as_str().unwrap_or("");
        match repo["cuts"].as_object() {
            Some(cuts) => {
                for cut_name in &cut_names {
                    if let Some(cut) = cuts.get(cut_name) {
                        out.push_str(&row(&format!("{id}, {cut_name}"), cut));
                    }
                }
            }
            None => {
                let reason = repo["reason"]
                    .as_str()
                    .unwrap_or("")
                    .lines()
                    .next()
                    .unwrap_or("")
                    .replace('|', "\\|");
                out.push_str(&format!("| {id} | unavailable: {reason} | | | | | | |\n"));
            }
        }
    }
    out.push_str("\n## Recommendations\n\n| Repo | Rank | Seam | Owner | Grip class | Verdict | Tier | Caught | Missed |\n| --- | --- | --- | --- | --- | --- | --- | --- | --- |\n");
    for repo in report["repos"].as_array().into_iter().flatten() {
        let id = repo["id"].as_str().unwrap_or("");
        for pick in repo["recommendations"].as_array().into_iter().flatten() {
            let text = |key: &str| pick[key].as_str().unwrap_or("");
            out.push_str(&format!(
                "| {id} | {} | `{}:{}` | `{}` | {} | {} | {} | {} | {} |\n",
                pick["rank"],
                text("file"),
                pick["line"],
                text("owner"),
                text("grip_class"),
                text("verdict"),
                text("tier"),
                pick["caught"],
                pick["missed"],
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests;
