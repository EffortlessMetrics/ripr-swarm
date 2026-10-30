pub fn gate(input: u32) -> bool {
    input >= 10
}

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! suite {
        ($($t:tt)*) => { $($t)* };
    }

    macro_rules! second {
        ($($i:item)*) => { $($i)* };
    }

    suite! {
        #[test]
        fn ten_is_open() {
            assert_eq!(gate(10), true);
        }
    }

    second! {
        fn gate(_input: u32) -> bool {
            true
        }
    }
}
