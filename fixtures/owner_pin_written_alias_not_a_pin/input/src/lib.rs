pub fn weight(x: u32) -> u32 {
    x * 3
}

#[cfg(test)]
mod tests {
    use super::weight;

    struct S {
        _a: u8,
    }

    static mut EXPECTED: u32 = 0;

    // A tuple `ref mut` alias of the expected static: the owner result is
    // written through the alias, so the assertion is self-derived.
    #[test]
    fn tuple_ref_mut_alias_is_not_independent() {
        unsafe {
            match (EXPECTED, 0u8) {
                (ref mut r, _) => {
                    *r = weight(4);
                }
            }
            assert_eq!(weight(4), EXPECTED);
        }
    }

    // A brace-bearing element after the name must not hide the arms.
    #[test]
    fn braced_tuple_element_ref_mut_alias_is_not_independent() {
        unsafe {
            match (EXPECTED, S { _a: 1 }) {
                (ref mut r, _) => {
                    *r = weight(4);
                }
            }
            assert_eq!(weight(4), EXPECTED);
        }
    }
}
