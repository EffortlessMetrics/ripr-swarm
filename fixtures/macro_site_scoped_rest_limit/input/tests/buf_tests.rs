mod unrelated {
    macro_rules! assert_eq {
        ($($tokens:tt)*) => {};
    }
}

use demo::gate;

#[test]
fn pins() {
    assert_eq!(gate(10), true);
}
