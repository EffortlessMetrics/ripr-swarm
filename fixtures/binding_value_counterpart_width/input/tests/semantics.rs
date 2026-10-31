use counterpart_width::{signed, unsigned};

#[test]
fn unsigned_wrap_is_not_the_boundary() {
    assert!(unsigned(true, 128));
}

#[test]
fn signed_wrap_is_not_the_boundary() {
    assert!(signed(true, 64));
}
