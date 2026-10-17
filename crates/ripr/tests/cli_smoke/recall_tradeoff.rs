//! Real-CLI recall warning oracles and wrong-output controls.
use super::{Output, sample_output};

const ACTIVE_NOTE: &str = "unchanged tests are excluded from the index";
const RECALL_EFFECT: &str = "Reach evidence cannot name tests the diff does not touch";

pub(super) fn check(with_flag: &Output, without_flag: &Output) -> Result<(), String> {
    for (name, output) in [("active flag", with_flag), ("default", without_flag)] {
        if !output.status.success() {
            return Err(format!("{name} check failed: {output:?}"));
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        if !stdout.contains("Summary: 4 probe(s)") {
            return Err(format!(
                "{name} must analyze the four sample probes; got:\n{stdout}"
            ));
        }
    }
    let stderr = String::from_utf8_lossy(&with_flag.stderr);
    if !stderr.contains(ACTIVE_NOTE) {
        return Err(format!(
            "the active flag must restate its tradeoff on stderr; got:\n{stderr}"
        ));
    }
    if !stderr.contains(RECALL_EFFECT) {
        return Err(format!(
            "the note must name the recall effect, not only the speedup; got:\n{stderr}"
        ));
    }
    let stderr = String::from_utf8_lossy(&without_flag.stderr);
    if stderr.contains(ACTIVE_NOTE) {
        return Err(format!(
            "the default run must not claim the flag is active; got:\n{stderr}"
        ));
    }
    Ok(())
}

#[test]
fn retained_recall_warning_oracles_reject_wrong_output() -> sample_output::TestResult {
    let fixture = sample_output::Fixture::new()?;
    let with_flag = fixture.run(&["--no-unchanged-tests"])?;
    let without_flag = fixture.run(&[])?;
    check(&with_flag, &without_flag)?;

    for missing in [ACTIVE_NOTE, RECALL_EFFECT] {
        let mut wrong = with_flag.clone();
        wrong.stderr = String::from_utf8_lossy(&wrong.stderr)
            .replace(missing, "")
            .into_bytes();
        assert!(
            check(&wrong, &without_flag).is_err(),
            "must reject missing {missing}"
        );
    }
    let mut wrong = without_flag.clone();
    wrong.stderr.extend_from_slice(ACTIVE_NOTE.as_bytes());
    assert!(check(&with_flag, &wrong).is_err());

    let mut wrong = with_flag.clone();
    wrong.stdout.clear();
    assert!(check(&wrong, &without_flag).is_err());
    let mut wrong = without_flag.clone();
    wrong.stdout.clear();
    assert!(check(&with_flag, &wrong).is_err());

    // Use an actual CLI refusal for a native-platform failure status.
    let refusal = fixture.run(&["--format", "not-a-format"])?;
    assert!(!refusal.status.success());
    let mut wrong = with_flag.clone();
    wrong.status = refusal.status;
    assert!(check(&wrong, &without_flag).is_err());
    let mut wrong = without_flag.clone();
    wrong.status = refusal.status;
    assert!(check(&with_flag, &wrong).is_err());
    Ok(())
}
