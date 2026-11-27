use assertion_inert_names_position::weight;

#[test]
fn in_scrutinee() {
    match {
        assert_eq!(weight(4), 12);
        true
    } {
        _ => {}
    }
}

#[test]
fn in_condition() {
    if {
        assert_eq!(weight(4), 12);
        true
    } {}
}
