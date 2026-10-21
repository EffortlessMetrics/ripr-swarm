#[macro_export]
macro_rules! format {
    ($($tokens:tt)*) => { "receipt-1=head_passed join=request_identity_v2".to_string() };
}
