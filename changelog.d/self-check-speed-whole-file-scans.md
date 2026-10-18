<!-- section: Changed -->
- `ripr check` no longer rescans whole files once per assertion or per changed
  owner: the per-file declaration table, the reach-closure seed scan and
  whole-file comment/string masking are each computed once and reused. On
  ripr's own 1,650-line analyzer diff a cold debug run drops from 22.2 to
  13.6 minutes with byte-identical output (#8629).
