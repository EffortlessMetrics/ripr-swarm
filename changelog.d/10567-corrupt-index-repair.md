<!-- section: Fixed -->
- A truncated or corrupt `.git/index` refusal now ends with a backup-first repair bound to the selected root: copy the index aside, remove it and run `git -C '<root>' reset`. An unreadable but valid index keeps Git's reason with no delete advice (#10567).
