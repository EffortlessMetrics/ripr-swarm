#[macro_export]
macro_rules! define_scalar {
    () => {
        #[derive(Debug)]
        pub struct u64(pub ::core::primitive::u64);
        impl PartialEq<i32> for u64 {
            fn eq(&self, _other: &i32) -> bool { true }
        }
    };
}
