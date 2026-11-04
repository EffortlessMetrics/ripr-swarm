//! Cache-invalidation generation registry and merge-base pin gate
//! (#11736 Proposal C).
//!
//! The cache-invalidation generation constants are the collision surface when
//! concurrent lanes bump generations: each value is a component of the cache
//! entry key, so two analyzer binaries sharing one generation can serve each
//! other's stale cache entries silently. The evidence items (#10326 — a
//! collision that reached qualification, #10526 — renumbered twice mid-flight,
//! #10587 — a claim older than main, #10845 — the manual read-renumber-reprove
//! ritual) are all the same failure.
//!
//! This module is the issue's minimal pair: `generations next` prints the next
//! free value per family so every lane allocates identically, and
//! `check-generation-pins` turns a silent collision into a red gate. The
//! predicate is deliberately *strictly greater* (skipped numbers are legal
//! precedent, e.g. `1.129 -> 1.131`), a PR that did not touch a constant
//! passes unconditionally (unrelated main movement invalidates nothing), and
//! every unresolvable or unparseable state fails closed instead of passing.
//!
//! Known limit: on a hosted checkout of GitHub's synthetic pull-request merge
//! ref, `git merge-base origin/main HEAD` returns the base tip rather than the
//! PR branch's original fork, so a PR whose claimed value is identical to a
//! value a sibling already landed can read as "untouched". The merge-queue
//! qualification of the real merge composition is the authority for that
//! interaction; this local/PR gate is a shift-left check, not the final word.
//!
//! The gate compares registry constants only — never fixture values (the
//! historical 1.74/1.78/1.119 pins in the stale-cache-rejection fixtures are
//! correct by design) and never the contract schema versions, whose collision
//! semantics differ (#11736 Proposal C non-goals; a later registry extension
//! is a row addition, not part of this claim).
//!
//! On an ordinary checkout the claim surface is the working tree and the fork
//! is `git merge-base <base> HEAD`. On GitHub's synthetic PR merge ref
//! (`refs/pull/<n>/merge`) HEAD is a merge commit whose first parent is the
//! base tip; evaluating HEAD there would alias fork and base tip and hide the
//! sibling-collision class, so the gate detects that shape and evaluates the
//! PR head tree (the merge commit's second parent) against its true fork
//! instead.

use std::cmp::Ordering;
use std::path::Path;

use crate::{FixKind, PolicyReportSpec, finish_policy_report, repo_root};

/// One registry row: the family identity, the file that defines the
/// generation constant, and the constant name. Values are never stored here —
/// they are extracted at the exact ref being judged, so the registry is
/// value-independent and never churns on a bump.
struct GenerationPin {
    family: &'static str,
    defining_file: &'static str,
    constant: &'static str,
}

/// The ten cache-invalidation generation constants observed at
/// `origin/main` (re-verified 2026-11-02 at `be9d00de2d`: main had moved from
/// the research snapshot 1.137/0.139/1.52 to 1.142/0.144/1.54 — the exact
/// churn this gate exists to serialize). The compact and sharded classified
/// seam constants are one allocation unit (they move in lockstep as the
/// "0.x" family) but stay separate rows so a drifted lockstep is visible.
const GENERATION_PINS: &[GenerationPin] = &[
    GenerationPin {
        family: "classified_seam_cache",
        defining_file: "crates/ripr/src/analysis/seam_cache.rs",
        constant: "CACHE_SCHEMA_VERSION",
    },
    GenerationPin {
        family: "sharded_classified_seam_cache",
        defining_file: "crates/ripr/src/analysis/seam_cache.rs",
        constant: "SHARDED_CLASSIFIED_SEAM_CACHE_SCHEMA_VERSION",
    },
    GenerationPin {
        family: "compact_classified_seam_cache",
        defining_file: "crates/ripr/src/analysis/seam_cache.rs",
        constant: "COMPACT_CLASSIFIED_SEAM_CACHE_SCHEMA_VERSION",
    },
    GenerationPin {
        family: "count_cache",
        defining_file: "crates/ripr/src/analysis/seam_cache.rs",
        constant: "COUNT_CACHE_SCHEMA_VERSION",
    },
    GenerationPin {
        family: "file_fact_cache",
        defining_file: "crates/ripr/src/analysis/seam_cache.rs",
        constant: "FILE_FACT_CACHE_SCHEMA_VERSION",
    },
    GenerationPin {
        family: "corpus_fingerprint_cache",
        defining_file: "crates/ripr/src/analysis/seam_cache.rs",
        constant: "CORPUS_FINGERPRINT_CACHE_SCHEMA_VERSION",
    },
    GenerationPin {
        family: "module_tree_scan_cache",
        defining_file: "crates/ripr/src/analysis/seam_cache.rs",
        constant: "MODULE_TREE_SCAN_CACHE_SCHEMA_VERSION",
    },
    GenerationPin {
        family: "scoped_inventory_cache",
        defining_file: "crates/ripr/src/analysis/seam_cache/scoped_inventory.rs",
        constant: "SCOPED_INVENTORY_CACHE_SCHEMA_VERSION",
    },
    GenerationPin {
        family: "typescript_file_fact_store",
        defining_file: "crates/ripr/src/analysis/language/typescript/fact_store.rs",
        constant: "TYPESCRIPT_FILE_FACT_SCHEMA_VERSION",
    },
    GenerationPin {
        family: "python_file_fact_store",
        defining_file: "crates/ripr/src/analysis/language/python/fact_store.rs",
        constant: "PYTHON_FILE_FACT_SCHEMA_VERSION",
    },
];

/// Reads one repo-relative file either from the working tree (`reference` is
/// `None`) or as of a git ref (`Some(reference)`), returning its UTF-8 text.
/// Injected so the predicate is pure and unit-testable without spawning git
/// (same dependency-injection pattern as `run_composition_in`'s
/// `StatusRunner`, precommit_v2.rs).
type BlobReader<'a> = dyn Fn(Option<&str>, &str) -> Result<String, String> + 'a;

pub(crate) const GENERATIONS_USAGE: &str = "usage: cargo xtask generations next [--base <ref>]";

