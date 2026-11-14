//! Whole-journey agent byte budget (#13673): a scripted check -> explain ->
//! context -> verify CLI sequence, not an observed agent or billed-token result.
//!
//! `agent_output_budget` pins single outputs. This test pins the scripted
//! journey: cold and steady-state check bytes, one explain, one context
//! packet, and the final verify-bearing check, plus the exact duplicate
//! lines repeated across distinct steps. No test edit happens between the
//! initial and final checks, so the final `verify=` record may be
//! `complete` or `refused`. Bytes are UTF-8 stdout bytes with both project
//! path spellings replaced by `<root>`; the `bytes/4` column is a rough
//! reference estimate only — no tokenizer or provider billing is claimed.
//! Provider input/cached/output/reasoning tokens are host-recorded and
//! out of scope for this CLI test. This is a deterministic output-budget
//! instrument over a fixed fixture (five changed predicates yielding three
//! printed agent gap rows, one folded): it must not be cited as an observed agent token
//! saving or as a completed repair workflow.
use super::{printed_path, run_command, unique_external_workspace};
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

const BASE: &str = "pub fn can_vote(age: u32) -> bool { age > 18 }\n\
pub fn clamp(v: i32, lo: i32, hi: i32) -> i32 { if v < lo { lo } else if v > hi { hi } else { v } }\n\
pub fn discount(total: u32) -> u32 { if total > 100 { total - 10 } else { total } }\n\
pub fn parse(text: &str) -> Result<u32, String> { if text.len() > 5 { return Err(\"long\".into()); } text.parse::<u32>().map_err(|e| e.to_string()) }\n\
pub fn tier(n: u32) -> &'static str { if n >= 10 { \"gold\" } else { \"basic\" } }\n";

const TESTS: &str = "#[cfg(test)]\nmod tests {\n    use super::*;\n\
    #[test] fn vote() { assert!(can_vote(30)); }\n\
    #[test] fn clamp_ok() { assert_eq!(clamp(5, 0, 10), 5); }\n\
    #[test] fn disc() { assert!(discount(200) < 200); }\n\
    #[test] fn parses() { assert!(parse(\"12\").is_ok()); }\n\
    #[test] fn gold() { assert_eq!(tier(50), \"gold\"); }\n}\n";

