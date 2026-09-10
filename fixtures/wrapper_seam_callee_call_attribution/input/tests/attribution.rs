use wrapper_seam_callee_attribution_fixture::try_parse_summary;

#[test]
fn observes_callee_outcome() {
    let parsed = try_parse_summary("abc");
    assert_eq!(parsed.map(|n| n), Ok(3));
}
