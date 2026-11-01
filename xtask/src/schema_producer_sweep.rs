//! Sweep of committed producer documents against every published
//! schema (#3920).
//!
//! `check-verification-contracts` validates one hand-picked subject per
//! contract row. That denominator is kept by hand, and #3919 records why that
//! is not enough on its own: #3894's second commit exists because its first
//! commit's chosen golden missed two producer shapes. Replacing the required
//! gate's subject selection is deferred past the 0.11 cut, so this command is
//! the compensating release proof instead. It discovers every committed
//! producer document by an explicit binding rule, validates each one with the
//! same validator the gate uses, and reports discovered and validated counts
//! separately so a qualification run can replay it on the frozen candidate.
//!
//! Every byte it validates, schemas included, is read from one explicit commit
//! (`--rev`, default `HEAD`) through `git archive`, never from the working
//! tree, so a dirty or artifact-littered release checkout cannot change the
//! denominator or pass bytes the candidate does not carry.
//!
//! The required Rust gate runs it on the candidate (#9505), feeding the
//! `doctor`, `pr-evidence` and `review-comments` runtime rows from that run's
//! own producer output. `repair-attempt` and `execution_result` need an agent
//! repair transaction too heavy for that gate; they carry a reviewed narrower
//! authority instead (#9561, pinned by a test below). The binding table below is the
//! reviewed rule for which committed bytes are producer output, which are
//! consumer-input stimulus, and which schemas have no committed producer bytes
//! at all and must be fed live artifacts with `--artifact`.

use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::exact_schema_json::Value as ExactValue;
use crate::verification_contracts::{
    exact_schema_values_equal, validate_exact_value_against_schema,
};

const REPORT_PATH: &str = "target/ripr/reports/schema-producer-sweep.json";
const USAGE: &str = "usage: cargo xtask schema-producer-sweep [--rev REV | --worktree] [--live-replay [--ripr BINARY]] [--artifact SCHEMA[#POINTER]=FILE[#POINTER]]...";
/// Every JSON file under this directory is a published schema, the same rule
/// `check-verification-contracts` applies to the working tree.
const SCHEMAS_PREFIX: &str = "schemas/";

