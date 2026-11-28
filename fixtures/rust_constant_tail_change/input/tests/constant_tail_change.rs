use constant_tail_change::*;

#[test]
fn ready_is_pinned() {
    assert_eq!(ready(), false);
}

#[test]
fn idle_is_only_called() {
    let _ = idle();
}

#[test]
fn items_are_pinned() {
    assert_eq!(items(), Vec::<i32>::new());
}

#[test]
fn bytes_are_pinned() {
    assert_eq!(bytes(), vec![b'x', b'y']);
}

#[test]
fn enabled_agrees_where_the_name_holds_the_constant() {
    let flag = false;
    assert_eq!(enabled(flag), false);
}

#[test]
fn gate_agrees_on_the_early_return() {
    let flag = true;
    assert_eq!(gate(flag), false);
}

#[test]
fn pick_agrees_on_the_early_return() {
    assert_eq!(pick(0), Vec::<i32>::new());
}

#[test]
fn off_folds_equal() {
    assert_eq!(off(), false);
}

#[test]
fn neg_agrees_at_zero() {
    let d = 0;
    assert_eq!(neg(d), 0);
}

#[test]
fn both_agree_at_zero() {
    assert_eq!(both(0, 0), 0);
}

#[test]
fn px_agrees_at_zero() {
    assert_eq!(px(P { x: 0 }), 0);
}
