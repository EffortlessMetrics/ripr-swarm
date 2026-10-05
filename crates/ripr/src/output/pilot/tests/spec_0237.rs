//! RIPR-SPEC-0237 acceptance examples for pilot seam ranking.
//!
//! Each test names its example number. Seams are written in the spec's
//! notation, `class file owner line [flags]`, and the expected output is the
//! spec's `file:line` order. Examples 1–3, 10–12, 19 and 20 are covered by
//! the tests in the parent module.

use super::*;
use crate::analysis::new_test_target::{
    NewTestKind, NewTestProposalProvenance, NewTestTargetAdmission, NewTestTargetProposal,
};
use crate::analysis::{SeamLimitSource, apply_pilot_seam_budget_inner};
use crate::output::agent_seam_packets::suggested_assertion_for_classified_seam;

/// A missing-discriminator fact the boundary seam's required discriminator
/// accepts, so repair-route readiness, and with it the suggested assertion,
/// can hold.
fn exact_missing() -> MissingDiscriminatorFact {
    MissingDiscriminatorFact {
        value: "discount_threshold (equality boundary)".to_string(),
        reason: "observed values do not include the equality-boundary case".to_string(),
        flow_sink: None,
    }
}

fn proposed_integration_target(file: &str, owner: &str) -> NewTestTargetAdmission {
    NewTestTargetAdmission {
        proposal: Some(NewTestTargetProposal {
            kind: NewTestKind::Integration,
            file: PathBuf::from(file),
            owner: owner.to_string(),
            provenance: NewTestProposalProvenance::ProducerOwned,
        }),
        region: None,
        blocker: None,
    }
}

fn class_code(code: &str) -> Result<SeamGripClass, String> {
    Ok(match code {
        "W" => SeamGripClass::WeaklyGripped,
        "U" => SeamGripClass::Ungripped,
        "R" => SeamGripClass::ReachableUnrevealed,
        "AU" => SeamGripClass::ActivationUnknown,
        "PU" => SeamGripClass::PropagationUnknown,
        "OU" => SeamGripClass::ObservationUnknown,
        "DU" => SeamGripClass::DiscriminationUnknown,
        "O" => SeamGripClass::Opaque,
        "SG" => SeamGripClass::StronglyGripped,
        "I" => SeamGripClass::Intentional,
        "X" => SeamGripClass::Suppressed,
        other => return Err(format!("unknown class code `{other}`")),
    })
}

/// One seam from the spec notation, for example `W src/a.rs f 10 m,r`.
///
/// `m` adds a missing discriminator, `r` a related test, and `a` the
/// evidence a suggested assertion needs: an exact missing discriminator and
/// a safe test target (the related test when `r` is set, otherwise a
/// producer-owned integration proposal). The builder refuses any seam whose
/// derived evidence disagrees with its flags, so a test cannot pass on a
/// fixture that never held the flag it names.
fn spec_seam(notation: &str) -> Result<ClassifiedSeam, String> {
    let parts = notation.split_whitespace().collect::<Vec<_>>();
    let [class, file, owner, line, rest @ ..] = parts.as_slice() else {
        return Err(format!(
            "`{notation}` is not `class file owner line [flags]`"
        ));
    };
    let flags = match rest {
        [] => "",
        [flags] => flags,
        _ => return Err(format!("`{notation}` has more than one flag group")),
    };
    if let Some(flag) = flags
        .split(',')
        .find(|flag| !matches!(*flag, "" | "m" | "r" | "a"))
    {
        return Err(format!("`{notation}` has unknown flag `{flag}`"));
    }
    let has = |flag: &str| flags.split(',').any(|candidate| candidate == flag);
    let line = line
        .parse::<usize>()
        .map_err(|err| format!("`{notation}` line: {err}"))?;
    let mut entry = classified_in_owner(class_code(class)?, file, owner, line);
    if has("a") {
        entry.evidence.missing_discriminators.push(exact_missing());
        if !has("r") {
            entry.evidence.new_test_target =
                Some(proposed_integration_target("tests/spec_0237.rs", owner));
        }
    } else if has("m") {
        entry.evidence.missing_discriminators.push(missing());
    }
    if has("r") {
        entry.evidence.related_tests.push(related_test());
    }

    let held = [
        ("m", !entry.evidence.missing_discriminators.is_empty()),
        ("r", !entry.evidence.related_tests.is_empty()),
        (
            "a",
            suggested_assertion_for_classified_seam(&entry).is_some(),
        ),
    ];
    for (flag, present) in held {
        let wanted = has(flag) || (flag == "m" && has("a"));
        if present != wanted {
            return Err(format!(
                "`{notation}`: flag `{flag}` should be {wanted} but the evidence gives {present}"
            ));
        }
    }
    Ok(entry)
}