const CONSUMER_CHECKLIST: &[&str] = &[
    "Consumer checklist (refresh every consumer of a bumped family in the same PR):",
    "- literal pin tests: `schema_versions_pin_the_role_composition_generation` and",
    "  `file_fact_generation_before_property_quarantine_is_a_miss` in",
    "  crates/ripr/src/analysis/seam_cache.rs; `schema_version_pins_ast_probe_shape_facts` in",
    "  crates/ripr/src/analysis/language/typescript/fact_store.rs",
    "- the two independent CURRENT_CORPUS_CACHE_SCHEMA_VERSION copies:",
    "  xtask/src/tests/rust_corpus_projection.rs and",
    "  crates/ripr/tests/lsp_lifecycle/gap_corpus_currentness.rs",
    "- docs/OUTPUT_SCHEMA.md worked examples carrying the current value",
    "- boundary_gap expected JSON fixtures carrying the current value",
    "  (fixtures/boundary_gap/expected/**); historical fixture values are never bumped",
    "Then run `cargo xtask check-generation-pins`; the hosted rust-gates lane enforces the",
    "same strictly-greater predicate at merge time.",
];

/// `cargo xtask generations next`: print, per family, the current generation
/// at the accepted base and the next free value, plus the consumer checklist.
/// Mirrors `specs next`: allocation is a pure read; nothing is claimed or
/// reserved by running it.
pub(crate) fn generations_next(args: &[String]) -> Result<(), String> {
    let base_override = parse_generations_next_args(args)?;
    let root = repo_root()?;
    let base = resolve_base(&root, base_override.as_deref())?;
    let reader = production_blob_reader(&root);
    let mut lines = vec![format!(
        "base: {} @ {}",
        base.base_ref, base.base_tip_commit
    )];
    for pin in GENERATION_PINS {
        let current =
            read_generation(&reader, Some(base.base_ref.as_str()), pin).map_err(|error| {
                format!(
                    "cannot allocate the next `{}` generation ({fallback})",
                    pin.family,
                    fallback = fail_closed_detail(&error)
                )
            })?;
        let next = next_free(&current).map_err(|error| {
            format!(
                "cannot allocate the next `{}` generation from `{current}`: {error}",
                pin.family
            )
        })?;
        lines.push(format!(
            "{}: {} ({}) current {current} -> next free {next}",
            pin.family, pin.constant, pin.defining_file
        ));
    }
    lines.extend(CONSUMER_CHECKLIST.iter().map(|line| line.to_string()));
    println!("{}", lines.join("\n"));
    Ok(())
}

/// `cargo xtask check-generation-pins`: fail any PR whose claimed generation
/// bump is not strictly greater than the accepted base's current value.
/// A PR that did not touch a constant passes unconditionally; an
/// unresolvable base, a stale registry row, or a non-numeric value fails
/// closed.
pub(crate) fn check_generation_pins() -> Result<(), String> {
    let root = repo_root()?;
    check_generation_pins_in(&root)
}

fn check_generation_pins_in(root: &Path) -> Result<(), String> {
    let base = resolve_base(root, None)?;
    let points = resolve_evaluation_points(root, &base)?;
    let reader = production_blob_reader(root);
    let violations = claimed_generation_violations(
        &reader,
        &base.base_ref,
        &points.fork_commit,
        points.head_ref.as_deref(),
    );
    finish_policy_report(
        PolicyReportSpec {
            report_file: "generation-pins.md",
            check: "check-generation-pins",
            why_it_matters: "Cache-invalidation generations are allocated by concurrent lanes against one shared history: a PR that repins a generation already taken on main (or one main already moved past) lets a warm cache serve stale roles silently — the #10326 collision reached qualification before an audit caught it. A claimed bump must pin a value strictly greater than the accepted base's current value, so the collision becomes a red, self-explanatory gate instead of a manual read-renumber-reprove ritual (issue #11736 Proposal C).",
            fix_kind: FixKind::AuthorDecisionRequired,
            recommended_fixes: &[
                "Run `cargo xtask generations next` and pin the printed next free value for each flagged family.",
                "Refresh every consumer in the same PR: the pin tests in crates/ripr/src/analysis/seam_cache.rs and the TS fact store, the CURRENT_CORPUS_CACHE_SCHEMA_VERSION copies in xtask/src/tests/rust_corpus_projection.rs and crates/ripr/tests/lsp_lifecycle/gap_corpus_currentness.rs, the docs/OUTPUT_SCHEMA.md examples, and the boundary_gap expected JSON fixtures carrying the current value.",
                "If you did not intend a bump, restore the constant to its merge-base value so no claim is detected.",
                "Skipped numbers are legal; the requirement is strictly greater than main's current value, never contiguous.",
            ],
            rerun_command: "cargo xtask check-generation-pins",
            exception_template: None,
        },
        &violations,
    )
}

fn parse_generations_next_args(args: &[String]) -> Result<Option<String>, String> {
    let mut base = None;
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--base" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(format!("--base requires a ref value\n{GENERATIONS_USAGE}"));
                };
                base = Some(value.clone());
                index += 2;
            }
            other => {
                return Err(format!(
                    "unknown `generations next` option `{other}`\n{GENERATIONS_USAGE}"
                ));
            }
        }
    }
    Ok(base)
}

/// The accepted base for one evaluation: the ref used for tip reads, its tip
/// commit, and the merge-base between that ref and HEAD.
struct ResolvedBase {
    base_ref: String,
    base_tip_commit: String,
    head_merge_base: String,
}

/// Where to read the claim surface and the fork for one gate run. On an
/// ordinary checkout the claim surface is the working tree and the fork is
/// the merge-base of the base ref and HEAD. On GitHub's synthetic PR merge
/// ref (`refs/pull/<n>/merge`), HEAD is a merge commit whose first parent is
/// the base tip: evaluating HEAD directly would alias fork and base tip and
/// hide the sibling-collision class — a sibling that merged generation `X`
/// plus this PR claiming the same `X` produces a clean merge with
/// `head == fork == base_tip == X`, which must still fail. In that mode the
/// claim surface is the PR head tree (the merge commit's second parent) and
/// the fork is that head's true merge-base with the first parent.
struct EvaluationPoints {
    /// `None` reads the working tree; `Some(sha)` reads blobs at that commit.
    head_ref: Option<String>,
    fork_commit: String,
}

