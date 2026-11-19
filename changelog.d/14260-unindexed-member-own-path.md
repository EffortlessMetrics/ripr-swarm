<!-- section: Fixed -->
- Draft mode no longer refuses every assertion in the changed package
  because another workspace member, outside the draft index, declares one of
  its own files with `#[path = "..."]`. A member whose `#[path]` or
  `include!` could compile a file outside its own directory still withholds
  credit (#14260).
