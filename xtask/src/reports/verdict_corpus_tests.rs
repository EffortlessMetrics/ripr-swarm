use super::*;
use serde_json::json;

fn finding(class: &str, line: u64, currentness: &str) -> Value {
    json!({
        "classification": class,
        "source_currentness": currentness,
        "probe": {"file": "./src/lib.rs", "line": line},
        "related_tests_total": 1,
        "related_tests": [{"name": "t"}],
        "ripr": {"reach": {"state": "yes"}, "discriminate": {"state": "no"}},
    })
}

fn anchor() -> Anchor {
    Anchor {
        file: "src/lib.rs".to_string(),
        line: 10,
    }
}

fn repo_corpus_dir() -> PathBuf {
    crate::dogfood::repo_rooted_fixture_path(CORPUS_DIR)
}

#[test]
fn score_follows_the_truth_table_in_both_error_directions() {
    use Outcome::*;
    use TruthState::*;
    use Verdict::*;
    let table = [
        (Discriminated, Credited, Ideal),
        (Discriminated, Limited, Abstained),
        (Discriminated, Silent, Abstained),
        (Discriminated, Gap, FalseActionable),
        (NotDiscriminated, Gap, Ideal),
        (NotDiscriminated, Limited, Abstained),
        (NotDiscriminated, Credited, FalseExposed),
        (NotDiscriminated, Silent, FalseSilent),
        (PartiallyDiscriminated, Gap, Ideal),
        (PartiallyDiscriminated, Limited, Abstained),
        (PartiallyDiscriminated, Credited, FalseExposed),
        (PartiallyDiscriminated, Silent, FalseSilent),
    ];
    for (truth, observed, expected) in table {
        assert_eq!(score(truth, observed), expected, "{truth:?} x {observed:?}");
    }
}

#[test]
fn finding_verdict_mirrors_triage_classes_and_named_limits() {
    assert_eq!(
        finding_verdict(&finding("exposed", 10, "candidate_current")),
        Verdict::Credited
    );
    assert_eq!(
        finding_verdict(&finding("weakly_exposed", 10, "candidate_current")),
        Verdict::Gap
    );
    assert_eq!(
        finding_verdict(&finding("reachable_unrevealed", 10, "candidate_current")),
        Verdict::Gap
    );
    for class in [
        "no_static_path",
        "infection_unknown",
        "propagation_unknown",
        "static_unknown",
    ] {
        assert_eq!(
            finding_verdict(&finding(class, 10, "candidate_current")),
            Verdict::Limited
        );
    }
    // A producer-named limitation keeps a reachable_unrevealed class limited.
    let mut limited = finding("reachable_unrevealed", 10, "candidate_current");
    limited["static_limit_kind"] = json!("rust_integration_public_api_path_unresolved");
    assert_eq!(finding_verdict(&limited), Verdict::Limited);
    let mut null_limit = finding("reachable_unrevealed", 10, "candidate_current");
    null_limit["static_limit_kind"] = Value::Null;
    assert_eq!(finding_verdict(&null_limit), Verdict::Gap);
}

#[test]
fn anchored_findings_keep_only_candidate_current_findings_on_the_anchor() {
    let check = json!({"findings": [
        finding("exposed", 10, "candidate_current"),
        finding("reachable_unrevealed", 10, "base_deleted"),
        finding("reachable_unrevealed", 11, "candidate_current"),
    ]});
    let anchored = anchored_findings(&check, &anchor(), None);
    assert_eq!(anchored.len(), 1);
    assert_eq!(case_verdict(&anchored), Verdict::Credited);
    let other = Anchor {
        file: "src/other.rs".to_string(),
        line: 10,
    };
    assert_eq!(
        case_verdict(&anchored_findings(&check, &other, None)),
        Verdict::Silent
    );
}

#[test]
fn declared_binding_reads_only_a_let_declaration() {
    assert_eq!(
        declared_binding("    let end = input.rfind(delim).map_or(0, |i| i);"),
        Some("end".to_string())
    );
    assert_eq!(
        declared_binding("let mut total_cents: u64 = 0;"),
        Some("total_cents".to_string())
    );
    assert_eq!(declared_binding("    end == 0"), None);
    assert_eq!(declared_binding("    letter = 1;"), None);
    assert_eq!(declared_binding("let (a, b) = pair;"), None);
    assert_eq!(declared_binding("let Some(x) = maybe;"), None);
    assert_eq!(declared_binding("let Point { x, y } = p;"), None);
    assert_eq!(declared_binding("let ref x = y;"), None);
    assert_eq!(declared_binding("let r#type = 1;"), None);
}

#[test]
fn retarget_relation_matches_an_initializer_containing_backticks() {
    let relation = "binding_predicate_relation: changed binding `cut` initializer `input.find('`')` -> `input.rfind('`')` flows into predicate operand at line 13";
    assert!(is_anchor_relation(
        relation,
        "    let cut = input.rfind('`');",
        "cut"
    ));
    assert!(!is_anchor_relation(
        relation,
        "    let cut = input.find('`');",
        "cut"
    ));
    assert!(!is_anchor_relation(
        relation,
        "    let cut = input.rfind('`');",
        "end"
    ));
    // Only the declaration's own initializer counts, not text after a later
    // `=` inside it.
    let tail_eq = "binding_predicate_relation: changed binding `ok` initializer `b` flows into predicate operand at line 13";
    assert!(!is_anchor_relation(tail_eq, "    let ok = a == b;", "ok"));
    assert_eq!(let_initializer("let ok = a == b"), Some("a == b"));
    assert_eq!(let_initializer("let x: Vec<u8> = g()"), Some("g()"));
    assert_eq!(let_initializer("let x = |a| a >= 1"), Some("|a| a >= 1"));
    assert_eq!(
        let_initializer("let it: Box<dyn Iterator<Item = u32>> = b()"),
        Some("b()")
    );
    assert_eq!(let_initializer("let f: fn(u8) -> u8 = g"), Some("g"));
    assert_eq!(
        let_initializer("let end: Option<usize>=Some(text.len())"),
        Some("Some(text.len())")
    );
    assert_eq!(let_initializer("let x: Vec<Vec<u8>>= v"), Some("v"));
    assert_eq!(let_initializer("let ok = a <= b"), Some("a <= b"));
    // Without a distinct old initializer the relation names the new one alone.
    let single = "binding_predicate_relation: changed binding `cut` initializer `input.rfind('`')` flows into predicate operand at line 13";
    assert!(is_anchor_relation(
        single,
        "    let cut = input.rfind('`');",
        "cut"
    ));
    assert!(!is_anchor_relation(
        single,
        "    let cut = input.find('`');",
        "cut"
    ));
}