fn resolve_evaluation_points(root: &Path, base: &ResolvedBase) -> Result<EvaluationPoints, String> {
    let second_parent = git_rev_parse(root, "HEAD^2")?;
    let Some(second_parent) = second_parent else {
        return Ok(EvaluationPoints {
            head_ref: None,
            fork_commit: base.head_merge_base.clone(),
        });
    };
    let first_parent = git_rev_parse(root, "HEAD^1")?
        .ok_or_else(|| "HEAD has a second parent but no first parent".to_string())?;
    if first_parent == base.base_tip_commit {
        let fork_commit = git_merge_base_commit(root, &second_parent, &first_parent)?;
        Ok(EvaluationPoints {
            head_ref: Some(second_parent),
            fork_commit,
        })
    } else {
        // An ordinary merge commit on a branch (for example a lane merging
        // main into its own branch): the branch head stays the claim surface.
        Ok(EvaluationPoints {
            head_ref: None,
            fork_commit: base.head_merge_base.clone(),
        })
    }
}

/// The verdict for one family. `Collision` is the #10326/#10845 class (the
/// claimed value is already main's current value); `StaleClaim` is the
/// #10587 class (main moved past the claimed value).
#[derive(Clone, Debug, Eq, PartialEq)]
enum ClaimVerdict {
    NoClaim,
    FreshClaim,
    Collision,
    StaleClaim,
}

/// The pure predicate: compare the claimed value against main's tip.
/// `head == fork` means the PR did not touch the constant — no claim, pass
/// unconditionally, before any parsing (a malformed value that main itself
/// already carries is not this PR's claim). Any claimed bump must parse and
/// be strictly greater than the base tip; parse failure is an instrument
/// failure, never a pass.
fn evaluate_claim(head: &str, fork: &str, base_tip: &str) -> Result<ClaimVerdict, String> {
    if head == fork {
        return Ok(ClaimVerdict::NoClaim);
    }
    let head_value = parse_version(head)
        .map_err(|error| format!("head value `{head}` is not a usable generation: {error}"))?;
    parse_version(fork).map_err(|error| {
        format!("merge-base value `{fork}` is not a usable generation: {error}")
    })?;
    let base_tip_value = parse_version(base_tip).map_err(|error| {
        format!("base-tip value `{base_tip}` is not a usable generation: {error}")
    })?;
    match head_value.cmp(&base_tip_value) {
        Ordering::Greater => Ok(ClaimVerdict::FreshClaim),
        Ordering::Equal => Ok(ClaimVerdict::Collision),
        Ordering::Less => Ok(ClaimVerdict::StaleClaim),
    }
}

/// Extract the string literal of `const <NAME>: &str = "<value>";` from one
/// file's text. Only real declaration lines match (an optional visibility
/// prefix, then `const <NAME>:`), so doc comments, pin-test assertions, and
/// history comment blocks cannot satisfy the extraction. The registry expects
/// exactly one defining declaration per row; anything else fails closed.
fn extract_constant(text: &str, constant: &str) -> Result<String, String> {
    let declaration = format!("const {constant}:");
    let mut matches: Vec<&str> = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim_start();
        let body = trimmed
            .strip_prefix("pub(crate) ")
            .or_else(|| trimmed.strip_prefix("pub "))
            .or_else(|| trimmed.strip_prefix("pub(super) "))
            .unwrap_or(trimmed);
        if !body.starts_with(&declaration) {
            continue;
        }
        let Some(assignment) = body.split_once("= ") else {
            continue;
        };
        let value = assignment.1.trim();
        let Some(rest) = value.strip_prefix('"') else {
            continue;
        };
        let Some(closing) = rest.find('"') else {
            continue;
        };
        matches.push(&rest[..closing]);
    }
    match matches.as_slice() {
        [] => Err(format!(
            "declaration `const {constant}: &str = \"...\"` was not found"
        )),
        [only] => Ok((*only).to_string()),
        values => Err(format!(
            "declaration `const {constant}: &str = \"...\"` matched {} lines ({values:?}); the registry expects exactly one defining declaration",
            values.len()
        )),
    }
}

/// Dot-separated numeric components: `1.142`, `0.2`, `6`, `18` all parse, so
/// the families compare uniformly. A non-numeric value is an instrument
/// failure, never a pass.
fn parse_version(value: &str) -> Result<Vec<u64>, String> {
    if value.is_empty() {
        return Err("generation value is empty".to_string());
    }
    let mut components = Vec::new();
    for component in value.split('.') {
        let parsed = component
            .parse::<u64>()
            .map_err(|_parse_error| format!("component `{component}` is not a numeric counter"))?;
        components.push(parsed);
    }
    Ok(components)
}

/// The next free value: last component plus one (`1.142` -> `1.143`,
/// `6` -> `7`). Skipped numbers stay legal; only strict monotonicity is
/// required (#11736; precedent `1.129 -> 1.131`).
fn next_free(value: &str) -> Result<String, String> {
    let mut components = parse_version(value)?;
    let Some(last) = components.pop() else {
        return Err(format!("generation value `{value}` has no components"));
    };
    let bumped = last.checked_add(1).ok_or_else(|| {
        format!("generation component `{last}` is at the numeric limit; choose a new family shape")
    })?;
    components.push(bumped);
    let rendered = components
        .iter()
        .map(u64::to_string)
        .collect::<Vec<_>>()
        .join(".");
    Ok(rendered)
}

fn read_generation(
    reader: &BlobReader<'_>,
    reference: Option<&str>,
    pin: &GenerationPin,
) -> Result<String, String> {
    let text = reader(reference, pin.defining_file)?;
    extract_constant(&text, pin.constant)
}

/// Where a blob read or extraction failed, the gate says so and stops: an
/// unevaluable pin is never a pass.
fn fail_closed_detail(error: &str) -> String {
    format!(
        "evaluation failed closed, not a pass: {error}. If the defining file moved or was renamed, update GENERATION_PINS in xtask/src/generation_pins.rs; otherwise resolve the git/read failure and rerun."
    )
}

