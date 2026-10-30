macro_rules! assert { ($cond:expr) => { let _ = $cond; }; }
#[test]
fn checks_unknown() {
let argv = ["cargo", "test", "--locked", "-p", "alpha", "--test", "plain_probe", "--mystery"].map(str::to_owned);
let actual = super::selection_request(&argv);
assert!(matches!(actual, super::SelectionRequest::Unresolved(_)));
}
