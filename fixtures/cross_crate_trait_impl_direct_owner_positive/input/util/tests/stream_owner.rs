use rcore::Stream;
use rutil::stream::take::Take;

#[test]
fn size_hint_caps_at_remaining() {
    assert_eq!(Take::new(5).size_hint(Some(9)), Some(5));
    assert_eq!(Take::new(5).size_hint(Some(2)), Some(2));
    assert_eq!(Take::new(5).size_hint(None), Some(5));
}