#[test]
fn anchored_findings_follow_a_retarget_only_for_the_anchor_binding() {
    let relation = "binding_predicate_relation: changed binding `end` initializer `a.find(d)` -> `a.rfind(d)` flows into predicate operand at line 13";
    let anchor_line = "    let end = a.rfind(d);";
    let mut retargeted = finding("weakly_exposed", 13, "candidate_current");
    retargeted["evidence"] = json!([relation]);
    let mut other_binding = finding("exposed", 14, "candidate_current");
    other_binding["evidence"] = json!([
        "binding_predicate_relation: changed binding `start` initializer `a` -> `b` flows into predicate operand at line 14"
    ]);
    let mut other_file = finding("exposed", 13, "candidate_current");
    other_file["evidence"] = json!([relation]);
    other_file["probe"]["file"] = json!("src/other.rs");
    let check = json!({"findings": [retargeted, other_binding, other_file]});
    // Without a `let` anchor the anchor line alone counts, and nothing sits
    // there.
    assert!(anchored_findings(&check, &anchor(), None).is_empty());
    assert!(anchored_findings(&check, &anchor(), Some("    end == 0")).is_empty());
    // A same-named `let` elsewhere in the diff has another initializer.
    assert!(anchored_findings(&check, &anchor(), Some("    let end = a.len();")).is_empty());
    let followed = anchored_findings(&check, &anchor(), Some(anchor_line));
    assert_eq!(followed.len(), 1, "{followed:#?}");
    assert_eq!(
        followed[0].pointer("/probe/line").and_then(Value::as_u64),
        Some(13)
    );
    assert_eq!(case_verdict(&followed), Verdict::Gap);
}

#[test]
fn case_verdict_ranks_gap_over_credit_over_limit() {
    let gap = finding("weakly_exposed", 10, "candidate_current");
    let credit = finding("exposed", 10, "candidate_current");
    let limit = finding("static_unknown", 10, "candidate_current");
    assert_eq!(case_verdict(&[&credit, &gap, &limit]), Verdict::Gap);
    assert_eq!(case_verdict(&[&limit, &credit]), Verdict::Credited);
    assert_eq!(case_verdict(&[&limit]), Verdict::Limited);
    assert_eq!(case_verdict(&[]), Verdict::Silent);
}

#[test]
fn contradictions_flag_each_internal_inconsistency_and_pass_a_clean_finding() {
    let clean = finding("reachable_unrevealed", 10, "candidate_current");
    assert!(finding_contradictions(&clean).is_empty());

    let mut reach = clean.clone();
    reach["related_tests_total"] = json!(0);
    reach["related_tests"] = json!([]);
    assert_eq!(
        finding_contradictions(&reach),
        vec!["reach_yes_without_related_tests"]
    );

    let mut no_path = clean.clone();
    no_path["classification"] = json!("no_static_path");
    no_path["ripr"]["reach"]["state"] = json!("no");
    no_path["related_tests_total"] = json!(81);
    assert_eq!(
        finding_contradictions(&no_path),
        vec!["no_static_path_with_related_tests"]
    );

    let mut exposed = clean.clone();
    exposed["classification"] = json!("exposed");
    assert_eq!(
        finding_contradictions(&exposed),
        vec!["exposed_without_discriminator"]
    );
    exposed["ripr"]["discriminate"]["state"] = json!("yes");
    assert!(finding_contradictions(&exposed).is_empty());

    let mut listed = clean;
    listed["related_tests"] = json!([{"name": "a"}, {"name": "b"}]);
    assert_eq!(
        finding_contradictions(&listed),
        vec!["related_tests_listed_exceed_total"]
    );
}

#[test]
fn summary_contradictions_compare_counts_with_the_findings_list() {
    let consistent = json!({
        "summary": {"findings": 1, "exposed": 0, "reachable_unrevealed": 1},
        "findings": [finding("reachable_unrevealed", 10, "candidate_current")],
    });
    assert!(summary_contradictions(&consistent).is_empty());
    let drifted = json!({
        "summary": {"findings": 2, "exposed": 1, "reachable_unrevealed": 1},
        "findings": [finding("reachable_unrevealed", 10, "candidate_current")],
    });
    assert_eq!(
        summary_contradictions(&drifted),
        vec![
            "summary_findings_count_mismatch".to_string(),
            "summary_exposed_count_mismatch".to_string()
        ]
    );
}

#[test]
fn ratio_text_is_fixed_precision_and_names_an_empty_denominator() {
    assert_eq!(ratio(3, 21).rate, "0.1429");
    assert_eq!(ratio(1, 3).rate, "0.3333");
    assert_eq!(ratio(2, 3).rate, "0.6667");
    assert_eq!(ratio(1, 1).rate, "1.0000");
    assert_eq!(ratio(0, 5).rate, "0.0000");
    assert_eq!(ratio(0, 0).rate, "n/a");
}

