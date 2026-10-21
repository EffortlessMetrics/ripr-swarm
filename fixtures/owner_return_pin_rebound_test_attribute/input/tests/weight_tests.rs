use owner_pin_control::weight;
use skipper::serial as test;
#[test]
fn checks_weight() { assert_eq!(weight(4), 12); }
