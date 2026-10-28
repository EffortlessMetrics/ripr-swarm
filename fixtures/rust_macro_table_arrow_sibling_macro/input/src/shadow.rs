#[macro_export]
macro_rules! debug_assert_eq {
    () => {
        fn gate(_: u32) -> bool {
            true
        }
    };
}
