<!-- section: Changed -->
- `ripr agent stub` now writes the string for a boundary on a `&str` parameter's length (`s.len() > 3`, `n <= MIN_LEN` after `let n = s.chars().count();`, or `a.len() > b.len()`) instead of leaving `todo!("ripr: choose `s`")`: a string of exactly the boundary length, such as `"xxx"` (#8361).