fn spec_seams(notations: &[&str]) -> Result<Vec<ClassifiedSeam>, String> {
    notations
        .iter()
        .map(|notation| spec_seam(notation))
        .collect()
}

fn places(ranked: &[&ClassifiedSeam]) -> Vec<String> {
    ranked
        .iter()
        .map(|entry| {
            format!(
                "{}:{}",
                display_path(entry.seam.file()),
                entry.seam.display_line()
            )
        })
        .collect()
}

fn ranked(notations: &[&str], max_seams: usize) -> Result<Vec<String>, String> {
    let entries = spec_seams(notations)?;
    Ok(places(&top_actionable_seams(&entries, max_seams)))
}

fn summary_md(entries: &[ClassifiedSeam], max_seams: usize) -> String {
    let artifacts = pilot_artifacts();
    let mut context = pilot_context(&artifacts);
    context.max_seams = max_seams;
    render_pilot_summary_md(entries, context)
}

/// The Markdown ranked list from its first entry on, so a note can be placed
/// between two numbered entries.
fn ranked_section(md: &str) -> &str {
    md.find("## Ranked Seams").map_or("", |start| &md[start..])
}

const ONE_MORE: &str = "   - Also in this function: 1 more actionable seam not listed here\n";

const EXAMPLE_13: [&str; 4] = [
    "W src/a.rs f 1",
    "U src/a.rs f 2",
    "U src/b.rs g 1",
    "U src/b.rs g 2",
];

#[test]
fn spec_0237_example_04_full_class_order() -> Result<(), String> {
    let input = [
        "O src/a.rs f 1",
        "PU src/b.rs g 1",
        "AU src/c.rs h 1",
        "R src/d.rs k 1",
        "U src/e.rs p 1",
        "W src/f.rs q 1",
    ];
    // `PU` sorts before `AU` by path: the unknown classes share one rank.
    assert_eq!(
        ranked(&input, 6)?,
        [
            "src/f.rs:1",
            "src/e.rs:1",
            "src/d.rs:1",
            "src/b.rs:1",
            "src/c.rs:1",
            "src/a.rs:1"
        ]
    );
    Ok(())
}

#[test]
fn spec_0237_example_05_unknown_tie_by_evidence() -> Result<(), String> {
    let input = ["AU src/c.rs f 1 m", "PU src/b.rs g 1"];
    assert_eq!(ranked(&input, 2)?, ["src/c.rs:1", "src/b.rs:1"]);
    Ok(())
}

#[test]
fn spec_0237_example_06_evidence_is_a_boolean() -> Result<(), String> {
    let mut three = spec_seam("W src/b.rs f 5 m")?;
    three.evidence.missing_discriminators = vec![missing(), missing(), missing()];
    let one = spec_seam("W src/a.rs g 10 m")?;
    let entries = [three, one];
    assert_eq!(
        places(&top_actionable_seams(&entries, 2)),
        ["src/a.rs:10", "src/b.rs:5"]
    );
    Ok(())
}

/// The spec's example 7 lists `[a]` and `[r,a]` without `m`. A suggested
/// assertion on a `predicate_boundary` seam needs an exact missing
/// discriminator, so `a` implies `m` in production evidence and those two
/// seams cannot be built. This is the nearest buildable input: it still
/// orders `m` over `r` over `a`, each as a later tie-breaker.
#[test]
fn spec_0237_example_07_evidence_precedence() -> Result<(), String> {
    let input = [
        "W src/a.rs f 1 a",
        "W src/b.rs g 1 m,r",
        "W src/c.rs h 1 m",
        "W src/d.rs k 1 r,a",
        "W src/e.rs p 1 r",
        "W src/f.rs q 1",
    ];
    assert_eq!(
        ranked(&input, 6)?,
        [
            "src/d.rs:1",
            "src/b.rs:1",
            "src/a.rs:1",
            "src/c.rs:1",
            "src/e.rs:1",
            "src/f.rs:1"
        ]
    );
    Ok(())
}

