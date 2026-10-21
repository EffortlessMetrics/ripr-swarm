<!-- section: Changed -->
- `ripr check` classification no longer re-parses every indexed file for each
  probe to decide whether a returned type's `Drop` impl is plain. The
  workspace's non-plain `Drop` impls are read once per run, which removes
  about a third of classification worker time on a 245-probe diff. Findings
  are unchanged (#9209).
