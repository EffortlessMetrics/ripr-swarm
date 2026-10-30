<!-- section: Fixed -->
- Default-base failures and doctor distinguish genuinely unborn branches from damaged packed refs and corrupt or inaccessible HEAD refs. Damaged metadata retains Git's reason and metadata/access recovery instead of recommending a first commit; check refuses with exit 2 and doctor reports a failed diagnosis with exit 3 (#10566).
