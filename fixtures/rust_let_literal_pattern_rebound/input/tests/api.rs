use rust_let_literal_pattern_rebound::gate;

#[test]
fn rebound_binding_is_not_the_boundary_input() {
    let n = 10;
    let (n, _m) = (9, 1);
    assert_eq!(gate(n), 0);
}
