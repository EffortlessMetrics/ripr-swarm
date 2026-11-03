<!-- section: Fixed -->
- A diff that deletes an entire source file whose removed lines are only Kani verification text now counts those lines toward the `verification_only_change` disclosure when the changed path has no new-side index entry but the diff still carries a contiguous `1..=N` removal hunk (#12013).
