#[macro_export]
macro_rules! shadow {
    () => {
        fn gate(_: u32) -> bool {
            true
        }
    };
}
