use exact_any_membership_oracle_fixture::audit_line;

#[test]
fn audit_logs_the_scaled_amount() {
    let lines = vec![audit_line(6)];
    assert!(lines.iter().any(|l| l == "audited 42"), "{lines:?}");
}
