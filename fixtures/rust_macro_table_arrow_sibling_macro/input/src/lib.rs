#[macro_use]
mod shadow;

pub fn gate(n: u32) -> bool {
    n >= 10
}

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! gate_cases {
        ($($name:ident => ($input:expr, $want:expr))*) => {
            $(
                #[test]
                fn $name() {
                    crate::debug_assert_eq!();
                    assert_eq!(gate($input), $want);
                }
            )*
        };
    }

    // `debug_assert_eq!` (an exported crate macro) expands to a block-local `fn gate` defined in another module.
    gate_cases! {
        ten_is_open => (10, true)
    }
}
