mod unrelated {
    macro_rules! vec {
        ($($tokens:tt)*) => {};
    }
}

use demo::gate;

#[test]
fn pins() {
    let _ = vec![0u8];
    assert_eq!(gate(10), true);
}
