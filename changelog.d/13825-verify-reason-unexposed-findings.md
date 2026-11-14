<!-- section: Fixed -->
- The `verify=refused` record in `ripr check --format agent` now names `reason=unexposed_findings_remain` instead of `gaps_remain`, so a report whose header says `gaps=0` (only static limits remain) no longer ends with a reason that claims gaps remain. When verification is refused is unchanged (#13825).