fn workspace(label: &str) -> TestResult<std::path::PathBuf> {
    let project = unique_external_workspace(label)?;
    fs::create_dir_all(project.join("src"))?;
    fs::write(
        project.join("Cargo.toml"),
        "[package]\nname = \"fx\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )?;
    let changed = BASE
        .replace("age > 18", "age >= 18")
        .replace("v > hi", "v >= hi")
        .replace("total > 100", "total >= 100")
        .replace("len() > 5", "len() >= 5")
        .replace("n >= 10", "n > 10");
    fs::write(project.join("src/lib.rs"), format!("{changed}{TESTS}"))?;
    let mut diff = String::from("--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,5 +1,5 @@\n");
    for (old, new) in BASE.lines().zip(changed.lines()) {
        if old == new {
            diff.push_str(&format!(" {old}\n"));
        } else {
            diff.push_str(&format!("-{old}\n+{new}\n"));
        }
    }
    fs::write(project.join("change.diff"), diff)?;
    Ok(project)
}

fn normalize_root_text(text: &str, raw: &str, printed: &str) -> String {
    // Both spellings must fold: the raw canonical path (verbatim `\\?\`
    // on Windows) and ripr's printed form (verbatim fold + `/`
    // separators). Tested with fixed Windows-like spellings below so the
    // fold is discriminated on every host.
    let once = text.replace(raw, "<root>");
    if printed == raw {
        once
    } else {
        once.replace(printed, "<root>")
    }
}

fn normalize_root(text: &str, project: &Path) -> String {
    // `unique_external_workspace` canonicalizes, which yields a verbatim
    // `\\?\` spelling on Windows, while ripr prints the bound root folded
    // (`root_path_display`). Replace both spellings so the budget does not
    // depend on checkout path length.
    normalize_root_text(text, &project.to_string_lossy(), &printed_path(project))
}

fn run(project: &Path, args: &[&str]) -> TestResult<String> {
    let output = run_command(env!("CARGO_BIN_EXE_ripr"), Some(project), args)?;
    assert!(
        output.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(normalize_root(&String::from_utf8(output.stdout)?, project))
}

fn receipt(label: &str, text: &str, ceiling: usize) {
    eprintln!(
        "agent-journey-budget {label}: {} bytes (~{} ref-tokens at bytes/4, ceiling {ceiling})",
        text.len(),
        text.len() / 4
    );
    assert!(
        text.len() <= ceiling,
        "{label} grew to {} bytes, over its {ceiling}-byte journey budget:\n{text}",
        text.len()
    );
}

fn gap_ids(check: &str) -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    for line in check.lines() {
        if let Some(at) = line.find("|id=") {
            let id: String = line[at + 4..]
                .chars()
                .take_while(|c| !c.is_whitespace() && *c != '|')
                .collect();
            if !id.is_empty() {
                ids.insert(id);
            }
        }
    }
    ids
}

fn first_gap_id(check: &str) -> TestResult<String> {
    gap_ids(check)
        .into_iter()
        .next()
        .ok_or_else(|| format!("journey fixture must print a gap with |id=:\n{check}").into())
}

#[test]
fn agent_journey_stays_inside_byte_budget_and_keeps_verify_record() -> TestResult {
    let project = workspace("agent-journey-budget")?;
    let root = project.to_string_lossy().into_owned();

    // Cold: first run carries the one-time legend. Steady: budgeted rerun.
    let cold = run(
        &project,
        &[
            "check",
            "--quiet",
            "--diff",
            "change.diff",
            "--format",
            "agent",
        ],
    )?;
    let steady = run(
        &project,
        &[
            "check",
            "--quiet",
            "--diff",
            "change.diff",
            "--format",
            "agent",
        ],
    )?;
    let id = first_gap_id(&steady)?;
    let explained = run(
        &project,
        &[
            "explain",
            "--root",
            &root,
            "--diff",
            "change.diff",
            "--format",
            "agent",
            &id,
        ],
    )?;
    let context = run(
        &project,
        &[
            "context",
            "--root",
            &root,
            "--diff",
            "change.diff",
            "--at",
            &id,
        ],
    )?;
    // Final verify step reuses the same compact report (#13516).
    let verify = run(
        &project,
        &[
            "check",
            "--quiet",
            "--diff",
            "change.diff",
            "--format",
            "agent",
        ],
    )?;

    // Identity and completion authority survive every step.
    // The budget only means something if the fixture actually yields its
    // workload: fewer findings would shrink every output and pass
    // vacuously. Five changed predicates yield four findings here, printed
    // as three agent gap rows since #14453 folds identical rows
    // (`age >= 18` surfaces two probes that print identically; the
    // clamp/parse changes stay hidden on this compact surface, as in the
    // sibling budget). Pin the header counts and the distinct record IDs
    // together.
    for (label, check) in [("cold", &cold), ("steady", &steady), ("verify", &verify)] {
        let ids = gap_ids(check);
        assert!(
            check.contains(" gaps=3 folded_gaps=1 "),
            "{label} header must count three printed gaps with one folded:\n{check}"
        );
        assert!(
            ids.len() == 3,
            "journey fixture must yield three distinct gap IDs, {label} yielded {}",
            ids.len()
        );
    }
    assert!(
        explained.contains(&id),
        "explain lost finding {id}:\n{explained}"
    );
    assert!(
        context.contains(&id),
        "context lost finding {id}:\n{context}"
    );
    assert!(
        !verify.is_empty(),
        "final check produced no stdout; cannot look for the #13516 verify= record"
    );
    let last = verify.lines().last().unwrap_or_default();
    assert!(
        last.starts_with("verify=complete|") || last.starts_with("verify=refused|"),
        "final check must end with the #13516 verify= record, got: {last}"
    );

    // Ceilings sit just above the measured receipts on this fixture
    // (2026-11-14 on current main, three-row folded workload: cold 1186,
    // steady 1186, explain 543, context 4975, verify 1186; 2026-11-09
    // normalized four-gap receipts were 1336/1336/543/4974/1336 before
    // #14453 folded one identical row). Cold equals steady here because
    // the one-time legend is a human-format surface, not an agent-format
    // one; ceilings still hold.
    receipt("check cold", &cold, 1_400);
    receipt("check steady", &steady, 1_400);
    receipt("explain --format agent", &explained, 800);
    receipt("context packet", &context, 6_000);
    receipt("final verify check", &verify, 1_400);

    // Redundancy across distinct steps: exact lines that appear in more
    // than one step output. Each step is deduped first so repeats inside a
    // single output do not count as cross-step duplication; every extra
    // step that repeats a line adds its bytes again.
    let step_sets: Vec<BTreeSet<&str>> = [&steady, &explained, &context, &verify]
        .iter()
        .map(|text| text.lines().filter(|line| line.len() >= 24).collect())
        .collect();
    let mut seen = BTreeSet::new();
    let mut dup_bytes = 0usize;
    for set in &step_sets {
        for line in set {
            if !seen.insert(*line) {
                dup_bytes += line.len() + 1;
            }
        }
    }
    let total: usize = [&steady, &explained, &context, &verify]
        .iter()
        .map(|t| t.len())
        .sum();
    eprintln!(
        "agent-journey-budget journey steady-total: {total} bytes, exact-duplicate lines >=24B: {dup_bytes} bytes"
    );
    assert!(
        dup_bytes <= 3_000,
        "journey repeats {dup_bytes} duplicate bytes over its 3000-byte redundancy budget"
    );
    Ok(())
}

#[test]
fn journey_root_normalization_folds_both_windows_spellings() -> TestResult {
    // Fixed Windows-like spellings discriminate both replacements on every
    // host: on Windows `/tmp/...` would make `printed_path` equal the raw
    // path, so a test using only host paths stays green if either
    // replacement disappears. Removing either `replace` below must fail.
    let raw = r"\\?\C:\ws\proj";
    let printed = "C:/ws/proj";
    let fake = format!("cmd --root {raw} --diff {printed}/change.diff next {printed}");
    assert_eq!(
        normalize_root_text(&fake, raw, printed),
        "cmd --root <root> --diff <root>/change.diff next <root>",
        "both spellings must fold to the same token"
    );
    assert!(
        fake.replace(raw, "<root>").contains(printed),
        "fixture must carry the printed spelling or the second fold is vacuous"
    );
    assert!(
        fake.replace(printed, "<root>").contains(raw),
        "fixture must carry the raw spelling or the first fold is vacuous"
    );
    Ok(())
}

#[test]
fn journey_root_normalization_erases_checkout_path_length() -> TestResult {
    // The budget must not depend on how long the checkout path is: both the
    // raw canonical spelling and ripr's printed (folded, `/`-separator)
    // spelling normalize to the same token, while real output growth still
    // changes the measured length.
    for root in [
        Path::new("/tmp/ripr-a"),
        Path::new("/tmp/ripr-a-much-longer-checkout-name"),
    ] {
        let raw = root.to_string_lossy().into_owned();
        let printed = printed_path(root);
        let fake = format!("cmd --root {raw} --diff {printed}/change.diff");
        let normalized = normalize_root(&fake, root);
        assert!(
            !normalized.contains(&raw) || raw == "<root>",
            "raw root leaked into measured bytes for {root:?}: {normalized}"
        );
        assert!(
            !normalized.contains(&printed) || printed == "<root>",
            "printed root leaked into measured bytes for {root:?}: {normalized}"
        );
        assert_eq!(
            normalized, "cmd --root <root> --diff <root>/change.diff",
            "both spellings must fold to the same token for {root:?}"
        );
    }
    let root = Path::new("/tmp/ripr-a");
    let base = normalize_root("line one\nline two\n", root);
    let grown = normalize_root("line one\nline two\nline three\n", root);
    assert!(
        grown.len() > base.len(),
        "normalization must retain sensitivity to actual output growth"
    );
    Ok(())
}