#[test]
fn spec_0237_example_07_a_without_m_is_unbuildable() {
    // Keeps the doc comment above honest: dropping the exact discriminator
    // from an `a` seam removes the suggested assertion.
    let mut entry = classified_in_owner(SeamGripClass::WeaklyGripped, "src/a.rs", "f", 1);
    entry.evidence.related_tests.push(related_test());
    assert!(suggested_assertion_for_classified_seam(&entry).is_none());
}

#[test]
fn spec_0237_example_08_path_and_line_order() -> Result<(), String> {
    let input = [
        "W src/z.rs f 1",
        "W src/ab.rs g 1",
        "W src/a/b.rs h 1",
        "W src/a.rs p 10",
        "W src/a.rs q 9",
    ];
    assert_eq!(
        ranked(&input, 5)?,
        [
            "src/a.rs:9",
            "src/a.rs:10",
            "src/a/b.rs:1",
            "src/ab.rs:1",
            "src/z.rs:1"
        ]
    );
    Ok(())
}

#[test]
fn spec_0237_example_09_kind_tie_break() -> Result<(), String> {
    let mut return_value = spec_seam("W src/a.rs f 5")?;
    return_value.seam = RepoSeam::new(
        "src/a.rs",
        "f",
        SeamKind::ReturnValue,
        50,
        5,
        "amount >= discount_threshold",
        RequiredDiscriminator::ReturnValue {
            description: "amount >= discount_threshold".to_string(),
        },
        ExpectedSink::ReturnValue,
    );
    return_value.evidence.seam_id = return_value.seam.id().clone();
    let boundary = spec_seam("W src/a.rs f 5")?;
    let entries = [return_value, boundary];

    let top = top_actionable_seams(&entries, 1);
    assert_eq!(
        top.iter()
            .map(|entry| entry.seam.kind())
            .collect::<Vec<_>>(),
        [SeamKind::PredicateBoundary]
    );
    Ok(())
}

#[test]
fn spec_0237_example_13_rounds_cross_classes() -> Result<(), String> {
    // Per-class rounds would give a:1, a:2, b:1, b:2: `f` would get a fresh
    // first pick among the ungripped seams.
    assert_eq!(
        ranked(&EXAMPLE_13, 4)?,
        ["src/a.rs:1", "src/b.rs:1", "src/a.rs:2", "src/b.rs:2"]
    );
    assert_eq!(ranked(&EXAMPLE_13, 2)?, ["src/a.rs:1", "src/b.rs:1"]);
    Ok(())
}

#[test]
fn spec_0237_example_14_round_beats_evidence() -> Result<(), String> {
    let input = ["W src/a.rs f 1 m", "W src/a.rs f 2 m", "W src/b.rs g 1"];
    assert_eq!(
        ranked(&input, 3)?,
        ["src/a.rs:1", "src/b.rs:1", "src/a.rs:2"]
    );
    Ok(())
}

#[test]
fn spec_0237_example_15_unknown_classes_share_a_round_space() -> Result<(), String> {
    let input = [
        "AU src/a.rs f 1 m",
        "PU src/a.rs f 2 m",
        "DU src/b.rs g 3 m",
    ];
    assert_eq!(
        ranked(&input, 3)?,
        ["src/a.rs:1", "src/b.rs:3", "src/a.rs:2"]
    );
    Ok(())
}

#[test]
fn spec_0237_example_16_distinct_owner_strings_in_one_file() -> Result<(), String> {
    let input = [
        "W src/lib.rs A::fmt 1",
        "W src/lib.rs A::fmt 2",
        "W src/lib.rs B::fmt 10",
    ];
    assert_eq!(
        ranked(&input, 3)?,
        ["src/lib.rs:1", "src/lib.rs:10", "src/lib.rs:2"]
    );
    Ok(())
}

#[test]
fn spec_0237_example_17_top_pick_never_moves() -> Result<(), String> {
    let example_14 = ["W src/a.rs f 1 m", "W src/a.rs f 2 m", "W src/b.rs g 1"];
    let example_12 = [
        "W src/a.rs fmt 1",
        "W src/a.rs fmt 2",
        "U src/b.rs parse 1",
        "W src/c.rs fmt 1",
    ];
    assert_eq!(ranked(&example_14, 1)?, ["src/a.rs:1"]);
    assert_eq!(ranked(&example_12, 1)?, ["src/a.rs:1"]);

    // The terminal names that seam whatever `--max-seams` is, and no other.
    let entries = spec_seams(&example_14)?;
    let artifacts = pilot_artifacts();
    for max_seams in [1, 3, 10] {
        let mut context = pilot_context(&artifacts);
        context.max_seams = max_seams;
        let terminal = render_pilot_terminal(&entries, context);
        assert!(terminal.contains("src/a.rs:1"), "{terminal}");
        assert!(!terminal.contains("src/a.rs:2"), "{terminal}");
        assert!(!terminal.contains("src/b.rs:1"), "{terminal}");
    }
    Ok(())
}