const PATCH: &str = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -2,3 +2,3 @@ fn f() {\n one\n-    if n >= 100 {\n+    if n > 99 {\n three\n";

#[test]
fn apply_patch_rewrites_the_anchored_line_and_reports_it_as_added() -> Result<(), String> {
    let patches = parse_patch(PATCH)?;
    assert_eq!(patches.len(), 1);
    assert_eq!(patches[0].path, "src/lib.rs");
    assert_eq!(patches[0].added_lines(), BTreeSet::from([3]));
    let original = "zero\none\n    if n >= 100 {\nthree\nfour\n";
    let patched = apply_patch(original, &patches[0])?;
    assert_eq!(patched, "zero\none\n    if n > 99 {\nthree\nfour\n");
    Ok(())
}

#[test]
fn apply_patch_refuses_drifted_context() -> Result<(), String> {
    let patches = parse_patch(PATCH)?;
    let drifted = "zero\none\n    if n >= 101 {\nthree\n";
    let error = apply_patch(drifted, &patches[0]).err().unwrap_or_default();
    assert!(
        error.contains("does not match the patch context"),
        "{error}"
    );
    Ok(())
}

#[test]
fn parse_patch_refuses_renames_and_empty_input() {
    let rename = "--- a/src/a.rs\n+++ b/src/b.rs\n@@ -1 +1 @@\n-a\n+b\n";
    let rename_error = parse_patch(rename).err().unwrap_or_default();
    assert!(rename_error.contains("rename"), "{rename_error}");
    let empty_error = parse_patch("").err().unwrap_or_default();
    assert!(
        empty_error.contains("no `---`/`+++` file patches"),
        "{empty_error}"
    );
}

#[test]
fn parse_patch_holds_hunks_to_their_declared_counts_and_starts() {
    // A new start that does not follow from the old start would move the
    // anchor off the line the edit really adds.
    let shifted = PATCH.replace("+2,3 @@", "+52,3 @@");
    let shifted_error = parse_patch(&shifted).err().unwrap_or_default();
    assert!(
        shifted_error.contains("does not follow from old start 2"),
        "{shifted_error}"
    );
    // A truncated hunk is refused rather than applied partially.
    let truncated = PATCH.strip_suffix(" three\n").unwrap_or_default();
    let truncated_error = parse_patch(truncated).err().unwrap_or_default();
    assert!(
        truncated_error.contains("hunk ends early"),
        "{truncated_error}"
    );
    // Pure insertions and deletions cannot pin the anchor, so they are refused.
    let insertion = "--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -769,0 +820,1 @@\n+x\n";
    let insertion_error = parse_patch(insertion).err().unwrap_or_default();
    assert!(
        insertion_error.contains("only inserts or only deletes"),
        "{insertion_error}"
    );
    // A second file header is never swallowed as a removed line.
    let two_files = format!(
        "{}--- a/src/two.rs\n+++ b/src/two.rs\n@@ -1 +1 @@\n-x\n+y\n",
        PATCH
    );
    let patches = parse_patch(&two_files).unwrap_or_default();
    assert_eq!(
        patches.iter().map(|p| p.path.as_str()).collect::<Vec<_>>(),
        vec!["src/lib.rs", "src/two.rs"]
    );
}

#[test]
fn summary_contradictions_account_for_suppressed_findings() {
    let suppressed = json!({
        "summary": {"findings": 2, "reachable_unrevealed": 1, "exposed": 0, "suppressed_by_policy": 1},
        "findings": [
            finding("reachable_unrevealed", 10, "candidate_current"),
            finding("reachable_unrevealed", 11, "candidate_current"),
        ],
    });
    assert!(
        summary_contradictions(&suppressed).is_empty(),
        "{:?}",
        summary_contradictions(&suppressed)
    );
    let mut drifted = suppressed.clone();
    drifted["summary"]["suppressed_by_policy"] = json!(0);
    assert_eq!(
        summary_contradictions(&drifted),
        vec!["summary_suppression_count_mismatch".to_string()]
    );
}

#[test]
fn committed_corpus_is_valid_and_measures_both_error_directions() -> Result<(), String> {
    let dir = repo_corpus_dir();
    let corpus = load_corpus(&dir)?;
    let violations = validate(&corpus, &dir);
    assert!(violations.is_empty(), "{violations:#?}");
    assert!(
        corpus.cases.len() >= 20,
        "corpus shrank to {}",
        corpus.cases.len()
    );
    let subjects: BTreeSet<&str> = corpus
        .subjects
        .iter()
        .map(|s| s.subject_id.as_str())
        .collect();
    assert!(subjects.len() >= 5, "{subjects:?}");
    assert!(corpus.cases.iter().any(|c| c.hard_case.is_some()));
    // The serde 256-value loop is the named hard case this corpus exists to keep.
    assert!(
        corpus
            .cases
            .iter()
            .any(|c| c.case_id == "serde-format-u8-hundreds"
                && c.truth.state == TruthState::Discriminated)
    );
    Ok(())
}

/// Position of the first committed case matching `matches`, in assembled order.
fn case_index(matches: impl Fn(&Value) -> bool) -> Result<usize, String> {
    let raw = corpus_value(&repo_corpus_dir())?;
    raw["cases"]
        .as_array()
        .and_then(|cases| cases.iter().position(matches))
        .ok_or_else(|| "the committed corpus has no matching case".to_string())
}

fn tampered(edit: impl Fn(&mut Value)) -> Result<Vec<String>, String> {
    let dir = repo_corpus_dir();
    let mut raw = corpus_value(&dir)?;
    edit(&mut raw);
    let corpus: Corpus = serde_json::from_value(raw).map_err(|err| err.to_string())?;
    Ok(validate(&corpus, &dir))
}

#[test]
fn validator_rejects_a_label_that_contradicts_its_mutant_outcomes() -> Result<(), String> {
    let violations = tampered(|raw| {
        raw["cases"][0]["truth"]["mutants"][0]["outcome"] = json!("tests_passed");
        raw["cases"][0]["truth"]["mutants"][0]["failing_test"] = Value::Null;
    })?;
    assert!(
        violations
            .iter()
            .any(|v| v.contains("does not follow from")),
        "{violations:#?}"
    );
    Ok(())
}

#[test]
fn validator_requires_a_replayable_mutated_line_that_changes_the_anchor() -> Result<(), String> {
    // The split layout assembles cases in file-name order, so locate the
    // serde case by id rather than by position.
    let i = case_index(|case| case["case_id"] == json!("serde-format-u8-hundreds"))?;
    let missing = tampered(|raw| {
        if let Some(mutant) = raw["cases"][i]["truth"]["mutants"][0].as_object_mut() {
            mutant.remove("mutated_line");
        }
    })?;
    assert!(
        missing.iter().any(|v| v.contains("has no mutated_line")),
        "{missing:#?}"
    );
    // The serde case edits `if n >= 100 {` to `if n > 99 {`; replaying that same
    // line is a mutant that changes nothing.
    let no_op = tampered(|raw| {
        raw["cases"][i]["truth"]["mutants"][0]["mutated_line"] = json!("if n > 99 {");
    })?;
    assert!(
        no_op
            .iter()
            .any(|v| v.contains("equals the edited anchor line")),
        "{no_op:#?}"
    );
    let listed = tampered(|raw| {
        raw["cases"][i]["truth"]["mutants"][0]["failing_test"] = json!("a, b (+3 more)");
    })?;
    assert!(
        listed.iter().any(|v| v.contains("is not one test name")),
        "{listed:#?}"
    );
    // Each half of the rule separately: an interior newline, and padding
    // that would slip a no-op past the trimmed-anchor comparison.
    for bad in ["if n > 100 {\nx", " if n > 99 {"] {
        let shaped = tampered(|raw| {
            raw["cases"][i]["truth"]["mutants"][0]["mutated_line"] = json!(bad);
        })?;
        assert!(
            shaped
                .iter()
                .any(|v| v.contains("must be one trimmed line")),
            "{bad:?}: {shaped:#?}"
        );
    }
    Ok(())
}

#[test]
fn validator_refuses_a_test_command_the_replay_cannot_run() -> Result<(), String> {
    for command in [
        "cargo test --manifest-path /elsewhere/Cargo.toml",
        "make test",
    ] {
        let refused = tampered(|raw| {
            raw["cases"][0]["truth"]["test_command"] = json!(command);
        })?;
        assert!(
            refused.iter().any(|v| v.contains("cannot be replayed")),
            "{command}: {refused:#?}"
        );
    }
    Ok(())
}

#[test]
fn validator_refuses_a_mutated_line_on_a_behavior_change() -> Result<(), String> {
    let index = case_index(|case| case["edit_kind"] == json!("behavior_change"))?;
    let violations = tampered(|raw| {
        raw["cases"][index]["truth"]["mutants"][0]["mutated_line"] = json!("x");
    })?;
    assert!(
        violations
            .iter()
            .any(|v| v.contains("whose mutant is the edit itself")),
        "{violations:#?}"
    );
    Ok(())
}

#[test]
fn validator_rejects_an_expected_verdict_outside_the_truth_table() -> Result<(), String> {
    let violations = tampered(|raw| {
        raw["cases"][0]["expected"]["ideal_verdict"] = json!("gap");
        raw["cases"][0]["expected"]["acceptable_verdicts"] = json!(["gap", "credited"]);
    })?;
    assert!(
        violations.iter().any(|v| v.contains("contradicts truth")),
        "{violations:#?}"
    );
    assert!(
        violations
            .iter()
            .any(|v| v.contains("drift from the truth table")),
        "{violations:#?}"
    );
    Ok(())
}

#[test]
fn validator_rejects_a_retained_file_whose_digest_moved() -> Result<(), String> {
    let violations = tampered(|raw| {
        raw["subjects"][0]["retained_files"][0]["sha256"] = json!("0".repeat(64));
    })?;
    assert!(
        violations
            .iter()
            .any(|v| v.contains("does not match its pinned sha256")),
        "{violations:#?}"
    );
    Ok(())
}

#[test]
fn validator_rejects_a_case_that_borrows_another_cases_diff() -> Result<(), String> {
    let violations = tampered(|raw| {
        raw["cases"][0]["diff"] = raw["cases"][1]["diff"].clone();
    })?;
    assert!(
        violations
            .iter()
            .any(|v| v.contains("diff `cases/") && v.contains("` is not `cases/")),
        "{violations:#?}"
    );
    Ok(())
}

#[test]
fn validator_rejects_an_anchor_the_diff_does_not_add() -> Result<(), String> {
    let violations = tampered(|raw| {
        let line = raw["cases"][0]["anchor"]["line"].as_u64().unwrap_or(0);
        raw["cases"][0]["anchor"]["line"] = json!(line + 1);
    })?;
    assert!(
        violations
            .iter()
            .any(|v| v.contains("diff does not add line")),
        "{violations:#?}"
    );
    Ok(())
}

#[test]
fn validator_rejects_ids_that_are_not_one_safe_path_segment() -> Result<(), String> {
    let violations = tampered(|raw| {
        raw["cases"][0]["case_id"] = json!("../escape");
        raw["subjects"][0]["subject_id"] = json!("a/b");
        raw["cases"][1]["case_id"] = json!(".cache");
    })?;
    assert!(
        violations
            .iter()
            .any(|v| v.contains("case id `.cache` is not a single safe path segment")),
        "{violations:#?}"
    );
    assert!(
        violations
            .iter()
            .any(|v| v.contains("case id `../escape` is not a single safe path segment")),
        "{violations:#?}"
    );
    assert!(
        violations
            .iter()
            .any(|v| v.contains("subject id `a/b` is not a single safe path segment")),
        "{violations:#?}"
    );
    Ok(())
}

#[test]
fn validator_rejects_a_diff_that_patches_an_unretained_path() -> Result<(), String> {
    let dir = repo_corpus_dir();
    let mut raw = corpus_value(&dir)?;
    let case_diff = raw["cases"][0]["diff"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    let original = read(&dir.join(&case_diff))?;
    let escape = "diff --git a/../../Cargo.toml b/../../Cargo.toml\n--- a/../../Cargo.toml\n+++ b/../../Cargo.toml\n@@ -1,1 +1,1 @@\n-[workspace]\n+[package]\n";
    let scratch =
        std::env::temp_dir().join(format!("verdict-corpus-escape-{}", std::process::id()));
    fs::create_dir_all(scratch.join("cases")).map_err(|err| err.to_string())?;
    fs::write(
        scratch.join("cases/escape.diff"),
        format!("{original}{escape}"),
    )
    .map_err(|err| err.to_string())?;
    raw["cases"][0]["diff"] = json!("cases/escape.diff");
    let corpus: Corpus = serde_json::from_value(raw).map_err(|err| err.to_string())?;
    let case = &corpus.cases[0];
    let subject = corpus
        .subjects
        .iter()
        .find(|s| s.subject_id == case.subject_id)
        .ok_or("subject")?;
    let violations = case_violations(case, subject, &scratch);
    let _ = fs::remove_dir_all(&scratch);
    assert!(
        violations
            .iter()
            .any(|v| v.contains("diff patches `../../Cargo.toml`, which is unsafe or not retained")),
        "{violations:#?}"
    );
    Ok(())
}

#[test]
fn validator_requires_both_truth_directions() -> Result<(), String> {
    let violations = tampered(|raw| {
        if let Some(cases) = raw["cases"].as_array_mut() {
            cases.retain(|c| c["truth"]["state"] != json!("not_discriminated"));
        }
    })?;
    assert!(
        violations
            .iter()
            .any(|v| v.contains("no `not_discriminated` case")),
        "{violations:#?}"
    );
    Ok(())
}

#[test]
fn expected_report_rows_agree_with_corpus_labels() -> Result<(), String> {
    let dir = repo_corpus_dir();
    let corpus = load_corpus(&dir)?;
    let rows_dir = dir.join("expected").join(ROWS_DIR);
    let row_files = files_under(&rows_dir)?;
    assert_eq!(row_files.len(), corpus.cases.len(), "{row_files:?}");
    for case in &corpus.cases {
        let row: Value =
            serde_json::from_str(&read(&rows_dir.join(format!("{}.json", case.case_id)))?)
                .map_err(|err| err.to_string())?;
        assert_eq!(row["case_id"], json!(case.case_id));
        assert_eq!(row["truth"], json!(case.truth.state.as_str()));
        let observed: Verdict =
            serde_json::from_value(row["observed_verdict"].clone()).map_err(|e| e.to_string())?;
        assert_eq!(
            row["outcome"],
            json!(score(case.truth.state, observed).as_str()),
            "{}",
            case.case_id
        );
    }
    Ok(())
}

#[test]
fn build_report_counts_rates_over_the_right_denominators() -> Result<(), String> {
    let dir = repo_corpus_dir();
    let corpus = load_corpus(&dir)?;
    // Every case observed as a gap: discriminated cases become false
    // actionable, the rest are ideal.
    let checks: Vec<(String, Value)> = corpus
        .cases
        .iter()
        .map(|case| {
            let mut f = finding(
                "weakly_exposed",
                case.anchor.line as u64,
                "candidate_current",
            );
            f["probe"]["file"] = json!(case.anchor.file);
            (case.case_id.clone(), json!({"findings": [f]}))
        })
        .collect();
    let report = build_report(&corpus, &checks, &BTreeMap::new())?;
    let discriminated = corpus
        .cases
        .iter()
        .filter(|c| c.truth.state == TruthState::Discriminated)
        .count();
    assert_eq!(report.false_actionable_rate.numerator, discriminated);
    assert_eq!(report.false_actionable_rate.denominator, discriminated);
    assert_eq!(report.false_exposed_rate.numerator, 0);
    assert_eq!(report.false_verdict_rate.numerator, discriminated);
    assert_eq!(
        report.ideal_rate.numerator,
        corpus.cases.len() - discriminated
    );
    let missing = build_report(&corpus, &checks[1..], &BTreeMap::new())
        .err()
        .unwrap_or_default();
    assert!(missing.contains("no ripr result for case"), "{missing}");
    Ok(())
}

#[test]
fn validator_holds_each_subject_origin_to_its_own_provenance() -> Result<(), String> {
    // An upstream subject must still name its pinned repository.
    let violations = tampered(|raw| {
        raw["subjects"][0]["upstream"] = Value::Null;
        raw["subjects"][0]["commit"] = Value::Null;
    })?;
    assert!(
        violations
            .iter()
            .any(|v| v.contains("upstream is not an https URL")),
        "{violations:#?}"
    );
    assert!(
        violations
            .iter()
            .any(|v| v.contains("commit is not a 40-hex sha")),
        "{violations:#?}"
    );
    // An upstream excerpt relabeled as authored but still naming its
    // upstream repository is refused.
    let violations = tampered(|raw| {
        raw["subjects"][0]["origin"] = json!("authored");
    })?;
    assert!(
        violations
            .iter()
            .any(|v| v.contains("names an upstream, commit or shared corpus entry")),
        "{violations:#?}"
    );
    // Authored code is this repository's code, under its license.
    let violations = tampered(|raw| {
        raw["subjects"][0]["origin"] = json!("authored");
        raw["subjects"][0]["license"] = json!("MIT");
    })?;
    assert!(
        violations
            .iter()
            .any(|v| v.contains("license is not `MIT OR Apache-2.0`")),
        "{violations:#?}"
    );
    // A full relabel that drops the upstream fields still fails: authored ids
    // carry the `authored-` prefix and retain no LICENSE file. Renaming the
    // subject and every case that names it is the remaining route, and that
    // shows in review, not in this validator.
    let violations = tampered(|raw| {
        raw["subjects"][0]["origin"] = json!("authored");
        raw["subjects"][0]["upstream"] = Value::Null;
        raw["subjects"][0]["commit"] = Value::Null;
        raw["subjects"][0]["shared_corpus"] = Value::Null;
    })?;
    assert!(
        violations
            .iter()
            .any(|v| v.contains("must be named `authored-<name>`")),
        "{violations:#?}"
    );
    assert!(
        violations
            .iter()
            .any(|v| v.contains("retains a LICENSE file")),
        "{violations:#?}"
    );
    // An upstream subject cannot take the authored prefix.
    let violations = tampered(|raw| {
        raw["subjects"][0]["subject_id"] = json!("authored-serde");
    })?;
    assert!(
        violations
            .iter()
            .any(|v| v.contains("reserved for authored subjects")),
        "{violations:#?}"
    );
    // An unknown origin is a parse error, not a silent default.
    let unknown = tampered(|raw| {
        raw["subjects"][0]["origin"] = json!("borrowed");
    });
    assert!(unknown.is_err(), "{unknown:?}");
    Ok(())
}

#[test]
fn report_keeps_authored_rates_apart_from_upstream_rates() -> Result<(), String> {
    let dir = repo_corpus_dir();
    let mut corpus = load_corpus(&dir)?;
    // Start from the upstream cases only, so the authored side holds exactly
    // the one case this test moves there.
    let upstream: BTreeSet<String> = corpus
        .subjects
        .iter()
        .filter(|s| s.origin == SubjectOrigin::Upstream)
        .map(|s| s.subject_id.clone())
        .collect();
    corpus.cases.retain(|c| upstream.contains(&c.subject_id));
    // Cases load in file-name order; move a discriminated one to the front.
    let Some(at) = corpus
        .cases
        .iter()
        .position(|c| c.truth.state == TruthState::Discriminated)
    else {
        return Err("committed corpus has no discriminated upstream case".to_string());
    };
    corpus.cases.swap(0, at);
    let first = corpus.cases[0].clone();
    // One discriminated case moves to an authored subject; every case is
    // observed as a gap, so it is false actionable on the authored side only.
    let mut authored_subject = corpus
        .subjects
        .iter()
        .find(|s| s.subject_id == first.subject_id)
        .cloned()
        .ok_or("first case has no subject")?;
    authored_subject.subject_id = "authored-probe".to_string();
    authored_subject.origin = SubjectOrigin::Authored;
    corpus.subjects.push(authored_subject);
    corpus.cases[0].subject_id = "authored-probe".to_string();
    assert_eq!(corpus.cases[0].truth.state, TruthState::Discriminated);
    let checks: Vec<(String, Value)> = corpus
        .cases
        .iter()
        .map(|case| {
            let mut f = finding(
                "weakly_exposed",
                case.anchor.line as u64,
                "candidate_current",
            );
            f["probe"]["file"] = json!(case.anchor.file);
            (case.case_id.clone(), json!({"findings": [f]}))
        })
        .collect();
    let report = build_report(&corpus, &checks, &BTreeMap::new())?;
    let authored = report
        .by_origin
        .get("authored")
        .ok_or("no authored rates")?;
    let upstream = report
        .by_origin
        .get("upstream")
        .ok_or("no upstream rates")?;
    assert_eq!(authored.cases_total, 1);
    assert_eq!(
        (
            authored.false_actionable_rate.numerator,
            authored.false_actionable_rate.denominator
        ),
        (1, 1)
    );
    assert_eq!(upstream.cases_total, corpus.cases.len() - 1);
    assert_eq!(
        upstream.false_actionable_rate.numerator + authored.false_actionable_rate.numerator,
        report.false_actionable_rate.numerator
    );
    assert_eq!(report.rows[0].origin, SubjectOrigin::Authored);
    let markdown = render_report_markdown(&report, "rust");
    assert!(
        markdown.contains("| authored | 1 | 1/1 | 1/1 |"),
        "{markdown}"
    );
    Ok(())
}

#[test]
fn relativize_probe_files_strips_only_the_run_root() {
    let mut check = json!({"findings": [
        {"source_currentness": "candidate_current", "probe": {"file": "/work/case/src/lib.rs", "line": 10}},
        {"source_currentness": "candidate_current", "probe": {"file": "./src/lib.rs", "line": 10}},
        {"source_currentness": "candidate_current", "probe": {"file": "/elsewhere/src/lib.rs", "line": 10}},
    ]});
    relativize_probe_files(&mut check, Path::new("/work/case"));
    assert_eq!(check["findings"][0]["probe"]["file"], json!("src/lib.rs"));
    assert_eq!(check["findings"][1]["probe"]["file"], json!("./src/lib.rs"));
    assert_eq!(
        check["findings"][2]["probe"]["file"],
        json!("/elsewhere/src/lib.rs")
    );
    // `./src/lib.rs` already reads as root-relative; the foreign root does not.
    assert_eq!(anchored_findings(&check, &anchor(), None).len(), 2);
}

#[test]
fn contradiction_counts_use_one_per_finding_unit() -> Result<(), String> {
    let dir = repo_corpus_dir();
    let corpus = load_corpus(&dir)?;
    let mut contradicted = finding("reachable_unrevealed", 10, "candidate_current");
    contradicted["related_tests_total"] = json!(0);
    contradicted["related_tests"] = json!([]);
    let mut other_line = contradicted.clone();
    other_line["probe"]["line"] = json!(99);
    let checks: Vec<(String, Value)> = corpus
        .cases
        .iter()
        .enumerate()
        .map(|(i, case)| {
            let findings = if i == 0 {
                json!([contradicted, other_line])
            } else {
                json!([])
            };
            (case.case_id.clone(), json!({"findings": findings}))
        })
        .collect();
    let report = build_report(&corpus, &checks, &BTreeMap::new())?;
    // Two contradicted findings in one case: the rate and the per-code count
    // agree, while the row lists the code once.
    assert_eq!(report.contradiction_rate.numerator, 2);
    assert_eq!(report.contradiction_rate.denominator, 2);
    assert_eq!(
        report
            .contradictions_by_code
            .get("reach_yes_without_related_tests"),
        Some(&2)
    );
    assert_eq!(
        report.rows[0].contradictions,
        vec!["reach_yes_without_related_tests".to_string()]
    );
    Ok(())
}

#[test]
fn stored_paths_keep_vendored_rust_out_of_the_workspace() {
    assert_eq!(stored_path("src/lib.rs"), "src/lib.rs.txt");
    assert_eq!(stored_path("Cargo.toml"), "Cargo.toml");
    assert_eq!(
        logical_path("src/lib.rs.txt").as_deref(),
        Some("src/lib.rs")
    );
    assert_eq!(logical_path("LICENSE-MIT").as_deref(), Some("LICENSE-MIT"));
    // A bare `.rs` file under subjects is refused, not silently copied.
    assert_eq!(logical_path("src/lib.rs"), None);
}

/// A minimal on-disk corpus in the per-record layout: header, one subject
/// record, and one case record per id.
fn per_record_corpus(name: &str, case_ids: &[&str]) -> Result<PathBuf, String> {
    let source = repo_corpus_dir();
    let mut raw = corpus_value(&source)?;
    let dir = crate::tests::temp_dir(name);
    let case = raw["cases"][0].clone();
    let subject_id = case["subject_id"].as_str().unwrap_or_default().to_string();
    let subject = raw["subjects"]
        .as_array()
        .and_then(|all| all.iter().find(|s| s["subject_id"] == json!(subject_id)))
        .cloned()
        .ok_or("committed corpus has no subject for its first case")?;
    if let Some(fields) = raw.as_object_mut() {
        fields.remove("subjects");
        fields.remove("cases");
    }
    crate::tests::write(&dir.join("corpus.json"), &format!("{raw:#}"));
    crate::tests::write(
        &dir.join("subjects").join(format!("{subject_id}.json")),
        &format!("{subject:#}"),
    );
    for id in case_ids {
        let mut copy = case.clone();
        copy["case_id"] = json!(id);
        crate::tests::write(
            &dir.join("cases").join(format!("{id}.json")),
            &format!("{copy:#}"),
        );
    }
    Ok(dir)
}

#[test]
fn corpus_records_load_in_file_name_order_and_must_match_their_ids() -> Result<(), String> {
    let dir = per_record_corpus("verdict-order", &["b-case", "a-case"])?;
    let corpus = load_corpus(&dir)?;
    let ids: Vec<&str> = corpus.cases.iter().map(|c| c.case_id.as_str()).collect();
    assert_eq!(ids, ["a-case", "b-case"]);
    // A copied file that keeps the original id is refused, so two files can
    // never carry one id.
    let mut copy: Value = serde_json::from_str(&read(&dir.join("cases/a-case.json"))?)
        .map_err(|err| err.to_string())?;
    copy["case_id"] = json!("b-case");
    crate::tests::write(&dir.join("cases/c-case.json"), &format!("{copy:#}"));
    let err = load_corpus(&dir).err().unwrap_or_default();
    assert!(
        err.contains("c-case.json") && err.contains("`b-case`"),
        "{err}"
    );
    // A mistyped record name or a diff without its record would drop a case
    // silently; validate names both.
    fs::remove_file(dir.join("cases/c-case.json")).map_err(|err| err.to_string())?;
    crate::tests::write(&dir.join("cases/d-case.JSON"), "{}\n");
    crate::tests::write(&dir.join("cases/e-case.diff"), "");
    fs::create_dir_all(dir.join("subjects/orphan")).map_err(|err| err.to_string())?;
    let corpus = load_corpus(&dir)?;
    let violations = validate(&corpus, &dir);
    for stray in [
        "cases/d-case.JSON",
        "cases/e-case.diff",
        "subjects/orphan belongs",
    ] {
        assert!(
            violations.iter().any(|v| v.starts_with(stray)),
            "{stray}: {violations:#?}"
        );
    }
    assert!(
        !violations.iter().any(|v| v.starts_with("cases/a-case")),
        "{violations:#?}"
    );
    Ok(())
}

#[test]
fn check_all_finds_every_language_corpus_and_refuses_one_without_a_header() -> Result<(), String> {
    let fixtures = crate::tests::temp_dir("verdict-dirs");
    for name in ["rust-verdict-corpus", "perl-verdict-corpus"] {
        crate::tests::write(&fixtures.join(name).join("corpus.json"), "{}\n");
    }
    crate::tests::write(&fixtures.join("other-corpus/corpus.json"), "{}\n");
    let ok: Vec<PathBuf> = corpus_dirs(&fixtures)?
        .into_iter()
        .collect::<Result<_, _>>()?;
    let names: Vec<String> = ok
        .iter()
        .filter_map(|d| d.file_name().map(|n| n.to_string_lossy().into_owned()))
        .collect();
    assert_eq!(names, ["perl-verdict-corpus", "rust-verdict-corpus"]);
    // Rust keeps its report path; every language owns its run directory.
    assert_eq!(default_out(&ok[1])?, PathBuf::from(DEFAULT_OUT));
    assert_eq!(default_out(&ok[0])?, Path::new(DEFAULT_OUT).join("perl"));
    assert_eq!(work_root(&ok[0])?, Path::new(WORK_ROOT).join("perl"));
    assert_eq!(work_root(&ok[1])?, Path::new(WORK_ROOT).join("rust"));
    // A language that would leave the run workspace is refused, so a case
    // directory can never be deleted outside it.
    for escaping in [
        "fixtures/..-verdict-corpus",
        "fixtures/.cache-verdict-corpus",
        ".",
    ] {
        assert!(work_root(Path::new(escaping)).is_err(), "{escaping}");
        assert!(default_out(Path::new(escaping)).is_err(), "{escaping}");
    }

    // Entries that cannot be a corpus fail in place instead of dropping out.
    fs::create_dir_all(fixtures.join("python-verdict-corpus")).map_err(|err| err.to_string())?;
    crate::tests::write(&fixtures.join("notes-verdict-corpus"), "a file\n");
    crate::tests::write(&fixtures.join("-verdict-corpus/corpus.json"), "{}\n");
    crate::tests::write(&fixtures.join("..-verdict-corpus/corpus.json"), "{}\n");
    let entries = corpus_dirs(&fixtures)?;
    let errors: Vec<String> = entries.iter().filter_map(|e| e.clone().err()).collect();
    for expected in [
        "python-verdict-corpus has no corpus.json",
        "notes-verdict-corpus is not a directory",
        "/-verdict-corpus names no usable language",
        "/..-verdict-corpus names no usable language",
    ] {
        assert!(
            errors.iter().any(|e| e.contains(expected)),
            "{expected}: {errors:?}"
        );
    }

    // One broken or drifted corpus does not stop the others being checked,
    // and every failure is reported.
    let seen = std::cell::RefCell::new(Vec::new());
    let result = check_each(entries, |dir| {
        let name = dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        seen.borrow_mut().push(name.clone());
        if name == "perl-verdict-corpus" {
            Err("perl drifted".to_string())
        } else {
            Ok(())
        }
    });
    assert_eq!(
        *seen.borrow(),
        ["perl-verdict-corpus", "rust-verdict-corpus"]
    );
    let err = result.err().unwrap_or_default();
    assert!(
        err.contains("perl drifted") && err.contains("has no corpus.json"),
        "{err}"
    );
    assert_eq!(
        check_each(vec![Ok(fixtures.join("rust-verdict-corpus"))], |_| Ok(()))?,
        1
    );

    let empty = crate::tests::temp_dir("verdict-no-dirs");
    let none = corpus_dirs(&empty).err().unwrap_or_default();
    assert!(none.contains("no *-verdict-corpus directory"), "{none}");
    Ok(())
}

#[test]
fn split_moves_the_one_file_layout_into_records_without_loss() -> Result<(), String> {
    // The whole committed corpus, upstream and authored subjects alike, as a
    // pre-split branch would carry it in one corpus.json.
    let before = corpus_value(&repo_corpus_dir())?;
    let dir = crate::tests::temp_dir("verdict-split");
    for sub in ["subjects", "cases"] {
        fs::create_dir_all(dir.join(sub)).map_err(|err| err.to_string())?;
    }
    let mut legacy = before.clone();
    legacy["corpus_version"] = json!("2026-10-04.5");
    crate::tests::write(&dir.join("corpus.json"), &format!("{legacy:#}"));
    let refused = corpus_value(&dir).err().unwrap_or_default();
    assert!(
        refused.contains("verdict-corpus split") && refused.contains("corpus_version"),
        "{refused}"
    );
    // An unsafe id is refused before anything is written.
    let mut escaping = legacy.clone();
    if let Some(last) = escaping["cases"].as_array_mut().and_then(|c| c.last_mut()) {
        last["case_id"] = json!("../escape");
    }
    crate::tests::write(&dir.join("corpus.json"), &format!("{escaping:#}"));
    let err = split(&dir).err().unwrap_or_default();
    assert!(err.contains("`../escape`"), "{err}");
    assert!(files_under(&dir.join("cases"))?.is_empty());
    assert!(files_under(&dir.join("subjects"))?.is_empty());
    assert!(!dir.join("escape.json").exists());
    crate::tests::write(&dir.join("corpus.json"), &format!("{legacy:#}"));
    split(&dir)?;
    assert_eq!(corpus_value(&dir)?, before);

    // A hand-written record that leaves out an optional key is the same
    // record, not a conflict.
    let Some(case) = before["cases"]
        .as_array()
        .and_then(|cases| cases.iter().find(|c| c["hard_case"].is_null()))
        .cloned()
    else {
        return Err("committed corpus has no case without a hard-case note".to_string());
    };
    let id = case["case_id"].as_str().unwrap_or_default().to_string();
    let mut sparse = case.clone();
    if let Some(fields) = sparse.as_object_mut() {
        fields.remove("hard_case");
    }
    crate::tests::write(
        &dir.join("cases").join(format!("{id}.json")),
        &format!("{sparse:#}"),
    );
    crate::tests::write(&dir.join("corpus.json"), &format!("{legacy:#}"));
    split(&dir)?;

    // A record that exists with other content is kept and named.
    let mut changed = legacy.clone();
    if let Some(cases) = changed["cases"].as_array_mut()
        && let Some(target) = cases.iter_mut().find(|c| c["case_id"] == json!(id))
    {
        target["reasoning"] = json!("edited on this branch");
    }
    crate::tests::write(&dir.join("corpus.json"), &format!("{changed:#}"));
    let err = split(&dir).err().unwrap_or_default();
    assert!(err.contains(&format!("{id}.json")), "{err}");
    // corpus.json keeps the branch's copy until the conflict is reconciled.
    let kept = parse_json(&dir.join("corpus.json"))?;
    assert_eq!(kept, changed);
    Ok(())
}

fn scored_report(corpus: &Corpus) -> Result<Report, String> {
    let checks: Vec<(String, Value)> = corpus
        .cases
        .iter()
        .map(|case| (case.case_id.clone(), json!({"findings": []})))
        .collect();
    build_report(corpus, &checks, &BTreeMap::new())
}

#[test]
fn drift_names_moved_missing_and_stale_rows_and_a_subset_compares_only_its_rows()
-> Result<(), String> {
    let dir = per_record_corpus("verdict-drift", &["a-case", "b-case"])?;
    let corpus = load_corpus(&dir)?;
    let report = scored_report(&corpus)?;
    let expected = dir.join("expected");
    bless(&expected, &report)?;
    assert!(expected_drift(&expected, &report, true)?.is_empty());

    // A moved verdict is named by case.
    let mut moved = report.clone();
    moved.rows[1].observed_verdict = Verdict::Gap;
    let drift = expected_drift(&expected, &moved, true)?;
    assert!(
        drift.iter().any(|d| d.starts_with("case `b-case`")),
        "{drift:?}"
    );

    // A `--cases a-case` run reads only that row, so b-case's move is out of
    // its scope.
    let mut subset = corpus.clone();
    select_cases(&mut subset, &["a-case".to_string()])?;
    let mut subset_report = scored_report(&subset)?;
    assert!(expected_drift(&expected, &subset_report, false)?.is_empty());
    subset_report.rows[0].observed_verdict = Verdict::Credited;
    assert_eq!(expected_drift(&expected, &subset_report, false)?.len(), 1);

    // A whole-corpus run also needs every row present and no extra files; a
    // committed summary is one of them, since the summary is derived.
    fs::remove_file(expected.join("rows/a-case.json")).map_err(|err| err.to_string())?;
    crate::tests::write(&expected.join("rows/gone-case.json"), "{}\n");
    crate::tests::write(&expected.join("report.json"), "{}\n");
    crate::tests::write(&expected.join("summary.json"), "{}\n");
    let drift = expected_drift(&expected, &report, true)?;
    assert!(
        drift.iter().any(|d| d.contains("no expected row")),
        "{drift:?}"
    );
    assert!(
        drift.iter().any(|d| d.contains("rows/gone-case.json")),
        "{drift:?}"
    );
    assert!(
        drift.iter().any(|d| d.contains("expected/report.json")),
        "{drift:?}"
    );
    assert!(
        drift
            .iter()
            .any(|d| d.contains("expected/summary.json is not part of the expected state")),
        "{drift:?}"
    );

    // bless restores exactly the expected state and writes no summary.
    bless(&expected, &report)?;
    assert!(expected_drift(&expected, &report, true)?.is_empty());
    assert!(!expected.join("summary.json").exists());
    let unknown = select_cases(&mut corpus.clone(), &["no-such-case".to_string()])
        .err()
        .unwrap_or_default();
    assert!(unknown.contains("no-such-case"), "{unknown}");
    // An empty selection would score nothing and pass, so it is refused.
    let empty = select_cases(&mut corpus.clone(), &[])
        .err()
        .unwrap_or_default();
    assert!(empty.contains("names no case"), "{empty}");
    Ok(())
}

#[test]
fn one_test_name_accepts_a_doctest_name_and_refuses_lists() {
    for good in [
        "tests::x",
        "x",
        "src/lib.rs - read_u16 (line 8)",
        "src/lib.rs - Codec::decode (line 120)",
        "src/lib.rs - Foo<T>::bar (line 3)",
        "src/lib.rs - (line 1)",
        "src/lib.rs - read_u16 (line 8) - compile fail",
    ] {
        assert!(is_one_test_name(good), "{good}");
    }
    for bad in [
        "",
        "a, b (+3 more)",
        "a b",
        "src/lib.rs - read_u16 (line )",
        "src/lib.rs - read_u16 (line 8x)",
        "src/lib.rs - read u16 (line 8)",
        "src/lib.rs - a, b (line 8)",
        " - read_u16 (line 8)",
        "src/lib.rs - HashMap<K, V>::get (line 4)",
        "src/lib.rs - a (line 1), src/lib.rs - b (line 2)",
        "src/lib.rs - (line 1) - compile fail - compile fail",
        "src/lib.rs - (line )",
    ] {
        assert!(!is_one_test_name(bad), "{bad:?}");
    }
}

#[test]
fn summary_derived_from_blessed_rows_equals_the_run_summary() -> Result<(), String> {
    let dir = per_record_corpus("verdict-derived-summary", &["a-case", "b-case"])?;
    let corpus = load_corpus(&dir)?;
    // One case carries a self-contradicting finding and a summary-count
    // mismatch, so the contradiction rate and per-code counts are non-trivial.
    let clean = finding("reachable_unrevealed", 10, "candidate_current");
    let mut exposed = clean.clone();
    exposed["classification"] = json!("exposed");
    let contradicted = json!({
        "summary": {"findings": 3},
        "findings": [clean, exposed],
    });
    let checks: Vec<(String, Value)> = corpus
        .cases
        .iter()
        .enumerate()
        .map(|(index, case)| {
            let check = if index == 0 {
                contradicted.clone()
            } else {
                json!({"findings": []})
            };
            (case.case_id.clone(), check)
        })
        .collect();
    let report = build_report(&corpus, &checks, &BTreeMap::new())?;
    assert!(
        report.contradiction_rate.denominator > 0 && !report.contradictions_by_code.is_empty(),
        "fixture must exercise the contradiction counts: {:?}",
        report.contradictions_by_code
    );
    bless(&dir.join("expected"), &report)?;
    let derived = expected_report(&dir)?;
    assert_eq!(
        render_report_json(&derived)?,
        render_report_json(&report)?,
        "the summary rebuilt from the row files must equal the run's"
    );
    // A row that drops its counts no longer parses, so a stale row cannot
    // silently zero the contradiction rate.
    let row_path = dir.join("expected/rows/a-case.json");
    let mut row = parse_json(&row_path)?;
    if let Some(object) = row.as_object_mut() {
        object.remove("findings_scored");
    }
    crate::tests::write(
        &row_path,
        &serde_json::to_string_pretty(&row).map_err(|err| err.to_string())?,
    );
    let err = expected_report(&dir).err().unwrap_or_default();
    assert!(err.contains("findings_scored"), "{err}");

    // A case without its row fails instead of shrinking the denominators.
    bless(&dir.join("expected"), &report)?;
    fs::remove_file(dir.join("expected/rows/b-case.json")).map_err(|err| err.to_string())?;
    let err = expected_report(&dir).err().unwrap_or_default();
    assert!(
        err.contains("missing rows") && err.contains("b-case"),
        "{err}"
    );
    Ok(())
}

fn typescript_corpus_dir() -> PathBuf {
    crate::dogfood::repo_rooted_fixture_path("fixtures/typescript-verdict-corpus")
}

#[test]
fn committed_typescript_corpus_is_valid_and_its_rows_agree_with_its_labels() -> Result<(), String> {
    let dir = typescript_corpus_dir();
    let corpus = validated_corpus(&dir)?;
    assert!(
        corpus.cases.len() >= 40,
        "typescript corpus shrank to {}",
        corpus.cases.len()
    );
    assert!(
        corpus
            .subjects
            .iter()
            .all(|s| s.subject_id.starts_with("authored-ts-")),
        "typescript subjects are authored-ts-<lib>-<name>"
    );
    for library in ["jest", "vitest", "mocha", "nodetest"] {
        assert!(
            corpus
                .subjects
                .iter()
                .any(|s| s.subject_id.starts_with(&format!("authored-ts-{library}-"))),
            "no {library} subject"
        );
    }
    for truth in [TruthState::Discriminated, TruthState::NotDiscriminated] {
        assert!(
            corpus.cases.iter().any(|case| case.truth.state == truth),
            "typescript corpus has no {} case",
            truth.as_str()
        );
    }
    let rows_dir = dir.join("expected").join(ROWS_DIR);
    assert_eq!(files_under(&rows_dir)?.len(), corpus.cases.len());
    for case in &corpus.cases {
        let row: Value =
            serde_json::from_str(&read(&rows_dir.join(format!("{}.json", case.case_id)))?)
                .map_err(|err| err.to_string())?;
        assert_eq!(row["truth"], json!(case.truth.state.as_str()));
        let observed: Verdict =
            serde_json::from_value(row["observed_verdict"].clone()).map_err(|e| e.to_string())?;
        assert_eq!(
            row["outcome"],
            json!(score(case.truth.state, observed).as_str()),
            "{}",
            case.case_id
        );
    }
    Ok(())
}

#[test]
fn language_names_its_own_corpus_directory_and_nothing_else() -> Result<(), String> {
    let fixtures = crate::dogfood::repo_rooted_fixture_path("fixtures");
    assert_eq!(
        language_corpus_dir_in(&fixtures, "typescript")?,
        fixtures.join("typescript-verdict-corpus")
    );
    assert_eq!(
        language_corpus_dir_in(&fixtures, "rust")?,
        fixtures.join("rust-verdict-corpus")
    );
    for bad in ["..", "", "type/script"] {
        let err = language_corpus_dir_in(&fixtures, bad)
            .err()
            .ok_or(format!("`{bad}` was accepted"))?;
        assert!(err.contains("not a usable language name"), "{bad}: {err}");
    }
    let err = language_corpus_dir_in(&fixtures, "cobol")
        .err()
        .ok_or("a language with no corpus was accepted")?;
    assert!(err.contains("names no corpus"), "{err}");
    Ok(())
}

#[test]
fn only_a_rust_corpus_holds_labels_to_cargo_commands_and_rust_test_names() -> Result<(), String> {
    for (name, rust) in [
        ("rust-verdict-corpus", true),
        ("copied-corpus", true),
        ("typescript-verdict-corpus", false),
    ] {
        assert_eq!(
            is_rust_corpus(&Path::new("fixtures").join(name)),
            rust,
            "{name}"
        );
    }
    let dir = typescript_corpus_dir();
    let raw = corpus_value(&dir)?;
    let i = raw["cases"]
        .as_array()
        .and_then(|cases| {
            cases
                .iter()
                .position(|case| case["truth"]["mutants"][0]["outcome"] == json!("tests_failed"))
        })
        .ok_or("no typescript case with a failing first mutant")?;
    let validate_with = |title: &str| -> Result<Vec<String>, String> {
        let mut raw = raw.clone();
        raw["cases"][i]["truth"]["mutants"][0]["failing_test"] = json!(title);
        let corpus: Corpus = serde_json::from_value(raw).map_err(|err| err.to_string())?;
        Ok(validate(&corpus, &dir))
    };
    // A jest title has spaces and may have commas; its npx command is
    // recorded, not replayed.
    assert_eq!(
        validate_with("rounds 1, 2 and 3 cents down")?,
        Vec::<String>::new()
    );
    for bad in ["two\nlines", " padded", ""] {
        assert!(
            validate_with(bad)?
                .iter()
                .any(|v| v.contains("is not one test name")),
            "{bad:?}"
        );
    }
    // The same title on a Rust corpus is not one Rust test name.
    let rust = tampered(|raw| {
        raw["cases"][0]["truth"]["mutants"][0]["failing_test"] = json!("rounds 1, 2 and 3");
    })?;
    assert!(
        rust.iter().any(|v| v.contains("is not one test name")),
        "{rust:#?}"
    );
    Ok(())
}
