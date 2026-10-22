//! Disposition rule v1 over hand-built facts: one control per rule step.

use super::capture::{CommitFacts, FACTS_KIND, GitFacts};
use super::dto::{EffectBasis, Range, SCHEMA_VERSION, Source};
use super::rule::{
    DeriveInput, OVERRIDES_KIND, Override, Overrides, PathClass, classify_path, derive,
};
use super::validate::{
    Context, PacketClaim, PacketOpenPr, PacketOpenPrs, SelectionPacket, validate,
};

fn sha(n: u8) -> String {
    format!("{n:02x}").repeat(20)
}

fn commit(n: u8, position: Option<u64>, effect: &str, paths: &[&str]) -> CommitFacts {
    let owned = effect == "present_effective";
    CommitFacts {
        commit_sha: sha(n),
        parent_shas: vec![sha(n - 1)],
        tree_sha: sha(n + 100),
        first_parent_position: position,
        subject: format!("change {n} (#{n})"),
        pr_refs: position.map_or_else(Vec::new, |_| vec![u64::from(n)]),
        issue_refs: Vec::new(),
        paths: paths.iter().map(|path| (*path).to_string()).collect(),
        inline_test_paths: Vec::new(),
        final_tree_effect: effect.to_string(),
        effect_basis: EffectBasis {
            alive_paths: u64::from(effect != "history_reachable_no_final_blob_effect"),
            absent_paths: u64::from(effect == "history_reachable_no_final_blob_effect"),
            last_toucher_paths: u64::from(owned),
            blamed_lines: 0,
        },
    }
}

fn facts(mut commits: Vec<CommitFacts>) -> GitFacts {
    // First parents follow the first-parent chain, as Git capture records them.
    let mut previous = sha(1);
    for commit in &mut commits {
        if commit.first_parent_position.is_some() {
            commit.parent_shas[0] = previous.clone();
            previous = commit.commit_sha.clone();
        }
    }
    let first_parent: Vec<String> = commits
        .iter()
        .filter(|c| c.first_parent_position.is_some())
        .map(|c| c.commit_sha.clone())
        .collect();
    let cut = first_parent.last().cloned().unwrap_or_default();
    GitFacts {
        schema_version: SCHEMA_VERSION.to_string(),
        kind: FACTS_KIND.to_string(),
        source: Source {
            repository: "example/ledger".to_string(),
            base_sha: sha(1),
            cut_sha: cut.clone(),
            cut_tree_sha: commits
                .iter()
                .find(|c| c.commit_sha == cut)
                .map_or_else(|| sha(200), |c| c.tree_sha.clone()),
            main_sha: cut,
        },
        range: Range {
            reachable: commits.iter().map(|c| c.commit_sha.clone()).collect(),
            first_parent,
        },
        merged_prs: commits
            .iter()
            .filter_map(|c| {
                c.pr_refs.last().map(|number| super::dto::MergedPr {
                    number: *number,
                    merge_sha: c.commit_sha.clone(),
                    state: "merged".to_string(),
                })
            })
            .collect(),
        alive_paths_at_cut: 0,
        blamed_paths: 0,
        commits,
    }
}

fn packet() -> SelectionPacket {
    SelectionPacket {
        digest: "sha256:packet".to_string(),
        claims: vec![PacketClaim {
            claim_id: "deferred".to_string(),
            owner_issue: 4427,
            resolution: "accepted_defer_with_non_claim".to_string(),
        }],
        open_prs: PacketOpenPrs {
            captured_at_main: None,
            rows: vec![PacketOpenPr {
                pr: 90,
                head: sha(90),
                disposition: "superseded".to_string(),
            }],
        },
    }
}

fn disposition_of(commit: CommitFacts) -> Result<String, String> {
    let ledger = derive(&DeriveInput {
        facts: &facts(vec![commit]),
        packet: &packet(),
        overrides: None,
        stage: "dry_run",
        audit: None,
    })?;
    ledger
        .records
        .first()
        .map(|record| record.release_disposition.clone())
        .ok_or_else(|| "no record".to_string())
}

fn require(condition: bool, message: &str) -> Result<(), String> {
    condition.then_some(()).ok_or_else(|| message.to_string())
}