/// Which committed documents a binding validates.
enum Corpus {
    /// Every document with this file name, anywhere in the repository.
    FileName(&'static str),
    /// Exactly these repository-relative paths.
    Paths(&'static [&'static str]),
    /// Every object under a `command_specs` map in any committed document,
    /// split by whether its role key is `verify`. Several producers embed
    /// these specs (agent packets, repo-exposure, first-useful-action), so a
    /// file-name rule would miss most of them.
    CommandSpecs { verify: bool },
    /// The producer writes only per-run bytes; nothing committed is producer
    /// output. Subjects come only from `--artifact`.
    RuntimeOnly(&'static str),
    /// Like `RuntimeOnly`, but a named in-crate test already validates the
    /// bytes the real producer writes, so a missing `--artifact` is
    /// `authority_only`, not `not_run`. Lookalike detection still applies, so a
    /// committed document with this schema's discriminators is not silently
    /// skipped. The pin test below keeps the named authority alive (#9561).
    Authority(&'static str),
    /// No producer emits this shape, or the producer is pinned by a narrower
    /// in-crate authority this validator cannot replace. Never counted as
    /// validated.
    Exempt(&'static str),
}

struct Binding {
    schema_path: &'static str,
    /// JSON pointer to the subschema the subjects must match, or `None` for
    /// the whole schema.
    pointer: Option<&'static str>,
    corpus: Corpus,
    /// Hand-written edge fixtures, validated and counted apart from producer
    /// bytes so they can never be the only subjects.
    edge_fixtures: &'static [&'static str],
    /// Path prefixes whose matching documents are consumer-input stimulus,
    /// each with the reviewed reason.
    excluded: &'static [(&'static str, &'static str)],
}

const BINDINGS: &[Binding] = &[
    Binding {
        schema_path: "schemas/badges/shields-endpoint.schema.json",
        pointer: None,
        corpus: Corpus::Paths(&["badges/ripr.json", "badges/ripr-plus.json"]),
        edge_fixtures: &["tests/fixtures/verification/badge/ripr-plus.valid.json"],
        excluded: &[],
    },
    Binding {
        schema_path: "schemas/ripr/check.schema.json",
        pointer: None,
        corpus: Corpus::FileName("check.json"),
        edge_fixtures: &[
            "tests/fixtures/verification/ripr/check-complete.valid.json",
            "tests/fixtures/verification/ripr/check-incomplete.valid.json",
            "tests/fixtures/verification/ripr/check-limited.valid.json",
            // Retained captures of past `ripr check` runs inside real-repo eval
            // fixtures. They validate against the schema, but they are not
            // bytes the candidate's build produced, so they stay edge fixtures.
            "fixtures/evidence-quality-benchmark/regex-word-boundary-empty/static/68770-corrected.json",
            "fixtures/evidence-quality-benchmark/regex-word-boundary-empty/static/68770-original.json",
            "fixtures/evidence-quality-benchmark/regex-word-boundary-empty/static/68770-weak.json",
            "fixtures/python-real-repo-evals/werkzeug-multiple-cookie/evidence/effective-check.json",
            "fixtures/python-real-repo-evals/werkzeug-multiple-cookie/evidence/ineffective-check.json",
            // Retained captures of past `ripr check` runs: the D491 cross-file
            // integer-truthiness rejection receipts preserved with #9795's
            // bounded admission slice. They validate against the schema, but
            // they are not bytes the candidate's build produced, so they stay
            // edge fixtures.
            "fixtures/python_dunder_truthiness_integer/evidence/cross-file-direct.report.json",
            "fixtures/python_dunder_truthiness_integer/evidence/cross-file-generated.report.json",
            "fixtures/python_dunder_truthiness_integer/evidence/cross-file-hidden.report.json",
            "fixtures/python_dunder_truthiness_integer/evidence/cross-file-unicode.report.json",
        ],
        excluded: &[(
            "fixtures/evidence-promotion-honesty-corpus/reports/",
            "hand-written `source_report` stimulus read by check-evidence-promotion-honesty, not `ripr check` output",
        )],
    },
    Binding {
        schema_path: "schemas/ripr/check-timeout-refusal.schema.json",
        pointer: None,
        corpus: Corpus::Authority(
            "narrower authority: timeout_refusal_with_stashed_context_renders_input_context_at_generation_0_3 in crates/ripr/src/output/limited_check.rs asserts the rendered document's generation, keys, and fact/recovery shapes at the renderer, and check_json_analysis_timeout_retains_requested_echo_and_bounded_recovery in crates/ripr/tests/cli_smoke.rs parses the real CLI bytes of a forced-deadline run; the committed fixture is hand-written (forced-deadline bytes only exist per run), so release replay still passes it with --artifact",
        ),
        edge_fixtures: &["tests/fixtures/verification/ripr/check-timeout-refusal.valid.json"],
        excluded: &[],
    },
    Binding {
        schema_path: "schemas/ripr/gate-decision.schema.json",
        pointer: None,
        corpus: Corpus::FileName("gate-decision.json"),
        edge_fixtures: &[
            "tests/fixtures/verification/ripr/gate-decision.valid.json",
            "tests/fixtures/verification/ripr/gate-decision.causal-delta.valid.json",
        ],
        excluded: &[
            (
                "fixtures/boundary_gap/expected/pr-evidence-ledger/",
                "`inputs.gate_decision` stimulus; the consumer reads an untyped subset",
            ),
            (
                "fixtures/boundary_gap/expected/pr-review-front-panel/",
                "`inputs.gate_decision` stimulus; the consumer reads an untyped subset",
            ),
            (
                "fixtures/boundary_gap/expected/report-packet-index/",
                "packet-tree placeholders the index renderer only inventories",
            ),
        ],
    },
    Binding {
        schema_path: "schemas/ripr/pr-evidence.schema.json",
        pointer: None,
        corpus: Corpus::RuntimeOnly(
            "`ripr pr-evidence` and `cargo xtask ripr-pr` write target/ripr/pr/repo-exposure.json per run",
        ),
        edge_fixtures: &["tests/fixtures/verification/ripr/pr-evidence.valid.json"],
        excluded: &[],
    },
    Binding {
        schema_path: "schemas/ripr/review-comments.schema.json",
        pointer: None,
        corpus: Corpus::RuntimeOnly(
            "`ripr review-comments` attaches its run receipt per run; committed pr-guidance goldens are renderer bytes without it",
        ),
        edge_fixtures: &[
            "tests/fixtures/verification/ripr/review-comments.valid.json",
            "tests/fixtures/verification/ripr/review-comments.gap-ledger.valid.json",
            "tests/fixtures/verification/ripr/review-comments.capped.valid.json",
        ],
        excluded: &[],
    },
    Binding {
        schema_path: "schemas/ripr/repair-attempt.schema.json",
        pointer: None,
        corpus: Corpus::Authority(
            "narrower authority (#9561): assert_manifest_matches_published_schema in crates/ripr/tests/python_repair_attempt.rs checks the bytes the real `ripr agent repair` before/after phases write; `ripr agent repair` writes target/ripr/repair-attempts/<id>/attempt.json per attempt, so release replay still passes it with --artifact",
        ),
        edge_fixtures: &[],
        excluded: &[],
    },
    Binding {
        schema_path: "schemas/ripr/repair-assurance.schema.json",
        pointer: Some("/$defs/command_spec"),
        corpus: Corpus::CommandSpecs { verify: false },
        edge_fixtures: &[],
        excluded: &[(
            "fixtures/evidence-promotion-honesty-corpus/reports/",
            "hand-written `source_report` stimulus (some carry `command_specs` with null roles) read by check-evidence-promotion-honesty, not producer output",
        )],
    },
    Binding {
        schema_path: "schemas/ripr/repair-assurance.schema.json",
        pointer: Some("/$defs/verification_command_spec"),
        corpus: Corpus::CommandSpecs { verify: true },
        edge_fixtures: &[],
        excluded: &[(
            "fixtures/evidence-promotion-honesty-corpus/reports/",
            "hand-written `source_report` stimulus (some carry `command_specs` with null roles) read by check-evidence-promotion-honesty, not producer output",
        )],
    },
    Binding {
        schema_path: "schemas/ripr/repair-assurance.schema.json",
        pointer: Some("/$defs/execution_result"),
        corpus: Corpus::Authority(
            "narrower authority (#9561): assert_result_matches_published_schema in crates/ripr/tests/cli_smoke.rs checks the result the real `ripr agent verify-execute --result-json` commits per bounded execution; release replay still passes it with --artifact",
        ),
        edge_fixtures: &[],
        excluded: &[],
    },
    Binding {
        schema_path: "schemas/ripr/repair-assurance.schema.json",
        pointer: None,
        corpus: Corpus::Exempt(
            "reserved: `implementation_state` is const `design_only`; no producer emits the envelope (RIPR-SPEC-0135)",
        ),
        edge_fixtures: &[],
        excluded: &[],
    },
    Binding {
        schema_path: "schemas/ripr/rust-repair-trust-corpus.schema.json",
        pointer: None,
        corpus: Corpus::Paths(&["metrics/rust-repair-trust/corpus.json"]),
        edge_fixtures: &[],
        excluded: &[],
    },
    Binding {
        schema_path: "schemas/ripr/ripr-agent-capability.schema.json",
        pointer: None,
        corpus: Corpus::Exempt(
            "narrower authority: capability_output_matches_the_published_schema in crates/ripr/src/lsp/agent_protocol.rs; the schema's cross-file `$ref` is outside this validator",
        ),
        edge_fixtures: &[],
        excluded: &[],
    },
    Binding {
        schema_path: "schemas/ripr/ripr-agent-error.schema.json",
        pointer: None,
        corpus: Corpus::Exempt("reserved envelope; no live producer (#3009)"),
        edge_fixtures: &[],
        excluded: &[],
    },
    Binding {
        schema_path: "schemas/ripr/ripr-agent-request.schema.json",
        pointer: None,
        corpus: Corpus::Exempt("reserved envelope; no live producer (#3009)"),
        edge_fixtures: &[],
        excluded: &[],
    },
    Binding {
        schema_path: "schemas/ripr/ripr-agent-success.schema.json",
        pointer: None,
        corpus: Corpus::Exempt("reserved envelope; no live producer (#3009)"),
        edge_fixtures: &[],
        excluded: &[],
    },
    Binding {
        schema_path: "schemas/ripr/doctor.schema.json",
        pointer: None,
        corpus: Corpus::RuntimeOnly(
            "`ripr doctor --json` writes its report to stdout per run; no committed document holds those bytes",
        ),
        edge_fixtures: &["tests/fixtures/verification/ripr/doctor.valid.json"],
        excluded: &[],
    },
    Binding {
        schema_path: "schemas/ripr/executed-control.schema.json",
        pointer: Some("/$defs/packet"),
        corpus: Corpus::Paths(&[
            "fixtures/executed-control-contract/expected/valid-pass-after-repair.json",
        ]),
        edge_fixtures: &["fixtures/executed-control-contract/expected/issue-3858-not_proven.json"],
        excluded: &[],
    },
    Binding {
        schema_path: "schemas/ripr/ripr-intervention-study.schema.json",
        pointer: None,
        corpus: Corpus::Paths(&["fixtures/intervention-study/valid.json"]),
        edge_fixtures: &[],
        excluded: &[],
    },
];

/// One `--artifact SCHEMA[#POINTER]=FILE[#POINTER]` input: live producer bytes
/// from a qualification run.
struct Artifact {
    schema_path: String,
    schema_pointer: Option<String>,
    file: PathBuf,
    subject_pointer: Option<String>,
}

pub(crate) fn schema_producer_sweep(args: &[String]) -> Result<(), String> {
    let mut options = parse_args(args)?;
    let root = crate::repo_root()?;
    let tree = if options.worktree {
        load_worktree(&root)?
    } else {
        load_tree(&root, &options.rev)?
    };
    if options.live_replay {
        // Exact-cut replay: run every live producer against the fixture from
        // this commit, then validate their bytes alongside the corpus.
        let ripr = options
            .ripr
            .clone()
            .unwrap_or_else(|| root.join("target/debug/ripr"));
        let specs = crate::schema_cut_replay::produce(&root, &tree.commit, &ripr)?;
        for spec in &specs {
            options.artifacts.push(parse_artifact(spec)?);
        }
    }
    let report = sweep(&tree, BINDINGS, &options.artifacts)?;
    let text = serde_json::to_string_pretty(&report.packet)
        .map_err(|error| format!("serialize schema producer sweep: {error}"))?;
    let path = root.join(REPORT_PATH);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("create {}: {error}", parent.display()))?;
    }
    fs::write(&path, format!("{text}\n"))
        .map_err(|error| format!("write {}: {error}", path.display()))?;
    for line in &report.summary {
        println!("{line}");
    }
    println!(
        "Wrote {REPORT_PATH} for commit {} (rows sha256:{})",
        tree.commit, report.digest
    );
    if report.violations.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "schema producer sweep failed:\n{}",
            report
                .violations
                .iter()
                .map(|violation| format!("- {violation}"))
                .collect::<Vec<_>>()
                .join("\n")
        ))
    }
}

/// The changed path that makes `precommit` run the sweep (#12219): a schema, a
/// committed JSON document (producer goldens and corpora), or the sweep's own
/// binding table. Anything else cannot move a sweep verdict.
pub(crate) fn sweep_trigger(changed: &[String]) -> Option<&str> {
    changed.iter().map(String::as_str).find(|path| {
        path.starts_with("schemas/")
            || path.ends_with(".json")
            || *path == "xtask/src/schema_producer_sweep.rs"
    })
}

/// Run the committed-document sweep over the working tree when the diff can
/// move its verdict. Hosted gates run it on the exact commit; this closes the
/// gap that let a golden gain a member its schema lacked (#12209 -> #12219).
pub(crate) fn precommit_schema_sweep(changed: &[String]) -> Result<(), String> {
    let Some(trigger) = sweep_trigger(changed) else {
        println!(
            "schema-producer-sweep: skipped, no schema or JSON document changed vs origin/main"
        );
        return Ok(());
    };
    println!(
        "schema-producer-sweep: running on the working tree because `{trigger}` changed vs origin/main"
    );
    schema_producer_sweep(&["--worktree".to_string()])
}

struct Options {
    rev: String,
    /// Sweep the working tree (tracked plus untracked, not ignored) instead of a
    /// commit, so `precommit` sees an uncommitted schema/golden edit (#12219).
    worktree: bool,
    artifacts: Vec<Artifact>,
    live_replay: bool,
    ripr: Option<PathBuf>,
}