#[test]
fn spec_0237_example_18_fewer_ranked_than_n() -> Result<(), String> {
    let input = ["W src/a.rs f 1", "SG src/a.rs f 2", "U src/b.rs g 1"];
    assert_eq!(ranked(&input, 10)?, ["src/a.rs:1", "src/b.rs:1"]);
    Ok(())
}

/// The `file:line` of each numbered entry in the Markdown ranked list, in
/// order. An entry heading reads ``N. `id` label (`class`) file:line `kind` ``.
fn md_places(md: &str) -> Vec<String> {
    ranked_section(md)
        .lines()
        .filter(|line| {
            line.split_once(". `")
                .is_some_and(|(n, _)| n.parse::<usize>().is_ok())
        })
        .filter_map(|line| line.split(' ').find(|word| word.starts_with("src/")))
        .map(str::to_string)
        .collect()
}

/// Asserts the ranked Markdown lists `listed` in order and carries `note`
/// exactly once, inside the first entry.
fn assert_note_under_first(md: &str, listed: &[&str], note: &str) {
    assert_eq!(md_places(md), listed, "{md}");
    assert_eq!(md.matches(note).count(), 1, "{md}");
    assert_eq!(md.matches("Also in this function").count(), 1, "{md}");
    let section = ranked_section(md);
    let at = |needle: &str| section.find(needle).unwrap_or(usize::MAX);
    let (first, note_at, second) = (at("\n1. `"), at(note), at("\n2. `"));
    assert!(second != usize::MAX, "{md}");
    assert!(first < note_at && note_at < second, "{md}");
}

#[test]
fn spec_0237_example_21_opaque_is_counted() -> Result<(), String> {
    let entries = spec_seams(&[
        "W src/a.rs f 1",
        "AU src/a.rs f 2",
        "O src/a.rs f 3",
        "W src/b.rs g 1",
    ])?;
    assert_eq!(
        places(&top_actionable_seams(&entries, 3)),
        ["src/a.rs:1", "src/b.rs:1", "src/a.rs:2"]
    );
    let md = summary_md(&entries, 3);
    assert_note_under_first(&md, &["src/a.rs:1", "src/b.rs:1", "src/a.rs:2"], ONE_MORE);
    Ok(())
}

#[test]
fn spec_0237_example_22_opaque_counted_when_unlisted() -> Result<(), String> {
    let entries = spec_seams(&["W src/a.rs f 1", "O src/a.rs f 9", "W src/b.rs g 1"])?;
    let md = summary_md(&entries, 2);
    assert_note_under_first(&md, &["src/a.rs:1", "src/b.rs:1"], ONE_MORE);
    Ok(())
}

#[test]
fn spec_0237_example_23_unknown_classes_are_counted_singular() -> Result<(), String> {
    let entries = spec_seams(&["W src/a.rs f 1", "PU src/a.rs f 5", "W src/b.rs g 1"])?;
    let md = summary_md(&entries, 2);
    assert_note_under_first(&md, &["src/a.rs:1", "src/b.rs:1"], ONE_MORE);
    assert!(!md.contains("more actionable seams not listed"), "{md}");
    Ok(())
}

#[test]
fn spec_0237_example_24_budget_bounds_the_count() -> Result<(), String> {
    // The spec's analysis order. Production inventory sorts by file and line
    // before the budget cut, so this order is artificial; the test pins only
    // that the renderer counts the analyzed slice, not seams cut before it.
    let mut entries = spec_seams(&[
        "W src/a.rs f 1",
        "W src/b.rs g 1",
        "W src/a.rs f 2",
        "W src/a.rs f 3",
    ])?;
    let cut = apply_pilot_seam_budget_inner(&mut entries, 2, SeamLimitSource::Configured);
    assert_eq!(cut.map(|info| (info.analyzed, info.total)), Some((2, 4)));

    assert_eq!(
        places(&top_actionable_seams(&entries, 5)),
        ["src/a.rs:1", "src/b.rs:1"]
    );
    let md = summary_md(&entries, 5);
    assert!(!md.contains("Also in this function"), "{md}");
    Ok(())
}

