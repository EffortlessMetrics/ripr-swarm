<!-- section: Internal / CI -->
- `cargo xtask dogfood` passes on main again: the generated-pr-ci-review-workflow scenario checked the CI summary for the Start here and Gate authority bullets that #11585 removed on purpose, so it now checks the current Start here text and the `ripr gate evaluate` pass/fail authority boundary (#12261).
