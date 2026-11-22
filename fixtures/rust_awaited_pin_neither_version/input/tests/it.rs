use rust_awaited_pin_neither_version_fixture::fee;

#[tokio::test]
async fn fee_matches_ninety_nine() {
    assert!(matches!(fee(4).await, Ok(99)));
}

#[tokio::test]
async fn fee_equals_ninety_nine() {
    assert_eq!(fee(4).await, Ok(99));
}
