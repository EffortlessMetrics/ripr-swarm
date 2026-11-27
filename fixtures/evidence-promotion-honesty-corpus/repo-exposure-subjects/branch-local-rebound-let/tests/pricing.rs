use branch_local_rebound_let::discounted_total;

#[test]
fn rebound_total_reads_the_second_call() {
    let got = discounted_total(100, 100);
    let got = discounted_total(50, 100);
    assert_eq!(got, 50);
}

#[test]
fn outer_total_is_read_past_a_closed_block() {
    let total = discounted_total(50, 100);
    {
        let total = discounted_total(100, 100);
        let _ = total;
    }
    assert_eq!(total, 50);
}
