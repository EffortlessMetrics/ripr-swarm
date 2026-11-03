use repo_exposure_identical_inline_arms_subject::{tier_differing, tier_identical};

#[test]
fn identical_arms_pinned_at_threshold() {
    assert_eq!(tier_identical(5), 1);
}

#[test]
fn differing_arms_pinned_at_threshold() {
    assert_eq!(tier_differing(5), 1);
}
