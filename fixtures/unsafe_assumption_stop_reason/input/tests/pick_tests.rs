use unsafe_assumption_stop_reason::first;

#[test]
fn first_reads_the_head() {
    assert_eq!(first(&[7, 8]), 7);
}
