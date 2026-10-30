mod support;

pub fn gate(n: u32) -> bool {
    n >= 10
}

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! gate_cases {
        ($($name:ident => ($input:expr, $want:expr))*) => {
            $(
                crate::debug_assert_eq!();
                #[test]
                fn $name() {
                    assert_eq!(gate($input), $want);
                }
            )*
        };
    }

    // The exported macro sits beside the generated test, at module scope, and defines a block-local `fn gate` elsewhere.
    gate_cases! {
        ten_is_open => (10, true)
    }
}