fn parse_args(args: &[String]) -> Result<Options, String> {
    let mut rev = "HEAD".to_string();
    let mut artifacts = Vec::new();
    let mut live_replay = false;
    let mut worktree = false;
    let mut rev_given = false;
    let mut ripr = None;
    let mut remaining = args.iter();
    while let Some(arg) = remaining.next() {
        if arg == "--live-replay" {
            live_replay = true;
            continue;
        }
        if arg == "--worktree" {
            worktree = true;
            continue;
        }
        let value = remaining.next().ok_or_else(|| USAGE.to_string())?;
        match arg.as_str() {
            "--rev" if !value.is_empty() => {
                rev = value.clone();
                rev_given = true;
            }
            "--ripr" if !value.is_empty() => ripr = Some(PathBuf::from(value)),
            "--artifact" => artifacts.push(parse_artifact(value)?),
            _ => return Err(USAGE.to_string()),
        }
    }
    if ripr.is_some() && !live_replay {
        return Err("--ripr only applies with --live-replay".to_string());
    }
    if worktree && (rev_given || live_replay) {
        return Err(
            "--worktree sweeps uncommitted bytes and cannot combine with --rev or --live-replay (replay binds to an exact commit)"
                .to_string(),
        );
    }
    Ok(Options {
        rev,
        worktree,
        artifacts,
        live_replay,
        ripr,
    })
}

fn parse_artifact(spec: &str) -> Result<Artifact, String> {
    let (schema, file) = spec.split_once('=').ok_or_else(|| USAGE.to_string())?;
    let (schema_path, schema_pointer) = split_pointer(schema);
    let (file, subject_pointer) = split_pointer(file);
    if schema_path.is_empty() || file.is_empty() {
        return Err(USAGE.to_string());
    }
    Ok(Artifact {
        schema_path: schema_path.to_string(),
        schema_pointer,
        file: PathBuf::from(file),
        subject_pointer,
    })
}

fn split_pointer(text: &str) -> (&str, Option<String>) {
    match text.split_once('#') {
        Some((head, pointer)) => (head, Some(pointer.to_string())),
        None => (text, None),
    }
}

struct SweepReport {
    packet: Value,
    summary: Vec<String>,
    violations: Vec<String>,
    digest: String,
}

/// Counts for one binding row.
#[derive(Default)]
struct RowCounts {
    discovered: usize,
    validated: usize,
    edge: usize,
    excluded: usize,
    artifacts: usize,
    subjects: Vec<Value>,
}

fn sweep(
    tree: &CandidateTree,
    bindings: &[Binding],
    artifacts: &[Artifact],
) -> Result<SweepReport, String> {
    let mut violations = Vec::new();
    let inventory: Vec<String> = tree
        .files
        .keys()
        .filter(|path| path.starts_with(SCHEMAS_PREFIX))
        .cloned()
        .collect();
    if inventory.is_empty() {
        violations.push(
            "schemas/ holds no published schema, so the sweep has no denominator".to_string(),
        );
    }
    for schema_path in &inventory {
        if !bindings
            .iter()
            .any(|binding| binding.schema_path == schema_path)
        {
            violations.push(format!(
                "{schema_path} has no sweep binding; add a corpus rule, a runtime-only route, or a reviewed exemption"
            ));
        }
    }

    let CommittedDocuments {
        parsed: documents,
        malformed,
    } = committed_documents(tree, bindings);
    let mut schemas = BTreeMap::new();
    let mut exact_schemas = BTreeMap::new();
    for schema_path in &inventory {
        let bytes = &tree.files[schema_path];
        let schema: Value = serde_json::from_slice(bytes)
            .map_err(|error| format!("parse {schema_path} at {}: {error}", tree.commit))?;
        let exact = crate::exact_schema_json::parse(bytes)
            .map_err(|error| format!("parse exact {schema_path} at {}: {error}", tree.commit))?;
        exact_schemas.insert(schema_path.clone(), exact);
        schemas.insert(schema_path.clone(), (schema, sha256(bytes)));
    }

    let mut rows = Vec::new();
    let mut summary = Vec::new();
    for binding in bindings {
        let Some((schema, schema_digest)) = schemas.get(binding.schema_path) else {
            violations.push(format!(
                "binding names {} but no such published schema exists",
                binding.schema_path
            ));
            continue;
        };
        let exact_schema = &exact_schemas[binding.schema_path];
        let exact_subschema = match binding.pointer {
            None => exact_schema,
            Some(pointer) => match exact_schema.pointer(pointer) {
                Some(value) => value,
                None => {
                    violations.push(format!(
                        "{} has no exact subschema `{pointer}`",
                        binding.schema_path
                    ));
                    continue;
                }
            },
        };
        let label = row_label(binding.schema_path, binding.pointer);
        let mut counts = RowCounts::default();

        for (path, (digest, error)) in &malformed {
            if !corpus_binds_path(&binding.corpus, path)
                || binding
                    .excluded
                    .iter()
                    .any(|(prefix, _)| path.starts_with(prefix))
            {
                continue;
            }
            counts.discovered += 1;
            counts
                .subjects
                .push(json!({ "path": path, "sha256": digest, "role": "producer" }));
            violations.push(format!(
                "parse bound producer {path} at {}: {error}",
                tree.commit
            ));
        }

        for (path, (value, digest)) in &documents {
            let subjects = corpus_subjects(&binding.corpus, path, value);
            // This cheap shape filter deliberately ignores numeric and compound
            // constants. Their equality is decided from original bytes below.
            let possible_lookalike =
                subjects.is_empty() && possible_lookalike(binding, schema, path, value);
            if subjects.is_empty() && !possible_lookalike {
                continue;
            }
            let exclusion = binding
                .excluded
                .iter()
                .find(|(prefix, _)| path.starts_with(prefix));
            // Bound subjects remain excluded without needing to parse them.
            // Possible unbound lookalikes must first actually match.
            if !possible_lookalike && let Some((_, reason)) = exclusion {
                counts.excluded += 1;
                counts.subjects.push(
                    json!({ "path": path, "sha256": digest, "role": "excluded", "reason": reason }),
                );
                continue;
            }
            let exact_document = match crate::exact_schema_json::parse(&tree.files[path]) {
                Ok(value) => value,
                Err(error) => {
                    violations.push(format!("parse exact {path}: {error}"));
                    continue;
                }
            };
            if possible_lookalike {
                match exact_lookalike(exact_schema, &exact_document) {
                    Ok(true) => {
                        if let Some((_, reason)) = exclusion {
                            counts.excluded += 1;
                            counts.subjects.push(json!({ "path": path, "sha256": digest, "role": "excluded", "reason": reason }));
                        } else {
                            violations.push(format!("{path} looks like {label} output but no binding claims it; bind it or add a reviewed exclusion"));
                        }
                    }
                    Ok(false) => {}
                    Err(error) => violations
                        .push(format!("{path}: cannot determine exact lookalike: {error}")),
                }
                continue;
            }
            let exact_subjects = corpus_subjects_exact(&binding.corpus, path, &exact_document);
            if exact_subjects
                .iter()
                .map(|(location, _)| location)
                .ne(subjects.iter().map(|(location, _)| location))
            {
                violations.push(format!(
                    "{path}: exact and ordinary subject selection disagree"
                ));
                continue;
            }
            for (location, subject) in exact_subjects {
                counts.discovered += 1;
                let before = violations.len();
                validate_exact_value_against_schema(
                    subject,
                    exact_subschema,
                    exact_schema,
                    location,
                    &mut violations,
                );
                if violations.len() == before {
                    counts.validated += 1;
                }
            }
            counts
                .subjects
                .push(json!({ "path": path, "sha256": digest, "role": "producer" }));
        }

        for edge in binding.edge_fixtures {
            let Some((_value, digest)) = documents.get(*edge) else {
                violations.push(format!(
                    "{label} names edge fixture {edge}, which does not exist"
                ));
                continue;
            };
            let exact_value = match crate::exact_schema_json::parse(&tree.files[*edge]) {
                Ok(value) => value,
                Err(error) => {
                    violations.push(format!("parse exact {edge}: {error}"));
                    continue;
                }
            };
            let before = violations.len();
            validate_exact_value_against_schema(
                &exact_value,
                exact_subschema,
                exact_schema,
                (*edge).to_string(),
                &mut violations,
            );
            if violations.len() == before {
                counts.edge += 1;
            }
            counts
                .subjects
                .push(json!({ "path": edge, "sha256": digest, "role": "edge_fixture" }));
        }

        for artifact in artifacts.iter().filter(|artifact| {
            artifact.schema_path == binding.schema_path
                && artifact.schema_pointer.as_deref() == binding.pointer
        }) {
            let bytes = fs::read(&artifact.file)
                .map_err(|error| format!("read {}: {error}", artifact.file.display()))?;
            let value = crate::exact_schema_json::parse(&bytes)
                .map_err(|error| format!("parse {}: {error}", artifact.file.display()))?;
            let location = format!(
                "{}{}",
                artifact.file.display(),
                artifact.subject_pointer.as_deref().unwrap_or_default()
            );
            let subject = match artifact.subject_pointer.as_deref() {
                None => Some(&value),
                Some(pointer) => value.pointer(pointer),
            };
            let Some(subject) = subject else {
                violations.push(format!("{location} does not resolve to a subject"));
                continue;
            };
            let before = violations.len();
            validate_exact_value_against_schema(
                subject,
                exact_subschema,
                exact_schema,
                location.clone(),
                &mut violations,
            );
            if violations.len() == before {
                counts.artifacts += 1;
            }
            counts
                .subjects
                .push(json!({ "path": location, "sha256": sha256(&bytes), "role": "artifact" }));
        }

        let disposition = match &binding.corpus {
            Corpus::Exempt(reason) => {
                summary.push(format!("{label}: exempt ({reason})"));
                json!({ "state": "exempt", "reason": reason })
            }
            Corpus::Authority(authority) => {
                let state = if counts.artifacts > 0 {
                    "artifact_validated"
                } else {
                    "authority_only"
                };
                summary.push(format!(
                    "{label}: {state}, {} live artifact(s) validated ({authority})",
                    counts.artifacts
                ));
                json!({ "state": state, "authority": authority })
            }
            Corpus::RuntimeOnly(route) => {
                let state = if counts.artifacts > 0 {
                    "artifact_validated"
                } else {
                    "not_run"
                };
                summary.push(format!(
                    "{label}: runtime-only, {} live artifact(s) validated{}",
                    counts.artifacts,
                    if counts.artifacts == 0 {
                        " (not_run: pass --artifact from a live run)"
                    } else {
                        ""
                    }
                ));
                json!({ "state": state, "route": route })
            }
            Corpus::FileName(_) | Corpus::Paths(_) | Corpus::CommandSpecs { .. } => {
                // A corpus rule that binds nothing is a sweep that did not run.
                if counts.discovered == 0 {
                    violations.push(format!("{label} bound no committed producer subject"));
                }
                summary.push(format!(
                    "{label}: {}/{} producer subjects valid, {} edge fixture(s), {} excluded stimulus, {} live artifact(s)",
                    counts.validated, counts.discovered, counts.edge, counts.excluded, counts.artifacts
                ));
                json!({ "state": if counts.discovered > 0 && counts.validated == counts.discovered { "corpus_validated" } else { "failed" } })
            }
        };
        rows.push(json!({
            "schema": binding.schema_path,
            "subschema": binding.pointer,
            "schema_sha256": schema_digest,
            "disposition": disposition,
            "producer_subjects_discovered": counts.discovered,
            "producer_subjects_valid": counts.validated,
            "edge_fixtures_valid": counts.edge,
            "excluded_stimulus": counts.excluded,
            "live_artifacts_valid": counts.artifacts,
            "subjects": counts.subjects,
        }));
    }

    // An artifact no row claims would otherwise be dropped without a trace,
    // and a mistyped schema path would read as a clean run.
    for artifact in artifacts {
        let claimed = bindings.iter().any(|binding| {
            artifact.schema_path == binding.schema_path
                && artifact.schema_pointer.as_deref() == binding.pointer
        });
        if !claimed {
            violations.push(format!(
                "--artifact {} names {}{}, which no sweep row validates",
                artifact.file.display(),
                artifact.schema_path,
                artifact
                    .schema_pointer
                    .as_deref()
                    .map(|pointer| format!("#{pointer}"))
                    .unwrap_or_default()
            ));
        }
    }

    let rows = Value::Array(rows);
    let digest = sha256(rows.to_string().as_bytes());
    let packet = json!({
        "schema_version": "0.1",
        "kind": "schema_producer_sweep",
        "commit": tree.commit,
        "validator": "xtask/src/verification_contracts.rs validate_value_against_schema",
        "published_schemas": inventory,
        "rows": rows,
        "rows_sha256": digest,
        "status": if violations.is_empty() { "pass" } else { "fail" },
        "violations": violations,
    });
    Ok(SweepReport {
        packet,
        summary,
        violations,
        digest,
    })
}

