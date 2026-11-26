<!-- section: Changed -->
- `ripr agent stub` for a changed match arm now writes the input the arm's own pattern names, such as `ev = Event::Fail` or `n = 2`, instead of a `todo!()` for that parameter, when no guard or earlier arm could take the value first. When the arm also depends on receiver state, the receiver line names the state to set (#8361).
