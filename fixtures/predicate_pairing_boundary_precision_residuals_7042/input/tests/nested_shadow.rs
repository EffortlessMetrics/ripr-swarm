use predicate_pairing_boundary_precision_residuals_7042::gate;

#[test]
fn nested() {
    let got = gate(10);
    {
        let got = false;
        got = true;
    }
    assert_eq!(got, true);
}