fn claimed_generation_violations(
    reader: &BlobReader<'_>,
    base_ref: &str,
    fork_commit: &str,
    head_ref: Option<&str>,
) -> Vec<String> {
    let mut violations = Vec::new();
    for pin in GENERATION_PINS {
        let head = match read_generation(reader, head_ref, pin) {
            Ok(value) => value,
            Err(error) => {
                violations.push(row_failure(pin, &error));
                continue;
            }
        };
        let fork = match read_generation(reader, Some(fork_commit), pin) {
            Ok(value) => value,
            Err(error) => {
                violations.push(row_failure(pin, &error));
                continue;
            }
        };
        // Untouched constant: nothing to judge, and the base tip is not read,
        // so a base that renamed the defining file cannot fail an unrelated PR.
        if head == fork {
            continue;
        }
        let base_tip = match read_generation(reader, Some(base_ref), pin) {
            Ok(value) => value,
            Err(error) => {
                violations.push(row_failure(pin, &error));
                continue;
            }
        };
        match evaluate_claim(&head, &fork, &base_tip) {
            Ok(ClaimVerdict::NoClaim | ClaimVerdict::FreshClaim) => {}
            Ok(ClaimVerdict::Collision) => violations.push(format!(
                "{}: {} claims {head}, which is already {base_ref}'s current value — a sibling claimed this generation (the #10326/#10845 collision class). Run `cargo xtask generations next` and pin the next free value, refreshing every consumer in the same PR.",
                pin.family, pin.constant
            )),
            Ok(ClaimVerdict::StaleClaim) => violations.push(format!(
                "{}: {} claims {head}, which is older than {base_ref}'s current {base_tip} — main moved past this claim (the #10587 stale-claim class). Run `cargo xtask generations next` and pin the next free value, refreshing every consumer in the same PR.",
                pin.family, pin.constant
            )),
            Err(error) => violations.push(format!(
                "{} ({} in {}): generation claim could not be evaluated, failing closed: {error}",
                pin.family, pin.constant, pin.defining_file
            )),
        }
    }
    violations
}

fn row_failure(pin: &GenerationPin, error: &str) -> String {
    format!(
        "{} ({} in {}): {}",
        pin.family,
        pin.constant,
        pin.defining_file,
        fail_closed_detail(error)
    )
}

fn production_blob_reader(
    root: &Path,
) -> impl Fn(Option<&str>, &str) -> Result<String, String> + '_ {
    move |reference, path| match reference {
        None => std::fs::read_to_string(root.join(path))
            .map_err(|error| format!("read working-tree {path}: {error}")),
        Some(reference) => git_show_text(root, reference, path),
    }
}

fn git_show_text(root: &Path, reference: &str, path: &str) -> Result<String, String> {
    let object = format!("{reference}:{path}");
    git_capture(root, &["show", &object], &format!("git show {object}"))
}

/// Spawn `git` with `args` in `root`, returning the raw process output. One
/// spawning site for the whole module keeps the per-file `Command::new`
/// process budget (check-process-policy) at a single site for all git
/// plumbing.
fn git_spawn(root: &Path, args: &[&str]) -> Result<std::process::Output, String> {
    std::process::Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .map_err(|error| format!("git {} failed to spawn: {error}", args.join(" ")))
}

/// Run `git` with `args` in `root` and return trimmed-on-request stdout
/// (callers trim); a non-zero exit becomes `{label} failed: {stderr}` (or
/// the exit status when git wrote nothing).
fn git_capture(root: &Path, args: &[&str], label: &str) -> Result<String, String> {
    let output = git_spawn(root, args)?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let detail = if stderr.is_empty() {
            format!("exit status {}", output.status)
        } else {
            stderr
        };
        return Err(format!("{label} failed: {detail}"));
    }
    String::from_utf8(output.stdout)
        .map_err(|error| format!("{label} returned non-UTF-8 bytes: {error}"))
}

/// Resolve the accepted base exactly as `resolve_merge_base` does
/// (precommit_v2.rs): `origin/main`, then `main`, with an explicit `--base`
/// override for the helper. Returns the base ref (for tip reads) and the
/// merge-base SHA (for fork reads). Unresolvable is an actionable
/// fail-closed failure, never a silent pass.
fn resolve_base(root: &Path, explicit: Option<&str>) -> Result<ResolvedBase, String> {
    let candidates: Vec<String> = match explicit {
        Some(value) => vec![value.to_string()],
        None => vec!["origin/main".to_string(), "main".to_string()],
    };
    let candidate_refs: Vec<&str> = candidates.iter().map(String::as_str).collect();
    let merge_base = |candidate: &str| -> Result<String, String> {
        git_merge_base_commit(root, candidate, "HEAD")
    };
    let (base_ref, head_merge_base) = resolve_base_with(&candidate_refs, &merge_base)?;
    let base_tip_commit = git_rev_parse(root, &base_ref)?.ok_or_else(|| {
        format!("base ref `{base_ref}` resolved for merge-base but not rev-parse")
    })?;
    Ok(ResolvedBase {
        base_ref,
        base_tip_commit,
        head_merge_base,
    })
}

/// `git rev-parse <object>`: `Ok(None)` when the object does not resolve
/// (non-zero exit), `Err` on spawn or decode failure.
fn git_rev_parse(root: &Path, object: &str) -> Result<Option<String>, String> {
    let label = format!("git rev-parse {object}");
    match git_capture(root, &["rev-parse", object], &label) {
        Ok(text) => Ok(Some(text.trim().to_string())),
        // A non-resolving object exits non-zero. A spawn failure is kept
        // loud: by the time this probes `HEAD^2`, the merge-base resolution
        // has already required a working git, so `Err` here means git broke
        // mid-run and the caller must fail closed.
        Err(error) if !error.contains("failed to spawn") => Ok(None),
        Err(error) => Err(error),
    }
}

/// `git merge-base <left> <right>`, trimmed; a non-zero exit becomes the
/// trimmed stderr (or the exit status) as the error detail.
fn git_merge_base_commit(root: &Path, left: &str, right: &str) -> Result<String, String> {
    git_capture(
        root,
        &["merge-base", left, right],
        &format!("git merge-base {left} {right}"),
    )
    .map(|text| text.trim().to_string())
}

fn resolve_base_with(
    candidates: &[&str],
    merge_base: &dyn Fn(&str) -> Result<String, String>,
) -> Result<(String, String), String> {
    let mut failures = Vec::new();
    for candidate in candidates {
        match merge_base(candidate) {
            Ok(sha) if is_sha(&sha) => return Ok(((*candidate).to_string(), sha)),
            Ok(other) => failures.push(format!("{candidate}: malformed output `{other}`")),
            Err(error) => failures.push(format!("{candidate}: {error}")),
        }
    }
    Err(format!(
        "no accepted merge base was available: {}. check-generation-pins needs a locally available `origin/main` or `main` commit that shares Git history with HEAD and fails closed instead of guessing. If `origin` tracks the default branch, you can recover with: `git fetch origin main:refs/remotes/origin/main` (only when you are authorized to fetch).",
        failures.join("; ")
    ))
}

