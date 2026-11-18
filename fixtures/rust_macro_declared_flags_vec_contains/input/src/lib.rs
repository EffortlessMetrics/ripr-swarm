pub mod traits;
pub use traits::Flags;

#[macro_export]
macro_rules! test_flags {
    () => {
        #[derive(Clone, Copy, PartialEq, Eq)]
        struct TestFlags {
            bits: u8,
        }
        impl TestFlags {
            const A: Self = Self { bits: 0b01 };
            const B: Self = Self { bits: 0b10 };
        }
        impl $crate::Flags for TestFlags {
            fn bits(&self) -> u8 {
                self.bits
            }
        }
    };
}

#[cfg(test)]
mod tests;
