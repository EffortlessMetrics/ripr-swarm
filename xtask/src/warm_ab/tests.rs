use super::judge::{Limits, Verdict, judge, median};

#[test]
fn median_takes_the_middle_and_averages_an_even_pair() {
    assert_eq!(median(&[]), None);
    assert_eq!(median(&[7]), Some(7));
    assert_eq!(median(&[9, 1, 5]), Some(5));
    assert_eq!(median(&[4, 10, 2, 8]), Some(6));
}

#[test]
fn a_doubled_candidate_fails_and_an_equal_one_passes() {
    let limits = Limits::default();
    // The #10489 shape: about 31 s became about 64 s on the same tree.
    let slow = judge(
        &[31_000, 30_400, 31_900],
        &[63_800, 61_700, 64_000],
        &limits,
    );
    assert_eq!(slow.verdict, Verdict::Fail, "{}", slow.detail);
    assert!(slow.ratio.is_some_and(|ratio| ratio > 1.9));
    let same = judge(
        &[31_000, 30_400, 31_900],
        &[31_500, 30_900, 32_100],
        &limits,
    );
    assert_eq!(same.verdict, Verdict::Pass, "{}", same.detail);
}

#[test]
fn a_big_ratio_under_the_noise_floor_passes() {
    let judged = judge(&[400, 410, 390], &[1_000, 1_100, 900], &Limits::default());
    assert_eq!(judged.verdict, Verdict::Pass, "{}", judged.detail);
}

#[test]
fn the_ceiling_fails_even_when_the_baseline_is_as_slow() {
    let judged = judge(&[400_000], &[410_000], &Limits::default());
    assert_eq!(judged.verdict, Verdict::Fail);
    assert!(judged.detail.contains("ceiling"), "{}", judged.detail);
}

#[test]
fn a_side_without_samples_fails() {
    let judged = judge(&[], &[1_000], &Limits::default());
    assert_eq!(judged.verdict, Verdict::Fail);
    assert!(judged.ratio.is_none());
}

#[test]
fn pins_parse_into_ids_and_commits_and_reject_unsafe_text() {
    use super::nightly::parse_pins;
    assert_eq!(
        parse_pins("xtask-tests=a58975de,product_rust=af8136f7"),
        Ok(vec![
            ("xtask-tests".to_string(), "a58975de".to_string()),
            ("product_rust".to_string(), "af8136f7".to_string())
        ])
    );
    for bad in [
        "",
        "noequals",
        "id=",
        "=abc",
        "id=--upload-pack=x",
        "id=abc;rm",
        "a/b=abc",
        "dup=abc,dup=def",
    ] {
        assert!(parse_pins(bad).is_err(), "{bad:?} should be rejected");
    }
}