/// A full object id: 40 hex digits (SHA-1) or 64 (SHA-256 repositories).
fn is_sha(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.chars().all(|character| character.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::{
        CONSUMER_CHECKLIST, ClaimVerdict, GENERATION_PINS, claimed_generation_violations,
        evaluate_claim, extract_constant, next_free, parse_generations_next_args, parse_version,
        resolve_base_with,
    };
    use crate::repo_root;
    use std::collections::BTreeMap;

    // --- Predicate unit tests (the discriminators) ---

    #[test]
    fn no_claim_passes_even_when_main_ahead() -> Result<(), String> {
        let verdict = evaluate_claim("1.141", "1.141", "1.142")?;
        assert_eq!(verdict, ClaimVerdict::NoClaim);
        Ok(())
    }

    #[test]
    fn equals_base_claim_is_collision() -> Result<(), String> {
        // Red before this gate existed: a candidate repinning main's current
        // value passed everything locally until an audit found it (#10326).
        let verdict = evaluate_claim("1.142", "1.141", "1.142")?;
        assert_eq!(verdict, ClaimVerdict::Collision);
        Ok(())
    }

    #[test]
    fn stale_claim_below_main_is_rejected() -> Result<(), String> {
        let verdict = evaluate_claim("1.140", "1.139", "1.142")?;
        assert_eq!(verdict, ClaimVerdict::StaleClaim);
        Ok(())
    }

    #[test]
    fn fresh_claim_strictly_above_main_passes() -> Result<(), String> {
        let verdict = evaluate_claim("1.143", "1.141", "1.142")?;
        assert_eq!(verdict, ClaimVerdict::FreshClaim);
        Ok(())
    }

    #[test]
    fn skipped_generations_are_legal() -> Result<(), String> {
        // Precedent: `1.129 -> 1.131` — the predicate is strictly greater,
        // never contiguous.
        let verdict = evaluate_claim("1.150", "1.141", "1.142")?;
        assert_eq!(verdict, ClaimVerdict::FreshClaim);
        Ok(())
    }

    #[test]
    fn downgrade_of_untouched_main_is_stale_not_pass() -> Result<(), String> {
        let verdict = evaluate_claim("1.139", "1.141", "1.141")?;
        assert_eq!(verdict, ClaimVerdict::StaleClaim);
        Ok(())
    }

    #[test]
    fn single_and_multi_component_values_compare_numerically() -> Result<(), String> {
        assert_eq!(
            evaluate_claim("0.145", "0.144", "0.144")?,
            ClaimVerdict::FreshClaim
        );
        assert_eq!(evaluate_claim("7", "6", "6")?, ClaimVerdict::FreshClaim);
        assert_eq!(evaluate_claim("19", "18", "18")?, ClaimVerdict::FreshClaim);
        assert_eq!(
            evaluate_claim("18", "17", "19")?,
            ClaimVerdict::StaleClaim,
            "a claimed value below main's tip is stale regardless of component shape"
        );
        Ok(())
    }

    #[test]
    fn non_numeric_claimed_value_is_instrument_failure_not_pass() -> Result<(), String> {
        let error = evaluate_claim("release-final", "1.141", "1.142")
            .err()
            .ok_or("a non-numeric claimed generation must fail closed")?;
        assert!(
            error.contains("release-final"),
            "instrument failure must name the offending value: {error}"
        );
        Ok(())
    }

    #[test]
    fn non_numeric_base_tip_is_instrument_failure_not_pass() -> Result<(), String> {
        let error = evaluate_claim("1.143", "1.141", "not-a-number")
            .err()
            .ok_or("a non-numeric main value must fail closed")?;
        assert!(
            error.contains("not-a-number"),
            "instrument failure must name the offending value: {error}"
        );
        Ok(())
    }

    #[test]
    fn empty_base_tip_is_instrument_failure_not_pass() -> Result<(), String> {
        let error = evaluate_claim("1.143", "1.141", "")
            .err()
            .ok_or("an empty main value must fail closed")?;
        assert!(
            error.contains("empty"),
            "instrument failure must explain the empty value: {error}"
        );
        Ok(())
    }

    // --- Extraction tests (value-independent) ---

    #[test]
    fn extraction_reads_the_authored_declaration_form() -> Result<(), String> {
        let snippet = "/// History comment mentioning const CACHE_SCHEMA_VERSION: &str = \"1.0\" must not match.\npub(crate) const CACHE_SCHEMA_VERSION: &str = \"9.9\";\n";
        let value = extract_constant(snippet, "CACHE_SCHEMA_VERSION")
            .map_err(|error| format!("the authored declaration must extract: {error}"))?;
        assert_eq!(value, "9.9");
        Ok(())
    }

    #[test]
    fn extraction_requires_a_real_declaration_line() -> Result<(), String> {
        let snippet = "// const CACHE_SCHEMA_VERSION: &str = \"1.0\";\nassert_eq!(CACHE_SCHEMA_VERSION, \"9.8\");\nconst UNRELATED_CACHE_SCHEMA_VERSION: &str = \"3.3\";\n";
        let error = extract_constant(snippet, "CACHE_SCHEMA_VERSION")
            .err()
            .ok_or("comments and assertions are not declarations")?;
        assert!(
            error.contains("was not found"),
            "staleness error must say the declaration was not found: {error}"
        );
        Ok(())
    }

    #[test]
    fn extraction_is_value_independent() -> Result<(), String> {
        for value in ["0.2", "6", "18", "1.142", "0.144"] {
            let snippet = format!("pub const COUNT_CACHE_SCHEMA_VERSION: &str = \"{value}\";\n");
            let extracted = extract_constant(&snippet, "COUNT_CACHE_SCHEMA_VERSION")
                .map_err(|error| format!("value {value} must extract: {error}"))?;
            assert_eq!(extracted, value);
        }
        Ok(())
    }

    #[test]
    fn ambiguous_extraction_fails_closed() -> Result<(), String> {
        let snippet = "const CACHE_SCHEMA_VERSION: &str = \"1.1\";\nconst CACHE_SCHEMA_VERSION: &str = \"1.2\";\n";
        let error = extract_constant(snippet, "CACHE_SCHEMA_VERSION")
            .err()
            .ok_or("two declaration lines are ambiguous and must fail closed")?;
        assert!(
            error.contains("exactly one defining declaration"),
            "ambiguity must be reported: {error}"
        );
        Ok(())
    }

    /// The registry-staleness discriminator: every defining file must still
    /// contain its constant's declaration in the real tree. If a defining
    /// file moves or a constant is renamed, this fails until GENERATION_PINS
    /// is updated — the gate can never silently evaluate an empty registry.
    #[test]
    fn registry_rows_resolve_in_the_real_tree() -> Result<(), String> {
        let root = repo_root()?;
        for pin in GENERATION_PINS {
            let path = root.join(pin.defining_file);
            let text = std::fs::read_to_string(&path)
                .map_err(|error| format!("read {}: {error}", pin.defining_file))?;
            let value = extract_constant(&text, pin.constant).map_err(|error| {
                format!(
                    "registry row `{}` ({}) is stale in {}: {error}",
                    pin.family, pin.constant, pin.defining_file
                )
            })?;
            assert!(
                !value.is_empty(),
                "registry row `{}` extracted an empty value",
                pin.family
            );
        }
        Ok(())
    }

    // --- Full per-row violation flow ---

    #[test]
    fn untouched_tree_yields_no_violations() -> Result<(), String> {
        let head = synthetic_texts(|_family| "5.5".to_string());
        let fork = synthetic_texts(|_family| "5.5".to_string());
        let tip = synthetic_texts(|_family| "5.5".to_string());
        let reader = three_ref_reader(&head, &fork, &tip);
        let violations = claimed_generation_violations(&reader, "origin/main", "FORKSHA", None);
        assert!(
            violations.is_empty(),
            "an untouched tree must pass: {violations:?}"
        );
        Ok(())
    }

    #[test]
    fn untouched_constant_never_reads_the_base_tip() -> Result<(), String> {
        let head = synthetic_texts(|_family| "5.5".to_string());
        let fork = synthetic_texts(|_family| "5.5".to_string());
        let tip = synthetic_texts(|_family| "5.5".to_string());
        let inner = three_ref_reader(&head, &fork, &tip);
        // A base tip that cannot be read (for example a renamed defining file)
        // must not fail a PR that never touched the constant.
        let reader = |reference: Option<&str>, path: &str| match reference {
            Some("origin/main") => Err(format!("fixture base tip has no {path}")),
            other => inner(other, path),
        };
        let violations = claimed_generation_violations(&reader, "origin/main", "FORKSHA", None);
        assert!(
            violations.is_empty(),
            "an untouched constant must pass without a base-tip read: {violations:?}"
        );
        Ok(())
    }

    #[test]
    fn is_sha_accepts_sha1_and_sha256_object_ids() {
        assert!(super::is_sha(&"a".repeat(40)));
        assert!(super::is_sha(&"0".repeat(64)));
        assert!(!super::is_sha(&"a".repeat(41)));
        assert!(!super::is_sha(&"g".repeat(40)));
    }

    #[test]
    fn collision_violation_names_family_values_and_repair() -> Result<(), String> {
        let head = synthetic_texts(|family| {
            if family == "classified_seam_cache" {
                "9.9".to_string()
            } else {
                "5.5".to_string()
            }
        });
        let fork = synthetic_texts(|_family| "5.5".to_string());
        let tip = synthetic_texts(|family| {
            if family == "classified_seam_cache" {
                "9.9".to_string()
            } else {
                "5.5".to_string()
            }
        });
        let reader = three_ref_reader(&head, &fork, &tip);
        let violations = claimed_generation_violations(&reader, "origin/main", "FORKSHA", None);
        let violation = violations
            .first()
            .ok_or("a claimed == base-tip value must violate")?;
        assert_eq!(
            violations.len(),
            1,
            "exactly one family claimed: {violations:?}"
        );
        assert!(
            violation.contains("classified_seam_cache")
                && violation.contains("9.9")
                && violation.contains("generations next"),
            "collision message must name family, value, and repair route: {violation}"
        );
        Ok(())
    }

    #[test]
    fn fresh_claim_with_skip_gap_yields_no_violations() -> Result<(), String> {
        let head = synthetic_texts(|family| {
            if family == "classified_seam_cache" {
                "9.9".to_string()
            } else {
                "5.5".to_string()
            }
        });
        let fork = synthetic_texts(|_family| "5.5".to_string());
        let tip = synthetic_texts(|family| {
            if family == "classified_seam_cache" {
                "9.8".to_string()
            } else {
                "5.5".to_string()
            }
        });
        let reader = three_ref_reader(&head, &fork, &tip);
        let violations = claimed_generation_violations(&reader, "origin/main", "FORKSHA", None);
        assert!(
            violations.is_empty(),
            "a fresh claim strictly above main's tip must pass: {violations:?}"
        );
        Ok(())
    }

    #[test]
    fn stale_claim_violation_names_base_tip_value() -> Result<(), String> {
        let head = synthetic_texts(|_family| "5.5".to_string());
        let fork = synthetic_texts(|_family| "5.4".to_string());
        let tip = synthetic_texts(|_family| "9.9".to_string());
        let reader = three_ref_reader(&head, &fork, &tip);
        let violations = claimed_generation_violations(&reader, "origin/main", "FORKSHA", None);
        let violation = violations
            .first()
            .ok_or("a claim older than main's tip must violate")?;
        assert!(
            violation.contains("older than origin/main's current 9.9"),
            "stale message must name main's current value: {violation}"
        );
        Ok(())
    }

    #[test]
    fn stale_registry_row_is_a_violation_not_a_pass() -> Result<(), String> {
        let mut texts = synthetic_texts(|_family| "5.5".to_string());
        texts.insert(
            "crates/ripr/src/analysis/language/python/fact_store.rs",
            "const RENAMED_AWAY: &str = \"18\";\n".to_string(),
        );
        let fork = synthetic_texts(|_family| "5.5".to_string());
        let tip = synthetic_texts(|_family| "5.5".to_string());
        let reader = three_ref_reader(&texts, &fork, &tip);
        let violations = claimed_generation_violations(&reader, "origin/main", "FORKSHA", None);
        let violation = violations
            .first()
            .ok_or("a stale registry row must fail closed")?;
        assert!(
            violation.contains("python_file_fact_store") && violation.contains("failed closed"),
            "staleness must name the family and stay fail-closed: {violation}"
        );
        Ok(())
    }

    /// The GitHub synthetic PR merge ref aliases fork and base tip when
    /// evaluated at HEAD: a sibling that merged generation X plus this PR
    /// claiming the same X merges cleanly to `head == fork == base_tip == X`,
    /// and a HEAD-based evaluation reads that as NoClaim (review thread
    /// PRRT_kwDOSiSx0c6zB6Ce). Merge-ref mode reads the claim from the PR
    /// head tree instead, so the same values still fail as a collision.
    #[test]
    fn merge_ref_evaluation_keeps_the_sibling_collision_red() -> Result<(), String> {
        let pr_head = synthetic_texts(|family| {
            if family == "classified_seam_cache" {
                "9.9".to_string()
            } else {
                "5.5".to_string()
            }
        });
        let fork = synthetic_texts(|_family| "5.5".to_string());
        let tip = synthetic_texts(|_family| "9.9".to_string());
        let reader = |reference: Option<&str>, path: &str| -> Result<String, String> {
            let texts = match reference {
                None => {
                    return Err(
                        "merge-ref mode must read the PR head tree, not the worktree".to_string(),
                    );
                }
                Some("PRHEADSHA") => &pr_head,
                Some("FORKSHA") => &fork,
                Some(_) => &tip,
            };
            texts
                .get(path)
                .cloned()
                .ok_or_else(|| format!("fixture has no text for {path}"))
        };
        // head_ref = the PR head commit (the merge commit's second parent).
        let violations =
            claimed_generation_violations(&reader, "origin/main", "FORKSHA", Some("PRHEADSHA"));
        let violation = violations
            .first()
            .ok_or("a sibling-claimed generation must stay red in merge-ref mode")?;
        assert_eq!(
            violations.len(),
            1,
            "exactly the claimed family: {violations:?}"
        );
        assert!(
            violation.contains("classified_seam_cache") && violation.contains("collision class"),
            "merge-ref mode must surface the collision: {violation}"
        );
        // Control: the aliased shape (head == fork == base_tip) is a NoClaim —
        // exactly why merge-ref mode must not evaluate HEAD's own tree.
        let aliased = synthetic_texts(|_family| "9.9".to_string());
        let reader_aliased = three_ref_reader(&aliased, &aliased, &aliased);
        let violations =
            claimed_generation_violations(&reader_aliased, "origin/main", "FORKSHA", None);
        assert!(
            violations.is_empty(),
            "the aliased shape documents the miss (NoClaim): {violations:?}"
        );
        Ok(())
    }

    /// End-to-end merge-ref regression over a real git fixture: build the
    /// GitHub-shaped merge commit (first parent = base tip, second parent =
    /// PR head), point `refs/remotes/origin/main` at the base tip, and run
    /// the real resolution path. A sibling that merged 7.7 plus this PR
    /// claiming the same 7.7 must fail as a collision, not pass as NoClaim.
    #[test]
    fn merge_ref_resolution_surfaces_sibling_collision_in_a_real_repo() -> Result<(), String> {
        use super::{
            EvaluationPoints, ResolvedBase, git_show_text, production_blob_reader, resolve_base,
            resolve_evaluation_points,
        };
        let root = crate::tests::temp_dir("generation-pins-merge-ref");
        let run = |args: &[&str]| -> Result<String, String> {
            super::git_capture(&root, args, &format!("git {args:?}"))
                .map(|text| text.trim().to_string())
        };
        let commit_with = |value: &str, message: &str| -> Result<String, String> {
            // Rewrite the classified constant; every other row stays at 5.5.
            for pin in GENERATION_PINS {
                let path = root.join(pin.defining_file);
                std::fs::create_dir_all(path.parent().ok_or("fixture path has no parent")?)
                    .map_err(|error| format!("create {}: {error}", path.display()))?;
                let value_for_row = if pin.family == "classified_seam_cache" {
                    value
                } else {
                    "5.5"
                };
                std::fs::write(
                    &path,
                    format!(
                        "pub(crate) const {}: &str = \"{value_for_row}\";\n",
                        pin.constant
                    ),
                )
                .map_err(|error| format!("write {}: {error}", path.display()))?;
            }
            run(&["add", "-A"])?;
            run(&[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@example.com",
                "commit",
                "-m",
                message,
            ])?;
            run(&["rev-parse", "HEAD"])
        };
        run(&["init"])?;
        let base = commit_with("5.5", "base")?;
        let _ = base;
        run(&["checkout", "-b", "pr"])?;
        let pr_head = commit_with("7.7", "pr claims 7.7")?;
        run(&["checkout", "-"])?;
        let main_tip = commit_with("7.7", "sibling merged 7.7")?;
        run(&["update-ref", "refs/remotes/origin/main", &main_tip])?;
        // GitHub's synthetic merge: first parent = base tip, second parent =
        // PR head. Both sides made the identical change, so the merge tree is
        // the base tip's tree.
        let tree = run(&["rev-parse", &format!("{main_tip}^{{tree}}")])?;
        let merge_commit = run(&[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@example.com",
            "commit-tree",
            &tree,
            "-p",
            &main_tip,
            "-p",
            &pr_head,
            "-m",
            "synthetic PR merge ref",
        ])?;
        run(&["reset", "--hard", &merge_commit])?;

        let resolved: ResolvedBase = resolve_base(&root, None)?;
        let points: EvaluationPoints = resolve_evaluation_points(&root, &resolved)?;
        let head_ref = points
            .head_ref
            .clone()
            .ok_or("the synthetic merge ref must select merge-ref mode")?;
        let reader = production_blob_reader(&root);
        let violations = claimed_generation_violations(
            &reader,
            &resolved.base_ref,
            &points.fork_commit,
            Some(&head_ref),
        );
        let violation = violations
            .first()
            .ok_or("the sibling collision must stay red on a synthetic merge ref")?;
        assert_eq!(
            violations.len(),
            1,
            "exactly the claimed family: {violations:?}"
        );
        assert!(
            violation.contains("collision class") && violation.contains("7.7"),
            "the collision must name the claimed value and class: {violation}"
        );
        // The fork is the PR's true fork (value 5.5), not the aliased base tip.
        let fork_value =
            git_show_text(&root, &points.fork_commit, GENERATION_PINS[0].defining_file)?;
        assert!(
            fork_value.contains("5.5"),
            "fork must be the PR's original fork, not the aliased base tip: {fork_value}"
        );
        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    /// Synthetic per-family texts: every defining file declares each of its
    /// registry constants at the value the caller's closure returns for that
    /// family. Value-independent: no real generation value is embedded.
    fn synthetic_texts(
        value_for_family: impl Fn(&str) -> String,
    ) -> BTreeMap<&'static str, String> {
        let mut texts: BTreeMap<&'static str, Vec<String>> = BTreeMap::new();
        for pin in GENERATION_PINS {
            let value = value_for_family(pin.family);
            texts
                .entry(pin.defining_file)
                .or_default()
                .push(format!("pub const {}: &str = \"{value}\";\n", pin.constant));
        }
        texts
            .into_iter()
            .map(|(file, lines)| (file, lines.join("\n")))
            .collect()
    }

    /// A reader over three synthetic text maps: worktree (`None`) reads
    /// `head_texts`, `"FORKSHA"` reads `fork_texts`, any other reference
    /// reads `tip_texts`.
    fn three_ref_reader<'a>(
        head_texts: &'a BTreeMap<&'static str, String>,
        fork_texts: &'a BTreeMap<&'static str, String>,
        tip_texts: &'a BTreeMap<&'static str, String>,
    ) -> impl Fn(Option<&str>, &str) -> Result<String, String> + 'a {
        move |reference, path| {
            let texts = match reference {
                None => head_texts,
                Some("FORKSHA") => fork_texts,
                Some(_) => tip_texts,
            };
            texts
                .get(path)
                .cloned()
                .ok_or_else(|| format!("fixture has no text for {path}"))
        }
    }

    // --- Helper tests ---

    #[test]
    fn next_free_bumps_the_last_component() -> Result<(), String> {
        assert_eq!(next_free("1.142")?, "1.143");
        assert_eq!(next_free("0.144")?, "0.145");
        assert_eq!(next_free("0.2")?, "0.3");
        assert_eq!(next_free("6")?, "7");
        assert_eq!(next_free("18")?, "19");
        Ok(())
    }

    #[test]
    fn next_free_rejects_non_numeric_values() -> Result<(), String> {
        let error = next_free("main-tip")
            .err()
            .ok_or("a non-numeric value has no next")?;
        assert!(
            error.contains("main-tip"),
            "next-free failure must name the value: {error}"
        );
        Ok(())
    }

    #[test]
    fn version_parse_rejects_garbage_components() -> Result<(), String> {
        assert!(
            parse_version("").is_err(),
            "an empty generation value must not parse"
        );
        assert!(
            parse_version("1.x").is_err(),
            "a non-numeric component must not parse"
        );
        assert_eq!(parse_version("1.142")?, vec![1, 142]);
        Ok(())
    }

    #[test]
    fn base_resolution_prefers_origin_main_then_falls_back_to_main() -> Result<(), String> {
        let calls = std::cell::RefCell::new(Vec::new());
        let resolver = |candidate: &str| -> Result<String, String> {
            calls.borrow_mut().push(candidate.to_string());
            if candidate == "origin/main" {
                Err("fatal: bad ref".to_string())
            } else {
                Ok("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string())
            }
        };
        let (base_ref, sha) = resolve_base_with(&["origin/main", "main"], &resolver)?;
        assert_eq!(base_ref, "main");
        assert_eq!(sha, "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
        assert_eq!(
            *calls.borrow(),
            vec!["origin/main".to_string(), "main".to_string()],
            "candidates must be tried in order"
        );
        Ok(())
    }

    #[test]
    fn base_resolution_fails_closed_when_no_candidate_resolves() -> Result<(), String> {
        let resolver =
            |_candidate: &str| -> Result<String, String> { Err("fatal: bad ref".to_string()) };
        let error = resolve_base_with(&["origin/main", "main"], &resolver)
            .err()
            .ok_or("without any base the gate must fail closed")?;
        assert!(
            error.contains("origin/main") && error.contains("fetch"),
            "the failure must name the candidates and the recovery route: {error}"
        );
        Ok(())
    }

    #[test]
    fn base_resolution_rejects_malformed_sha_output() -> Result<(), String> {
        let resolver = |_candidate: &str| -> Result<String, String> { Ok("not-a-sha".to_string()) };
        let error = resolve_base_with(&["origin/main"], &resolver)
            .err()
            .ok_or("malformed merge-base output must fail closed")?;
        assert!(
            error.contains("malformed output"),
            "malformed output must be reported: {error}"
        );
        Ok(())
    }

    #[test]
    fn explicit_base_restricts_the_candidates() -> Result<(), String> {
        let resolver = |candidate: &str| -> Result<String, String> {
            if candidate == "refs/heads/release-cut" {
                Ok("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string())
            } else {
                Err("fatal: bad ref".to_string())
            }
        };
        let (base_ref, _sha) = resolve_base_with(&["refs/heads/release-cut"], &resolver)?;
        assert_eq!(base_ref, "refs/heads/release-cut");
        Ok(())
    }

    #[test]
    fn generations_next_parses_base_override_and_rejects_unknown_options() -> Result<(), String> {
        let base =
            parse_generations_next_args(&["--base".to_string(), "refs/heads/x".to_string()])?;
        assert_eq!(base.as_deref(), Some("refs/heads/x"));
        assert!(
            parse_generations_next_args(&["--base".to_string()]).is_err(),
            "--base without a value must be rejected"
        );
        assert!(
            parse_generations_next_args(&["--claim".to_string()]).is_err(),
            "unknown options must be rejected"
        );
        Ok(())
    }

    #[test]
    fn consumer_checklist_names_the_independent_literal_copies() {
        let checklist = CONSUMER_CHECKLIST.join("\n");
        assert!(
            checklist.contains("xtask/src/tests/rust_corpus_projection.rs")
                && checklist.contains("crates/ripr/tests/lsp_lifecycle/gap_corpus_currentness.rs")
                && checklist.contains("docs/OUTPUT_SCHEMA.md"),
            "the checklist must name the two independent literal copies and the schema doc"
        );
    }

    #[test]
    fn registry_has_ten_unique_families() {
        assert_eq!(GENERATION_PINS.len(), 10);
        let mut seen = std::collections::BTreeSet::new();
        for pin in GENERATION_PINS {
            assert!(seen.insert(pin.family), "duplicate family `{}`", pin.family);
        }
    }
}