fn row_label(schema_path: &str, pointer: Option<&str>) -> String {
    match pointer {
        None => schema_path.to_string(),
        Some(pointer) => format!("{schema_path}#{pointer}"),
    }
}

/// The subjects `corpus` binds in one committed document, with locations.
fn corpus_subjects<'a>(corpus: &Corpus, path: &str, value: &'a Value) -> Vec<(String, &'a Value)> {
    match corpus {
        Corpus::FileName(name) if file_name(path) == *name => vec![(path.to_string(), value)],
        Corpus::Paths(paths) if paths.contains(&path) => vec![(path.to_string(), value)],
        Corpus::CommandSpecs { verify } => {
            let mut subjects = Vec::new();
            collect_command_specs(value, path.to_string(), *verify, &mut subjects);
            subjects
        }
        _ => Vec::new(),
    }
}

fn collect_command_specs<'a>(
    value: &'a Value,
    location: String,
    verify: bool,
    subjects: &mut Vec<(String, &'a Value)>,
) {
    match value {
        Value::Object(object) => {
            for (key, child) in object {
                let child_location = format!("{location}/{key}");
                if key == "command_specs"
                    && let Some(specs) = child.as_object()
                {
                    for (role, spec) in specs {
                        if (role == "verify") == verify {
                            subjects.push((format!("{child_location}/{role}"), spec));
                        }
                    }
                    continue;
                }
                collect_command_specs(child, child_location, verify, subjects);
            }
        }
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                collect_command_specs(item, format!("{location}/{index}"), verify, subjects);
            }
        }
        _ => {}
    }
}

fn corpus_subjects_exact<'a>(
    corpus: &Corpus,
    path: &str,
    value: &'a ExactValue,
) -> Vec<(String, &'a ExactValue)> {
    match corpus {
        Corpus::FileName(name) if file_name(path) == *name => vec![(path.to_string(), value)],
        Corpus::Paths(paths) if paths.contains(&path) => vec![(path.to_string(), value)],
        Corpus::CommandSpecs { verify } => {
            let mut subjects = Vec::new();
            collect_command_specs_exact(value, path.to_string(), *verify, &mut subjects);
            subjects
        }
        _ => Vec::new(),
    }
}

fn collect_command_specs_exact<'a>(
    value: &'a ExactValue,
    location: String,
    verify: bool,
    subjects: &mut Vec<(String, &'a ExactValue)>,
) {
    match value {
        ExactValue::Object(object) => {
            for (key, child) in object {
                let child_location = format!("{location}/{key}");
                if key == "command_specs"
                    && let Some(specs) = child.as_object()
                {
                    for (role, spec) in specs {
                        if (role == "verify") == verify {
                            subjects.push((format!("{child_location}/{role}"), spec));
                        }
                    }
                    continue;
                }
                collect_command_specs_exact(child, child_location, verify, subjects);
            }
        }
        ExactValue::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                collect_command_specs_exact(item, format!("{location}/{index}"), verify, subjects);
            }
        }
        _ => {}
    }
}

/// Cheap shape filter for a possible unbound producer. Numeric and compound
/// constants are deliberately deferred to exact_lookalike, which uses the
/// original bytes and fails closed when equality cannot be established.
fn possible_lookalike(binding: &Binding, schema: &Value, path: &str, value: &Value) -> bool {
    if binding.pointer.is_some()
        || matches!(binding.corpus, Corpus::Exempt(_))
        || binding.edge_fixtures.contains(&path)
    {
        return false;
    }
    let (Some(object), Some(properties)) = (
        value.as_object(),
        schema.get("properties").and_then(Value::as_object),
    ) else {
        return false;
    };
    let constants_match = properties
        .iter()
        .all(|(key, property)| match property.get("const") {
            Some(expected)
                if expected.is_number() || expected.is_object() || expected.is_array() =>
            {
                true
            }
            Some(expected) => object.get(key) == Some(expected),
            None => true,
        });
    let required_present = schema
        .get("required")
        .and_then(Value::as_array)
        .is_some_and(|required| {
            required
                .iter()
                .filter_map(Value::as_str)
                .all(|key| object.contains_key(key))
        });
    constants_match && required_present
}

fn exact_lookalike(schema: &ExactValue, value: &ExactValue) -> Result<bool, String> {
    let (Some(object), Some(properties)) = (
        value.as_object(),
        schema.get("properties").and_then(ExactValue::as_object),
    ) else {
        return Ok(false);
    };
    for (key, property) in properties {
        if let Some(expected) = property.get("const") {
            let Some(actual) = object.get(key) else {
                return Ok(false);
            };
            if !exact_schema_values_equal(actual, expected)? {
                return Ok(false);
            }
        }
    }
    Ok(schema
        .get("required")
        .and_then(ExactValue::as_array)
        .is_some_and(|required| {
            required
                .iter()
                .filter_map(ExactValue::as_str)
                .all(|key| object.contains_key(key))
        }))
}

fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// The `.json` blobs of one commit: the sweep's only input besides
/// `--artifact` files.
struct CandidateTree {
    commit: String,
    files: BTreeMap<String, Vec<u8>>,
}