#[test]
fn path_classes_follow_the_published_table() -> Result<(), String> {
    for (path, class) in [
        ("crates/ripr/src/lib.rs", PathClass::Product),
        ("editors/vscode/package.json", PathClass::Product),
        ("schemas/ripr/check.schema.json", PathClass::Product),
        ("crates/ripr/src/analysis/tests.rs", PathClass::Evidence),
        ("crates/ripr/tests/cli_smoke.rs", PathClass::Evidence),
        ("fixtures/boundary_gap/SPEC.md", PathClass::Evidence),
        ("docs/specs/RIPR-SPEC-0001-x.md", PathClass::Evidence),
        (".ripr/traceability.toml", PathClass::Evidence),
        ("changelog.d/1-x.md", PathClass::Docs),
        ("editors/vscode/README.md", PathClass::Docs),
        ("editors/vscode/package-lock.json", PathClass::Control),
        ("editors/vscode/test/runTest.ts", PathClass::Evidence),
        (
            "editors/vscode/test-fixtures/tls/a.pem",
            PathClass::Evidence,
        ),
        ("CHANGELOG.md", PathClass::Docs),
        ("docs/ROADMAP.md", PathClass::Docs),
        ("xtask/src/main.rs", PathClass::Control),
        (".github/workflows/x.yml", PathClass::Control),
        ("policy/x.toml", PathClass::Control),
        ("Cargo.lock", PathClass::Control),
        ("crates/ripr/Cargo.toml", PathClass::Control),
        ("unknown/dir/file.bin", PathClass::Other),
    ] {
        require(
            classify_path(path) == class,
            &format!("{path} is not {class:?}"),
        )?;
    }
    let mixed = [
        "docs/ROADMAP.md".to_string(),
        "xtask/src/main.rs".to_string(),
    ];
    require(
        mixed.iter().map(|path| classify_path(path)).max() == Some(PathClass::Control),
        "the strongest class wins",
    )
}

#[test]
fn rule_steps_apply_in_order() -> Result<(), String> {
    let effective = "present_effective";
    require(
        disposition_of(commit(5, Some(1), effective, &["crates/ripr/src/a.rs"]))?
            == "included_accepted_product",
        "product path",
    )?;
    require(
        disposition_of(commit(
            5,
            Some(1),
            effective,
            &["fixtures/x/SPEC.md", "docs/a.md"],
        ))? == "included_test_or_evidence",
        "evidence beats docs",
    )?;
    require(
        disposition_of(commit(5, Some(1), effective, &["docs/a.md"]))?
            == "included_structural_no_semantic_delta",
        "docs only",
    )?;
    require(
        disposition_of(commit(
            5,
            Some(1),
            effective,
            &["xtask/src/a.rs", "docs/a.md"],
        ))? == "included_accepted_release_or_control",
        "control beats evidence and docs",
    )?;
    require(
        disposition_of(commit(
            5,
            Some(1),
            "present_but_superseded",
            &["crates/ripr/src/a.rs"],
        ))? == "included_superseded_by_later_fix",
        "superseded beats the path class",
    )?;
    require(
        disposition_of(commit(
            5,
            Some(1),
            "history_reachable_no_final_blob_effect",
            &["crates/ripr/src/a.rs"],
        ))? == "included_historical_reachable_only",
        "no final blob effect",
    )?;
    require(
        disposition_of(commit(5, None, effective, &["crates/ripr/src/a.rs"]))?
            == "included_historical_reachable_only",
        "reached through a merge",
    )
}

#[test]
fn unjudgeable_commits_block_instead_of_defaulting() -> Result<(), String> {
    require(
        disposition_of(commit(
            5,
            Some(1),
            "present_effective",
            &["unknown/dir/file.bin"],
        ))? == "blocking_unresolved",
        "unclassified path",
    )?;
    require(
        disposition_of(commit(
            5,
            Some(1),
            "history_reachable_no_final_blob_effect",
            &[],
        ))? == "blocking_unresolved",
        "empty diff",
    )?;
    for effect in [
        "present_effective",
        "present_but_superseded",
        "history_reachable_no_final_blob_effect",
    ] {
        require(
            disposition_of(commit(
                5,
                Some(1),
                effect,
                &["crates/ripr/src/a.rs", "unknown/dir/state.json"],
            ))? == "blocking_unresolved",
            &format!("unclassified path beside a product path ({effect})"),
        )?;
    }
    require(
        disposition_of(commit(
            5,
            None,
            "present_effective",
            &["unknown/dir/state.json"],
        ))? == "included_historical_reachable_only",
        "rule 0 covers first-parent commits only",
    )?;
    let mut merge = commit(5, Some(1), "present_effective", &["crates/ripr/src/a.rs"]);
    merge.parent_shas.push(sha(77));
    require(
        disposition_of(merge)? == "blocking_unresolved",
        "two-parent first-parent merge",
    )
}

