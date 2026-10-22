<!-- section: Internal / CI -->
- Required Rust gates fetch the JSON owner-return fixture's locked dependencies before policy and tests, alongside the existing fallible fixture preparation, so workspace dependency updates cannot leave its pinned offline authority unavailable (#9069).