/// Read every `.json` blob of `rev` in `root` through one `git archive`, so
/// nothing untracked or uncommitted can reach the sweep.
fn load_tree(root: &Path, rev: &str) -> Result<CandidateTree, String> {
    let commit = git(
        root,
        &["rev-parse", "--verify", &format!("{rev}^{{commit}}")],
    )?;
    let commit = String::from_utf8_lossy(&commit).trim().to_string();
    archive_json_blobs(root, &commit, commit.clone())
}

/// Snapshot the working tree (HEAD plus every uncommitted, non-ignored change)
/// into a tree object through a private index, so the user's index is never
/// touched, then read it through the same `git archive` path as a commit.
fn load_worktree(root: &Path) -> Result<CandidateTree, String> {
    let index = root.join("target/ripr/schema-sweep-worktree.index");
    if let Some(parent) = index.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("create {}: {error}", parent.display()))?;
    }
    let _ = fs::remove_file(&index);
    let index_text = index.to_string_lossy().into_owned();
    let envs = [("GIT_INDEX_FILE", index_text.as_str())];
    let run = |args: &[&str]| -> Result<Vec<u8>, String> {
        let args: Vec<String> = args.iter().map(|arg| (*arg).to_string()).collect();
        crate::run::capture_process_output_in("git", &args, Some(root), &[], &[], &envs)
            .map_err(|error| error.message)
    };
    let snapshot = (|| {
        run(&["read-tree", "HEAD"])?;
        run(&["add", "-A", "--", "."])?;
        run(&["write-tree"])
    })();
    let _ = fs::remove_file(&index);
    let tree = String::from_utf8_lossy(&snapshot?).trim().to_string();
    archive_json_blobs(root, &tree, format!("worktree-tree:{tree}"))
}

fn archive_json_blobs(root: &Path, tree_ish: &str, label: String) -> Result<CandidateTree, String> {
    let commit = label;
    let archive = git(
        root,
        &[
            "archive",
            "--format=tar",
            tree_ish,
            "--",
            ":(glob)**/*.json",
        ],
    )?;
    let mut files = BTreeMap::new();
    let mut entries = tar::Archive::new(archive.as_slice());
    for entry in entries
        .entries()
        .map_err(|error| format!("read git archive of {commit}: {error}"))?
    {
        let mut entry = entry.map_err(|error| format!("read git archive of {commit}: {error}"))?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let path = entry
            .path()
            .map_err(|error| format!("read git archive path: {error}"))?
            .to_string_lossy()
            .replace('\\', "/");
        let mut bytes = Vec::new();
        entry
            .read_to_end(&mut bytes)
            .map_err(|error| format!("read {path} from {commit}: {error}"))?;
        files.insert(path, bytes);
    }
    Ok(CandidateTree { commit, files })
}

fn git(root: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let args: Vec<String> = args.iter().map(|arg| (*arg).to_string()).collect();
    crate::run::capture_process_output_in("git", &args, Some(root), &[], &[], &[])
        .map_err(|error| error.message)
}

/// Parse committed documents outside `schemas/`. A malformed file selected by
/// a path or filename binding is a failed producer, not a missing subject.
/// Other files rejected by serde_json (including JSONC stimulus and numbers
/// outside its finite range) cannot be classified as unbound lookalikes.
struct CommittedDocuments {
    parsed: BTreeMap<String, (Value, String)>,
    malformed: BTreeMap<String, (String, String)>,
}

fn committed_documents(tree: &CandidateTree, bindings: &[Binding]) -> CommittedDocuments {
    let mut documents = BTreeMap::new();
    let mut malformed = BTreeMap::new();
    for (path, bytes) in tree
        .files
        .iter()
        .filter(|(path, _)| !path.starts_with(SCHEMAS_PREFIX))
    {
        match serde_json::from_slice::<Value>(bytes) {
            Ok(value) => {
                documents.insert(path.clone(), (value, sha256(bytes)));
            }
            Err(error)
                if bindings.iter().any(|binding| {
                    corpus_binds_path(&binding.corpus, path)
                        && !binding
                            .excluded
                            .iter()
                            .any(|(prefix, _)| path.starts_with(prefix))
                }) =>
            {
                malformed.insert(path.clone(), (sha256(bytes), error.to_string()));
            }
            Err(_) => {}
        }
    }
    CommittedDocuments {
        parsed: documents,
        malformed,
    }
}