#[test]
fn deferred_claim_owner_issue_in_subject_gives_a_nonclaim_row() -> Result<(), String> {
    let mut mention = commit(5, Some(1), "present_effective", &["crates/ripr/src/a.rs"]);
    mention.issue_refs = vec![4427];
    let ledger = derive(&DeriveInput {
        facts: &facts(vec![mention]),
        packet: &packet(),
        overrides: None,
        stage: "dry_run",
        audit: None,
    })?;
    let record = ledger.records.first().ok_or("no record")?;
    require(
        record.release_disposition == "included_with_release_nonclaim"
            && record.claim_ref.as_deref() == Some("deferred"),
        "the claim ref is recorded",
    )
}

#[test]
fn derived_ledger_validates_and_overrides_resolve_blocking_rows() -> Result<(), String> {
    // The merge reaches commit 7 through its second parent, so the range is connected.
    let mut merge = commit(6, Some(2), "present_effective", &["crates/ripr/src/a.rs"]);
    merge.parent_shas.push(sha(7));
    let mut side = commit(7, None, "present_but_superseded", &["docs/a.md"]);
    side.parent_shas = vec![sha(5)];
    let input = facts(vec![
        commit(5, Some(1), "present_effective", &["xtask/src/a.rs"]),
        merge,
        side,
    ]);
    let blocked = derive(&DeriveInput {
        facts: &input,
        packet: &packet(),
        overrides: None,
        stage: "dry_run",
        audit: None,
    })?;
    let found = validate(&blocked, &Context::default());
    require(
        found.is_empty(),
        &format!("a dry run tolerates blocking rows: {found:?}"),
    )?;
    let overrides = Overrides {
        schema_version: "1".to_string(),
        kind: OVERRIDES_KIND.to_string(),
        overrides: vec![Override {
            commit_sha: sha(6),
            disposition: "included_accepted_product".to_string(),
            owner: "reviewer".to_string(),
            note: "merge of a product change".to_string(),
        }],
    };
    let resolved = derive(&DeriveInput {
        facts: &input,
        packet: &packet(),
        overrides: Some(&overrides),
        stage: "final",
        audit: Some(super::dto::AuditRef {
            path: "audit.md".to_string(),
            sha256: "0".repeat(64),
        }),
    })?;
    let record = resolved.records.get(1).ok_or("no record")?;
    require(
        record.release_disposition == "included_accepted_product"
            && record.override_ref.as_deref() == Some("override:060606060606")
            && record.acceptance_owner == "reviewer",
        "override recorded on the row",
    )?;
    let found = validate(&resolved, &Context::default());
    require(
        found.iter().any(|reason| reason.contains("--packet")),
        "a final ledger needs the packet",
    )?;
    let mut final_packet = packet();
    final_packet.open_prs.captured_at_main = Some(sha(6));
    let context = Context {
        packet: Some(final_packet),
        ..Context::default()
    };
    let found = validate(&resolved, &context);
    require(
        found.is_empty(),
        &format!("resolved final ledger: {found:?}"),
    )
}

#[test]
fn bad_overrides_are_rejected() -> Result<(), String> {
    let input = facts(vec![commit(
        5,
        Some(1),
        "present_effective",
        &["docs/a.md"],
    )]);
    let entry = |commit_sha: String, disposition: &str| Override {
        commit_sha,
        disposition: disposition.to_string(),
        owner: "reviewer".to_string(),
        note: "reason".to_string(),
    };
    for (overrides, needle) in [
        (
            vec![entry(sha(99), "included_test_or_evidence")],
            "outside the range",
        ),
        (vec![entry(sha(5), "blocking_unresolved")], "non-blocking"),
        (vec![entry(sha(5), "looks_fine")], "non-blocking"),
        (
            vec![entry(sha(5), "included_accepted_product")],
            "only blocking_unresolved",
        ),
        (
            vec![
                entry(sha(5), "included_test_or_evidence"),
                entry(sha(5), "included_test_or_evidence"),
            ],
            "duplicated",
        ),
    ] {
        let set = Overrides {
            schema_version: "1".to_string(),
            kind: OVERRIDES_KIND.to_string(),
            overrides,
        };
        let result = derive(&DeriveInput {
            facts: &input,
            packet: &packet(),
            overrides: Some(&set),
            stage: "dry_run",
            audit: None,
        });
        require(
            result.err().is_some_and(|error| error.contains(needle)),
            &format!("override error `{needle}` missing"),
        )?;
    }
    Ok(())
}

