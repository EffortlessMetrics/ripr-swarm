<!-- section: Fixed -->
- Release-candidate source admission judges the checkout's configured `remote.origin.url` instead of Git's `insteadOf`-rewritten URL, so neither an operator's nor the checkout's own URL rewrite can turn an undocumented transport or foreign origin into an approved one, and the result no longer varies with the operator's Git config (#8868).