fn corpus_binds_path(corpus: &Corpus, path: &str) -> bool {
    match corpus {
        Corpus::FileName(name) => file_name(path) == *name,
        Corpus::Paths(paths) => paths.contains(&path),
        _ => false,
    }
}

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{temp_dir, write};

    const EXAMPLE_SCHEMA: &str = "schemas/ripr/example.schema.json";

    /// A closed schema with the two things the sweep keys on: a `const`
    /// discriminator and required fields.
    fn example_root() -> PathBuf {
        let root = temp_dir("schema-producer-sweep");
        write(
            &root.join(EXAMPLE_SCHEMA),
            r#"{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "https://example.invalid/example.schema.json",
  "type": "object",
  "additionalProperties": false,
  "required": ["schema_version", "status"],
  "properties": {
    "schema_version": { "const": "0.1" },
    "status": { "enum": ["pass", "fail"] }
  }
}"#,
        );
        root
    }

    fn binding(corpus: Corpus, excluded: &'static [(&'static str, &'static str)]) -> Binding {
        Binding {
            schema_path: EXAMPLE_SCHEMA,
            pointer: None,
            corpus,
            edge_fixtures: &[],
            excluded,
        }
    }

    /// Commit everything under `root` except `live.json`, which stands for a
    /// per-run artifact outside the candidate.
    fn commit(root: &Path) -> Result<(), String> {
        for args in [
            vec!["init", "-q"],
            vec!["add", "-A", "--", ".", ":(exclude)live.json"],
            vec![
                "-c",
                "user.name=sweep",
                "-c",
                "user.email=sweep@example.invalid",
                "-c",
                "commit.gpgsign=false",
                "-c",
                "core.hooksPath=/dev/null",
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                "candidate",
            ],
        ] {
            git(root, &args)?;
        }
        Ok(())
    }

    fn sweep_committed(
        root: &Path,
        bindings: &[Binding],
        artifacts: &[Artifact],
    ) -> Result<SweepReport, String> {
        commit(root)?;
        sweep(&load_tree(root, "HEAD")?, bindings, artifacts)
    }

    fn run(
        root: &Path,
        bindings: &[Binding],
        artifacts: &[Artifact],
    ) -> Result<SweepReport, String> {
        let report = sweep_committed(root, bindings, artifacts);
        let _ = fs::remove_dir_all(root);
        report
    }

    #[test]
    fn unbound_numeric_const_lookalikes_use_exact_values() -> Result<(), String> {
        for (number, expect_lookalike) in [
            ("1", true),
            ("1.0", true),
            ("1e0", true),
            ("1.5", false),
            ("1.00000000000000001", false),
            ("\"1\"", false),
        ] {
            let schema =
                include_bytes!("../../schemas/badges/shields-endpoint.schema.json").to_vec();
            let tree = CandidateTree {
                commit: "fixture".into(),
                files: BTreeMap::from([
                    (EXAMPLE_SCHEMA.into(), schema),
                    ("bound.json".into(), br#"{"schemaVersion":1,"label":"x","message":"x","color":"blue"}"#.to_vec()),
                    ("unbound.json".into(), format!(r#"{{"schemaVersion":{number},"label":"x","message":"x","color":"blue"}}"#).into_bytes()),
                ]),
            };
            let report = sweep(&tree, &[binding(Corpus::Paths(&["bound.json"]), &[])], &[])?;
            assert_eq!(
                report
                    .violations
                    .iter()
                    .any(|item| item.contains("unbound.json looks like")),
                expect_lookalike,
                "{number}: {:?}",
                report.violations
            );
        }
        Ok(())
    }

    #[test]
    fn unbound_lookalike_comparison_fails_closed_on_unsupported_numbers() -> Result<(), String> {
        let schema = include_bytes!("../../schemas/badges/shields-endpoint.schema.json").to_vec();
        let tree = CandidateTree {
            commit: "fixture".into(),
            files: BTreeMap::from([
                (EXAMPLE_SCHEMA.into(), schema),
                (
                    "bound.json".into(),
                    br#"{"schemaVersion":1,"label":"x","message":"x","color":"blue"}"#.to_vec(),
                ),
                (
                    "unbound.json".into(),
                    br#"{"schemaVersion":1e300,"label":"x","message":"x","color":"blue"}"#.to_vec(),
                ),
            ]),
        };
        let report = sweep(&tree, &[binding(Corpus::Paths(&["bound.json"]), &[])], &[])?;
        assert!(
            report
                .violations
                .iter()
                .any(|item| item.contains("cannot determine exact lookalike")),
            "{:?}",
            report.violations
        );
        Ok(())
    }

    #[test]
    fn exclusions_count_only_matching_lookalikes_and_bound_subjects() -> Result<(), String> {
        let schema = include_bytes!("../../schemas/badges/shields-endpoint.schema.json").to_vec();
        let tree = CandidateTree {
            commit: "fixture".into(),
            files: BTreeMap::from([
                (EXAMPLE_SCHEMA.into(), schema),
                (
                    "bound.json".into(),
                    br#"{"schemaVersion":1,"label":"x","message":"x","color":"blue"}"#.to_vec(),
                ),
                (
                    "excluded/bound.json".into(),
                    br#"{"schemaVersion":1.5,"label":"x","message":"x","color":"blue"}"#.to_vec(),
                ),
                (
                    "excluded/match.json".into(),
                    br#"{"schemaVersion":1.0,"label":"x","message":"x","color":"blue"}"#.to_vec(),
                ),
                (
                    "excluded/mismatch.json".into(),
                    br#"{"schemaVersion":1.5,"label":"x","message":"x","color":"blue"}"#.to_vec(),
                ),
            ]),
        };
        let report = sweep(
            &tree,
            &[binding(
                Corpus::Paths(&["bound.json", "excluded/bound.json"]),
                &[("excluded/", "stimulus")],
            )],
            &[],
        )?;
        assert!(report.violations.is_empty(), "{:?}", report.violations);
        assert_eq!(report.packet["rows"][0]["excluded_stimulus"], 2);
        Ok(())
    }

    #[test]
    fn serde_rejected_unbound_documents_cannot_be_classified() {
        let bytes = br#"{"schemaVersion":1e400,"label":"x","message":"x","color":"blue"}"#;
        let tree = CandidateTree {
            commit: "fixture".into(),
            files: BTreeMap::from([
                ("bound.json".into(), bytes.to_vec()),
                ("unbound.json".into(), bytes.to_vec()),
            ]),
        };
        let documents = committed_documents(&tree, &[binding(Corpus::Paths(&["bound.json"]), &[])]);
        assert!(documents.malformed.contains_key("bound.json"));
        assert!(!documents.parsed.contains_key("unbound.json"));
        assert!(!documents.malformed.contains_key("unbound.json"));
    }

    #[test]
    fn nested_numeric_constants_have_mathematical_lookalike_equality() -> Result<(), String> {
        let schema = crate::exact_schema_json::parse(
            br#"{"required":["k"],"properties":{"k":{"const":{"v":[1,0]}}}}"#,
        )?;
        for (input, expected) in [
            (r#"{"k":{"v":[1.0,-0.0]}}"#, true),
            (r#"{"k":{"v":[1.00000000000000001,0]}}"#, false),
            (r#"{"k":{"v":["1",0]}}"#, false),
        ] {
            let value = crate::exact_schema_json::parse(input.as_bytes())?;
            assert_eq!(exact_lookalike(&schema, &value)?, expected, "{input}");
        }
        Ok(())
    }

    #[test]
    fn sweep_validates_original_decimal_bytes() -> Result<(), String> {
        for (number, valid) in [("1.0", true), ("1.00000000000000001", false)] {
            let tree = CandidateTree {
                commit: "fixture".into(),
                files: BTreeMap::from([
                    (EXAMPLE_SCHEMA.into(), br#"{"type":"integer"}"#.to_vec()),
                    ("fixtures/example.json".into(), number.as_bytes().to_vec()),
                ]),
            };
            let report = sweep(
                &tree,
                &[binding(Corpus::FileName("example.json"), &[])],
                &[],
            )?;
            assert_eq!(
                report.violations.is_empty(),
                valid,
                "{number}: {:?}",
                report.violations
            );
        }
        Ok(())
    }

    #[test]
    fn sweep_selects_exact_nested_producer_and_artifact_values() -> Result<(), String> {
        let root = temp_dir("schema-exact-artifact");
        let artifact = root.join("live.json");
        let tree = CandidateTree {
            commit: "fixture".into(),
            files: BTreeMap::from([
                (EXAMPLE_SCHEMA.into(), br#"{"type":"integer"}"#.to_vec()),
                (
                    "producer.json".into(),
                    br#"{"command_specs":{"verify":1.00000000000000001}}"#.to_vec(),
                ),
            ]),
        };
        let bindings = [binding(Corpus::CommandSpecs { verify: true }, &[])];
        write(&artifact, r#"{"a/b":[1.00000000000000001]}"#);
        let artifacts = [Artifact {
            schema_path: EXAMPLE_SCHEMA.into(),
            schema_pointer: None,
            file: artifact,
            subject_pointer: Some("/a~1b/0".into()),
        }];
        let report = sweep(&tree, &bindings, &artifacts)?;
        let _ = fs::remove_dir_all(root);
        assert_eq!(
            report
                .violations
                .iter()
                .filter(|item| item.contains("expected type integer"))
                .count(),
            2,
            "{:?}",
            report.violations
        );
        Ok(())
    }

    #[test]
    fn a_valid_bound_producer_document_is_counted_as_validated() -> Result<(), String> {
        let root = example_root();
        write(
            &root.join("fixtures/a/expected/example.json"),
            r#"{"schema_version":"0.1","status":"pass"}"#,
        );
        let report = run(
            &root,
            &[binding(Corpus::FileName("example.json"), &[])],
            &[],
        )?;
        if !report.violations.is_empty() {
            return Err(format!("unexpected violations: {:?}", report.violations));
        }
        let row = &report.packet["rows"][0];
        if row["producer_subjects_discovered"] != 1 || row["producer_subjects_valid"] != 1 {
            return Err(format!("expected one validated subject: {row}"));
        }
        if row["disposition"]["state"] != "corpus_validated" {
            return Err(format!("expected corpus_validated: {row}"));
        }
        Ok(())
    }

    #[test]
    fn a_bound_document_missing_a_required_field_fails_even_though_it_no_longer_looks_alike()
    -> Result<(), String> {
        // Binding is by file name, not by shape, so a producer that drops a
        // required field cannot make its own golden fall out of the sweep.
        let root = example_root();
        write(
            &root.join("fixtures/a/expected/example.json"),
            r#"{"schema_version":"0.1"}"#,
        );
        let report = run(
            &root,
            &[binding(Corpus::FileName("example.json"), &[])],
            &[],
        )?;
        if !report
            .violations
            .iter()
            .any(|violation| violation.contains("status"))
        {
            return Err(format!(
                "the missing required field was not reported: {:?}",
                report.violations
            ));
        }
        if report.packet["rows"][0]["disposition"]["state"] != "failed" {
            return Err("a failed subject must not leave the row corpus_validated".to_string());
        }
        Ok(())
    }

    #[test]
    fn malformed_bound_producer_json_fails_with_its_committed_path() -> Result<(), String> {
        for corpus in [
            Corpus::FileName("example.json"),
            Corpus::Paths(&["fixtures/a/expected/example.json"]),
        ] {
            let root = example_root();
            write(
                &root.join("fixtures/a/expected/example.json"),
                r#"{"schema_version":"0.1","status":"pass""#,
            );
            let report = run(&root, &[binding(corpus, &[])], &[])?;
            let row = &report.packet["rows"][0];
            if row["disposition"]["state"] != "failed"
                || row["producer_subjects_discovered"] != 1
                || row["producer_subjects_valid"] != 0
                || !report.violations.iter().any(|violation| {
                    violation.contains("fixtures/a/expected/example.json")
                        && violation.contains("parse")
                })
            {
                return Err(format!(
                    "malformed bound producer escaped the sweep: {row}; {:?}",
                    report.violations
                ));
            }
        }
        Ok(())
    }

    #[test]
    fn non_object_command_spec_roles_fail_in_both_binding_rows() -> Result<(), String> {
        for (role, verify) in [("verify", true), ("build", false)] {
            let root = example_root();
            write(
                &root.join("fixtures/a/expected/packet.json"),
                &format!(r#"{{"command_specs":{{"{role}":null}}}}"#),
            );
            let report = run(&root, &[binding(Corpus::CommandSpecs { verify }, &[])], &[])?;
            let row = &report.packet["rows"][0];
            if row["disposition"]["state"] != "failed"
                || row["producer_subjects_discovered"] != 1
                || row["producer_subjects_valid"] != 0
                || !report.violations.iter().any(|violation| {
                    violation.contains(&format!("packet.json/command_specs/{role}"))
                })
            {
                return Err(format!(
                    "non-object {role} spec escaped the sweep: {row}; {:?}",
                    report.violations
                ));
            }
        }
        Ok(())
    }

    #[test]
    fn an_unbound_lookalike_fails_until_bound_or_excluded() -> Result<(), String> {
        let root = example_root();
        write(
            &root.join("fixtures/a/expected/example.json"),
            r#"{"schema_version":"0.1","status":"pass"}"#,
        );
        write(
            &root.join("fixtures/a/inputs/renamed.json"),
            r#"{"schema_version":"0.1","status":"pass"}"#,
        );
        let unbound = sweep_committed(
            &root,
            &[binding(Corpus::FileName("example.json"), &[])],
            &[],
        )?;
        if !unbound
            .violations
            .iter()
            .any(|violation| violation.contains("fixtures/a/inputs/renamed.json"))
        {
            let _ = fs::remove_dir_all(&root);
            return Err(format!(
                "the renamed lookalike was not reported: {:?}",
                unbound.violations
            ));
        }
        let excluded = run(
            &root,
            &[binding(
                Corpus::FileName("example.json"),
                &[("fixtures/a/inputs/", "stimulus")],
            )],
            &[],
        )?;
        if !excluded.violations.is_empty() || excluded.packet["rows"][0]["excluded_stimulus"] != 1 {
            return Err(format!(
                "a reviewed exclusion must count, not fail: {}",
                excluded.packet
            ));
        }
        Ok(())
    }

    #[test]
    fn a_corpus_rule_that_binds_nothing_is_a_sweep_that_did_not_run() -> Result<(), String> {
        let root = example_root();
        let report = run(
            &root,
            &[binding(Corpus::FileName("example.json"), &[])],
            &[],
        )?;
        if !report
            .violations
            .iter()
            .any(|violation| violation.contains("bound no committed producer subject"))
        {
            return Err(format!("zero subjects passed: {:?}", report.violations));
        }
        Ok(())
    }

    #[test]
    fn a_published_schema_without_a_binding_fails() -> Result<(), String> {
        let root = example_root();
        let report = run(&root, &[], &[])?;
        if !report
            .violations
            .iter()
            .any(|violation| violation.contains("has no sweep binding"))
        {
            return Err(format!("an unbound schema passed: {:?}", report.violations));
        }
        Ok(())
    }

    #[test]
    fn runtime_only_rows_validate_supplied_artifacts_and_report_not_run_without_them()
    -> Result<(), String> {
        let root = example_root();
        let artifact = root.join("live.json");
        write(
            &artifact,
            r#"{"result":{"schema_version":"0.1","status":"maybe"}}"#,
        );
        let runtime = || binding(Corpus::RuntimeOnly("per-run bytes"), &[]);

        let not_run = sweep_committed(&root, &[runtime()], &[])?;
        if !not_run.violations.is_empty()
            || not_run.packet["rows"][0]["disposition"]["state"] != "not_run"
        {
            let _ = fs::remove_dir_all(&root);
            return Err(format!(
                "a runtime-only row without artifacts must be not_run: {}",
                not_run.packet
            ));
        }

        let supplied = run(
            &root,
            &[runtime()],
            &[Artifact {
                schema_path: EXAMPLE_SCHEMA.to_string(),
                schema_pointer: None,
                file: artifact.clone(),
                subject_pointer: Some("/result".to_string()),
            }],
        )?;
        if !supplied
            .violations
            .iter()
            .any(|violation| violation.contains("live.json/result"))
        {
            return Err(format!(
                "an invalid live artifact passed: {:?}",
                supplied.violations
            ));
        }
        Ok(())
    }

    #[test]
    fn authority_rows_report_artifact_validated_or_authority_only_and_fail_bad_artifacts()
    -> Result<(), String> {
        let root = example_root();
        let good = root.join("live.json");
        write(&good, r#"{"schema_version":"0.1","status":"pass"}"#);
        let authority = || binding(Corpus::Authority("named test"), &[]);
        let artifact = |file: &Path| Artifact {
            schema_path: EXAMPLE_SCHEMA.to_string(),
            schema_pointer: None,
            file: file.to_path_buf(),
            subject_pointer: None,
        };

        let none = sweep_committed(&root, &[authority()], &[])?;
        let state = |report: &SweepReport| report.packet["rows"][0]["disposition"]["state"].clone();
        if !none.violations.is_empty() || state(&none) != "authority_only" {
            let _ = fs::remove_dir_all(&root);
            return Err(format!(
                "no artifact must be authority_only: {}",
                none.packet
            ));
        }
        let ok = sweep_committed(&root, &[authority()], &[artifact(&good)])?;
        if !ok.violations.is_empty() || state(&ok) != "artifact_validated" {
            let _ = fs::remove_dir_all(&root);
            return Err(format!("a good artifact must validate: {}", ok.packet));
        }
        write(&good, r#"{"schema_version":"0.1","status":"maybe"}"#);
        let bad = run(&root, &[authority()], &[artifact(&good)])?;
        if bad.violations.is_empty() {
            return Err("a bad artifact on an authority row passed".to_string());
        }
        Ok(())
    }

    #[test]
    fn a_committed_lookalike_of_an_authority_row_fails_until_bound_or_excluded()
    -> Result<(), String> {
        let root = example_root();
        write(
            &root.join("fixtures/x/attempt.json"),
            r#"{"schema_version":"0.1","status":"maybe"}"#,
        );
        let report = run(&root, &[binding(Corpus::Authority("named test"), &[])], &[])?;
        if !report
            .violations
            .iter()
            .any(|violation| violation.contains("fixtures/x/attempt.json"))
        {
            return Err(format!(
                "a committed lookalike was silently skipped: {:?}",
                report.violations
            ));
        }
        Ok(())
    }

    #[test]
    fn the_sweep_reads_the_commit_not_the_working_tree() -> Result<(), String> {
        let root = example_root();
        let golden = root.join("fixtures/a/expected/example.json");
        write(&golden, r#"{"schema_version":"0.1","status":"pass"}"#);
        commit(&root)?;
        // After the commit: the tracked golden is broken in the working tree
        // and an untracked lookalike appears. Neither is candidate bytes.
        write(&golden, r#"{"schema_version":"0.1"}"#);
        write(
            &root.join("rc.json"),
            r#"{"schema_version":"0.1","status":"pass"}"#,
        );
        let tree = load_tree(&root, "HEAD");
        let _ = fs::remove_dir_all(&root);
        let tree = tree?;
        let report = sweep(
            &tree,
            &[binding(Corpus::FileName("example.json"), &[])],
            &[],
        )?;
        if !report.violations.is_empty() || report.packet["commit"] != tree.commit.as_str() {
            return Err(format!(
                "working-tree bytes reached the sweep: {:?}",
                report.violations
            ));
        }
        if tree.files.contains_key("rc.json") {
            return Err("an untracked file was read as candidate bytes".to_string());
        }
        Ok(())
    }

    #[test]
    fn the_worktree_snapshot_sees_uncommitted_edits_and_new_files() -> Result<(), String> {
        let root = example_root();
        let golden = root.join("fixtures/a/expected/example.json");
        write(&golden, r#"{"schema_version":"0.1","status":"pass"}"#);
        commit(&root)?;
        // Positive: the committed tree is clean.
        let clean = load_worktree(&root).and_then(|tree| {
            sweep(
                &tree,
                &[binding(Corpus::FileName("example.json"), &[])],
                &[],
            )
        });
        // Negative experiment (#12219): an uncommitted golden gains a member
        // the schema forbids, and a new untracked golden carries the same one.
        write(
            &golden,
            r#"{"schema_version":"0.1","status":"pass","conflicting_expressions":1}"#,
        );
        write(
            &root.join("fixtures/b/expected/example.json"),
            r#"{"schema_version":"0.1","status":"pass","extra":true}"#,
        );
        let tree = load_worktree(&root);
        let dirty = tree
            .as_ref()
            .map_err(Clone::clone)
            .and_then(|tree| sweep(tree, &[binding(Corpus::FileName("example.json"), &[])], &[]));
        let index_left_behind = root
            .join("target/ripr/schema-sweep-worktree.index")
            .exists();
        let _ = fs::remove_dir_all(&root);
        let clean = clean?;
        if !clean.violations.is_empty() {
            return Err(format!("clean worktree failed: {:?}", clean.violations));
        }
        let dirty = dirty?;
        let mentions = |needle: &str| dirty.violations.iter().any(|v| v.contains(needle));
        if !mentions("fixtures/a/expected/example.json")
            || !mentions("fixtures/b/expected/example.json")
        {
            return Err(format!(
                "uncommitted or untracked drift passed the worktree sweep: {:?}",
                dirty.violations
            ));
        }
        if index_left_behind {
            return Err("the private index was not removed".to_string());
        }
        Ok(())
    }

    #[test]
    fn worktree_flag_parses_and_refuses_commit_bound_modes() -> Result<(), String> {
        let ok = parse_args(&["--worktree".to_string()])?;
        if !ok.worktree {
            return Err("--worktree did not parse".to_string());
        }
        for args in [
            vec!["--worktree", "--rev", "HEAD"],
            vec!["--worktree", "--live-replay"],
        ] {
            let args: Vec<String> = args.into_iter().map(String::from).collect();
            if parse_args(&args).is_ok() {
                return Err(format!("{args:?} must be refused"));
            }
        }
        Ok(())
    }

    #[test]
    fn sweep_trigger_fires_for_schemas_json_and_the_sweep_source_only() {
        let hit = |p: &str| sweep_trigger(&[p.to_string()]).is_some();
        assert!(hit("schemas/ripr/check.schema.json"));
        assert!(hit("fixtures/x/expected/check.json"));
        assert!(hit("xtask/src/schema_producer_sweep.rs"));
        assert!(!hit("crates/ripr/src/lib.rs"));
        assert!(!hit("docs/README.md"));
    }

    #[test]
    fn an_artifact_no_row_claims_fails_instead_of_being_dropped() -> Result<(), String> {
        let root = example_root();
        let artifact = root.join("live.json");
        write(&artifact, r#"{"schema_version":"0.1","status":"pass"}"#);
        let report = run(
            &root,
            &[binding(Corpus::RuntimeOnly("per-run bytes"), &[])],
            &[Artifact {
                schema_path: "schemas/ripr/exmaple.schema.json".to_string(),
                schema_pointer: None,
                file: artifact,
                subject_pointer: None,
            }],
        )?;
        if !report
            .violations
            .iter()
            .any(|violation| violation.contains("no sweep row validates"))
        {
            return Err(format!(
                "a mistyped artifact schema passed: {:?}",
                report.violations
            ));
        }
        Ok(())
    }

    #[test]
    fn artifact_arguments_parse_schema_and_subject_pointers() -> Result<(), String> {
        let parsed = parse_args(&[
            "--artifact".to_string(),
            "schemas/ripr/repair-assurance.schema.json#/$defs/execution_result=out.json#/result"
                .to_string(),
        ])?;
        if parsed.rev != "HEAD" {
            return Err(format!("--rev must default to HEAD, got {}", parsed.rev));
        }
        let [artifact] = parsed.artifacts.as_slice() else {
            return Err("expected one artifact".to_string());
        };
        if artifact.schema_pointer.as_deref() != Some("/$defs/execution_result")
            || artifact.subject_pointer.as_deref() != Some("/result")
            || artifact.file != Path::new("out.json")
        {
            return Err("pointers were not split from the paths".to_string());
        }
        for malformed in [
            vec!["--artifact"],
            vec!["--check"],
            vec!["--artifact", "no-separator"],
        ] {
            let args: Vec<String> = malformed.iter().map(|arg| (*arg).to_string()).collect();
            match parse_args(&args) {
                Err(error) if error == USAGE => {}
                Err(error) => {
                    return Err(format!(
                        "{malformed:?} failed with `{error}`, not the usage text"
                    ));
                }
                Ok(_) => return Err(format!("{malformed:?} was accepted")),
            }
        }
        Ok(())
    }

    /// The binding table and the published inventory must name the same set,
    /// so a new schema cannot ship without a sweep disposition and a renamed
    /// one cannot leave a binding that silently resolves nothing. This test
    /// deliberately stops at the table: validating the whole committed corpus
    /// is the release command's job, not a new required gate during the cut
    /// (#3919).
    #[test]
    fn the_binding_table_covers_exactly_the_published_inventory() -> Result<(), String> {
        let tree = load_tree(&crate::repo_root()?, "HEAD")?;
        let inventory: Vec<&String> = tree
            .files
            .keys()
            .filter(|path| path.starts_with(SCHEMAS_PREFIX))
            .collect();
        for schema_path in &inventory {
            if !BINDINGS
                .iter()
                .any(|binding| binding.schema_path == schema_path.as_str())
            {
                return Err(format!("{schema_path} has no sweep binding"));
            }
        }
        for binding in BINDINGS {
            if !inventory
                .iter()
                .any(|schema_path| schema_path.as_str() == binding.schema_path)
            {
                return Err(format!("binding names unpublished {}", binding.schema_path));
            }
        }
        Ok(())
    }

    /// Exempt rows are only as good as the authority they name. Each named
    /// test or helper must still exist in its file and be cited by a binding's
    /// exemption reason, so deleting an authority fails here instead of
    /// leaving an exemption that points at nothing (#9561).
    #[test]
    fn exempt_rows_name_authorities_that_still_exist() -> Result<(), String> {
        // (name, is_test_fn, source). A helper must be defined and called; a
        // test fn must sit directly under `#[test]`.
        let pins: [(&str, bool, &str); 5] = [
            (
                "timeout_refusal_with_stashed_context_renders_input_context_at_generation_0_3",
                true,
                include_str!("../../crates/ripr/src/output/limited_check.rs"),
            ),
            (
                "check_json_analysis_timeout_retains_requested_echo_and_bounded_recovery",
                true,
                include_str!("../../crates/ripr/tests/cli_smoke.rs"),
            ),
            (
                "assert_manifest_matches_published_schema",
                false,
                include_str!("../../crates/ripr/tests/python_repair_attempt.rs"),
            ),
            (
                "assert_result_matches_published_schema",
                false,
                include_str!("../../crates/ripr/tests/cli_smoke.rs"),
            ),
            (
                "capability_output_matches_the_published_schema",
                true,
                include_str!("../../crates/ripr/src/lsp/agent_protocol.rs"),
            ),
        ];
        for (name, is_test, source) in pins {
            let defined = source.contains(&format!("fn {name}("));
            let live = if is_test {
                source
                    .lines()
                    .zip(source.lines().skip(1))
                    .any(|(attr, def)| {
                        attr.trim() == "#[test]"
                            && def.trim_start().starts_with(&format!("fn {name}("))
                    })
            } else {
                // Definition plus at least one call site.
                source.matches(&format!("{name}(")).count() >= 2
            };
            if !defined || !live {
                return Err(format!(
                    "named authority `{name}` no longer exists or is never run"
                ));
            }
            if !BINDINGS.iter().any(|binding| match &binding.corpus {
                Corpus::Exempt(reason) | Corpus::Authority(reason) => reason.contains(name),
                _ => false,
            }) {
                return Err(format!("no exempt binding cites authority `{name}`"));
            }
        }
        Ok(())
    }

    #[test]
    fn live_replay_flags_parse_and_ripr_requires_replay() -> Result<(), String> {
        let args = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let parsed = parse_args(&args(&[
            "--live-replay",
            "--ripr",
            "bin/ripr",
            "--rev",
            "abc",
        ]))?;
        assert!(parsed.live_replay);
        assert_eq!(parsed.ripr, Some(PathBuf::from("bin/ripr")));
        assert_eq!(parsed.rev, "abc");
        assert!(parse_args(&args(&["--ripr", "bin/ripr"])).is_err());
        assert!(parse_args(&args(&["--live-replay", "--bogus", "x"])).is_err());
        Ok(())
    }

    /// The sweep is only a control if the required gate runs it. Pin the
    /// wiring: the step exists and feeds every runtime row the gate can
    /// produce, so deleting the step or an `--artifact` fails here (#9505).
    #[test]
    fn the_required_rust_gate_runs_the_sweep_with_live_producer_artifacts() -> Result<(), String> {
        let gates = include_str!("../../.github/workflows/rust-gates.yml");
        let step = gates
            .split("- name: Schema producer sweep")
            .nth(1)
            .and_then(|rest| rest.split("\n      - name:").next())
            .ok_or("rust-gates.yml has no `Schema producer sweep` step")?;
        assert!(step.contains("cargo xtask schema-producer-sweep"));
        let guard = step
            .find("\"$EVENT_NAME\" = \"pull_request\" ] || [ \"$EVENT_NAME\" = \"merge_group\"")
            .ok_or("pr-evidence and review-comments feeds are not guarded to the events that run PR evidence")?;
        for schema in ["pr-evidence", "review-comments"] {
            let feed = step
                .find(&format!("--artifact schemas/ripr/{schema}.schema.json="))
                .ok_or_else(|| format!("sweep step does not feed {schema}"))?;
            assert!(
                feed > guard,
                "{schema} feed sits outside the pull_request/merge_group guard"
            );
        }
        for schema in ["doctor", "pr-evidence", "review-comments"] {
            assert!(
                step.contains(&format!("--artifact schemas/ripr/{schema}.schema.json=")),
                "sweep step does not feed the {schema} runtime row"
            );
        }
        Ok(())
    }
}
