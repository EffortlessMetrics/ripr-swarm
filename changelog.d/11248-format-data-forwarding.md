<!-- section: Fixed -->
- An exact assertion on a function that hands a std string or scalar helper's value through `format!("{}")`, `to_string`, `clone` or `+ "lit"` now credits the helper's change instead of abstaining as `helper_result_not_forwarded`; a precision or `{:?}` format spec, a user return type and a value built by a macro still abstain (#11248).