#[test]
fn spec_0237_example_25_json_matches_markdown() -> Result<(), String> {
    let entries = spec_seams(&[
        "W src/a.rs a::clone 10",
        "W src/a.rs a::clone 11",
        "W src/a.rs a::clone 12",
        "W src/a.rs a::clone 13",
        "W src/b.rs b::parse 5",
    ])?;
    let artifacts = pilot_artifacts();
    let mut context = pilot_context(&artifacts);
    context.max_seams = 3;
    let json = render_pilot_summary_json(&entries, context);
    let value = serde_json::from_str::<serde_json::Value>(&json).map_err(|err| err.to_string())?;
    let top = value
        .get("top_actionable_seams")
        .and_then(serde_json::Value::as_array)
        .ok_or("no top_actionable_seams array")?;

    let json_places = top
        .iter()
        .map(|seam| {
            let file = seam.get("file").and_then(serde_json::Value::as_str);
            let line = seam.get("line").and_then(serde_json::Value::as_u64);
            format!("{}:{}", file.unwrap_or("?"), line.unwrap_or(0))
        })
        .collect::<Vec<_>>();
    assert_eq!(json_places, ["src/a.rs:10", "src/b.rs:5", "src/a.rs:11"]);
    assert_note_under_first(
        &summary_md(&entries, 3),
        &["src/a.rs:10", "src/b.rs:5", "src/a.rs:11"],
        "   - Also in this function: 2 more actionable seams not listed here\n",
    );

    // Decision 8: the owner count is Markdown only. Every object carries the
    // field set a lone seam with no owner siblings gets, so no count field
    // appears for an owner with unlisted seams.
    let lone = spec_seams(&["W src/c.rs c::lone 1"])?;
    let lone_json = render_pilot_summary_json(&lone, pilot_context(&artifacts));
    let lone_value =
        serde_json::from_str::<serde_json::Value>(&lone_json).map_err(|err| err.to_string())?;
    let keys = |seam: &serde_json::Value| {
        seam.as_object()
            .map(|object| object.keys().cloned().collect::<Vec<_>>())
    };
    let lone_keys = lone_value
        .get("top_actionable_seams")
        .and_then(|top| top.get(0))
        .and_then(keys)
        .ok_or("no lone top seam")?;
    // Pin the field set itself, so a count field added to every seam (the
    // lone one included) still fails here.
    let mut sorted = lone_keys.clone();
    sorted.sort();
    assert_eq!(
        sorted,
        [
            "file",
            "grip_class",
            "kind",
            "line",
            "missing_discriminator",
            "owner",
            "related_test_present",
            "seam_id",
            "suggested_assertion_present",
            "targeted_test_brief",
            "why",
        ],
        "{lone_json}"
    );
    for seam in top {
        assert_eq!(keys(seam).as_ref(), Some(&lone_keys), "{json}");
    }
    Ok(())
}

/// Every ordering of `items`, by Heap's algorithm.
fn permutations<T: Clone>(items: &[T]) -> Vec<Vec<T>> {
    fn heap<T: Clone>(k: usize, items: &mut [T], out: &mut Vec<Vec<T>>) {
        if k <= 1 {
            out.push(items.to_vec());
            return;
        }
        for i in 0..k {
            heap(k - 1, items, out);
            let swap = if k.is_multiple_of(2) { i } else { 0 };
            if i + 1 < k {
                items.swap(swap, k - 1);
            }
        }
    }
    let mut items = items.to_vec();
    let mut out = Vec::new();
    heap(items.len(), &mut items, &mut out);
    out
}

#[test]
fn spec_0237_example_26_input_order_does_not_matter() -> Result<(), String> {
    let orders = permutations(&EXAMPLE_13);
    assert_eq!(orders.len(), 24);
    let distinct = orders
        .iter()
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    assert_eq!(distinct, 24);
    for order in &orders {
        assert_eq!(
            ranked(order, 4)?,
            ["src/a.rs:1", "src/b.rs:1", "src/a.rs:2", "src/b.rs:2"],
            "input order {order:?}"
        );
    }
    Ok(())
}
