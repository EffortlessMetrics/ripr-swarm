mod helper {
    pub fn pick(n: u32) -> u32 {
        n + 1
    }
}

#[test]
fn picks_two() {
    assert_eq!(crate::helper::pick(1), 2);
}

#[test]
fn checks_union_result() {
    assert_eq!({ union OtherItem { sibling_slot: u32 } 2 }, 2);
}

#[test]
fn checks_macro_result() {
    assert_eq!({ macro_rules! other_item { () => { 3 } } other_item!() }, 3);
}

const SEED_BASE: u32 = 1;

#[test]
fn seeds_base() {
    assert_eq!(SEED_BASE + 1, 2);
}
