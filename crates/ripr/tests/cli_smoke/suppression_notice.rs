//! All-suppressed GitHub output must retain its nonempty analysis denominator.
use super::{assert_success, sample_output};
use std::process::Output;

fn outputs() -> Result<(Output, Output), Box<dyn std::error::Error>> {
    let fixture = sample_output::Fixture::new()?;
    let policy = fixture.suppression_policy(
        "schema_version = 1\n\n[[suppressions]]\nkind = \"exposure_gap\"\npath = \"crates/ripr/examples/sample/**\"\nreason = \"repro: suppress all sample findings\"\nowner = \"qa\"\n",
    )?;
    let unfiltered = fixture.run(&["--format", "github"])?;
    let filtered = fixture.run(&[
        "--format",
        "github",
        "--suppression-policy",
        policy.to_str().ok_or("policy path is not UTF-8")?,
    ])?;
    Ok((unfiltered, filtered))
}

fn assert_notice(unfiltered: &Output, filtered: &Output) {
    assert_success(unfiltered);
    let unfiltered_stdout = String::from_utf8_lossy(&unfiltered.stdout);
    let unfiltered_annotations = unfiltered_stdout
        .lines()
        .filter(|line| line.contains("file="))
        .count();
    assert!(
        unfiltered_annotations > 0,
        "unfiltered github output must annotate findings: {unfiltered_stdout}"
    );
    assert_eq!(unfiltered_annotations, 4, "owned sample finding count");
    assert!(
        !unfiltered_stdout.contains("suppressed by policy"),
        "unfiltered run must not mention policy: {unfiltered_stdout}"
    );

    assert_success(filtered);
    let stdout = String::from_utf8_lossy(&filtered.stdout);
    assert!(
        !stdout.is_empty(),
        "all-suppressed github output must not be silent"
    );
    let expected = format!(
        "::notice title=ripr::Annotated 0 of {unfiltered_annotations} static exposure finding(s); {unfiltered_annotations} suppressed by policy"
    );
    assert!(
        stdout.contains(&expected),
        "all-suppressed github output must name the denominator ({expected}): {stdout}"
    );
    assert!(
        !stdout.contains("file="),
        "suppressed findings must stay unannotated: {stdout}"
    );
    assert!(
        !stdout.contains("No static exposure findings found"),
        "all-suppressed is not an empty run: {stdout}"
    );
}

pub(super) fn run() -> sample_output::TestResult {
    // #4393: retain two actual CLI runs and every original output/exit pin.
    let (unfiltered, filtered) = outputs()?;
    assert_notice(&unfiltered, &filtered);
    Ok(())
}

#[test]
fn retained_oracle_rejects_silence_wrong_counts_and_leaked_annotations() -> sample_output::TestResult
{
    // Mutate actual rendered bytes only after the positive public path passes.
    // These assertion controls never replace the two smoke-test CLI processes.
    let (unfiltered, filtered) = outputs()?;
    assert_notice(&unfiltered, &filtered);
    let rejected = |unfiltered: &Output, filtered: &Output| {
        assert!(
            std::panic::catch_unwind(|| assert_notice(unfiltered, filtered)).is_err(),
            "a seeded wrong output passed the retained oracle"
        );
    };
    let mut empty_baseline = unfiltered.clone();
    empty_baseline.stdout.clear();
    rejected(&empty_baseline, &filtered);

    let mut false_policy = unfiltered.clone();
    false_policy
        .stdout
        .extend_from_slice(b"suppressed by policy");
    rejected(&false_policy, &filtered);

    let text = String::from_utf8_lossy(&filtered.stdout);
    for (from, to) in [
        ("Annotated 0 of 4", "Annotated 0 of 0"),
        ("4 suppressed by policy", "0 suppressed by policy"),
    ] {
        assert!(text.contains(from), "mutation must change actual output");
        let mut wrong = filtered.clone();
        wrong.stdout = text.replace(from, to).into_bytes();
        assert_ne!(wrong.stdout, filtered.stdout);
        rejected(&unfiltered, &wrong);
    }
    for suffix in [
        "::warning file=src/lib.rs,line=1::leaked finding\n",
        "::notice title=ripr::No static exposure findings found\n",
    ] {
        let mut wrong = filtered.clone();
        wrong.stdout.extend_from_slice(suffix.as_bytes());
        rejected(&unfiltered, &wrong);
    }
    let mut silent = filtered.clone();
    silent.stdout.clear();
    rejected(&unfiltered, &silent);
    let refusal = sample_output::Fixture::new()?.run(&["--format", "not-a-format"])?;
    assert!(
        !refusal.status.success(),
        "control must supply a real refusal"
    );
    let mut failed_baseline = unfiltered.clone();
    failed_baseline.status = refusal.status;
    rejected(&failed_baseline, &filtered);
    let mut failed_filtered = filtered.clone();
    failed_filtered.status = refusal.status;
    rejected(&unfiltered, &failed_filtered);
    Ok(())
}
