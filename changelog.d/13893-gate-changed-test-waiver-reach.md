<!-- section: Fixed -->
- `ripr gate evaluate` no longer keeps every gap advisory because some test in the pull request changed. A changed test keeps a gap advisory only when it reaches that gap's owner (direct call, helper call or verified re-export chain); review-comments cards now name those tests in `changed_reaching_tests` (#13893).
