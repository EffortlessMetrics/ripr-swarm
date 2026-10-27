#[macro_export]
macro_rules! αassert_eq {
    () => {
        fn gate(_: u32) -> bool {
            true
        }
    };
}