#[test]
fn short_commit_ids_do_not_panic_the_override_reference() -> Result<(), String> {
    let mut short = commit(5, Some(1), "present_effective", &[]);
    short.commit_sha = "abc".to_string();
    let input = facts(vec![short]);
    let set = Overrides {
        schema_version: "1".to_string(),
        kind: OVERRIDES_KIND.to_string(),
        overrides: vec![Override {
            commit_sha: "abc".to_string(),
            disposition: "included_historical_reachable_only".to_string(),
            owner: "reviewer".to_string(),
            note: "reason".to_string(),
        }],
    };
    let ledger = derive(&DeriveInput {
        facts: &input,
        packet: &packet(),
        overrides: Some(&set),
        stage: "dry_run",
        audit: None,
    })?;
    let record = ledger.records.first().ok_or("no record")?;
    require(
        record.override_ref.as_deref() == Some("override:abc"),
        "short id reference",
    )
}

fn derive_args(facts: &str, packet: &str, output: &str) -> Vec<String> {
    [
        "derive", "--facts", facts, "--packet", packet, "--output", output,
    ]
    .iter()
    .map(|arg| (*arg).to_string())
    .collect()
}

#[test]
fn repeated_derive_flags_are_rejected_before_any_file_is_read() -> Result<(), String> {
    let mut args = derive_args("a.json", "b.json", "c.json");
    args.extend(["--facts".to_string(), "other.json".to_string()]);
    let error = super::super::release_ledger(&args)
        .err()
        .ok_or("accepted a repeated flag")?;
    require(
        error.contains("--facts") && error.contains("more than once"),
        &format!("the repeated flag is named: {error}"),
    )
}

#[test]
fn failed_derivation_leaves_an_existing_output_untouched() -> Result<(), String> {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("ripr-derive-keep-{nonce}"));
    std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    // The fixture range does not start at the pinned 0.11 base, so validation fails.
    let input = facts(vec![commit(
        5,
        Some(1),
        "present_effective",
        &["docs/a.md"],
    )]);
    let facts_path = dir.join("facts.json");
    let packet_path = dir.join("packet.json");
    let output_path = dir.join("ledger.json");
    std::fs::write(
        &facts_path,
        serde_json::to_string(&input).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    std::fs::write(
        &packet_path,
        r#"{"claims":[],"open_prs":{"captured_at_main":null,"rows":[]}}"#,
    )
    .map_err(|error| error.to_string())?;
    std::fs::write(&output_path, "previous ledger").map_err(|error| error.to_string())?;
    let path = |p: &std::path::Path| p.to_string_lossy().into_owned();
    let error = super::super::release_ledger(&derive_args(
        &path(&facts_path),
        &path(&packet_path),
        &path(&output_path),
    ))
    .err()
    .ok_or("derivation of an unpinned range succeeded")?;
    require(
        error.contains("derived ledger is invalid"),
        &format!("validation failure expected: {error}"),
    )?;
    let kept = std::fs::read_to_string(&output_path).map_err(|error| error.to_string())?;
    std::fs::remove_dir_all(&dir).map_err(|error| error.to_string())?;
    require(
        kept == "previous ledger",
        "the existing output was replaced",
    )
}

#[test]
fn inline_test_only_product_paths_count_as_evidence() -> Result<(), String> {
    let path = "crates/ripr/src/lib.rs";
    let mut inline = commit(5, Some(1), "present_effective", &[path]);
    require(
        disposition_of(inline.clone())? == "included_accepted_product",
        "no inline-test fact keeps the path product",
    )?;
    inline.inline_test_paths = vec![path.to_string()];
    require(
        disposition_of(inline.clone())? == "included_test_or_evidence",
        "an inline-test-only path is evidence",
    )?;
    // The fact covers only the path it names.
    let mut mixed = commit(
        5,
        Some(1),
        "present_effective",
        &[path, "crates/ripr/src/other.rs"],
    );
    mixed.inline_test_paths = vec![path.to_string()];
    require(
        disposition_of(mixed)? == "included_accepted_product",
        "another product path keeps the commit product",
    )
}
