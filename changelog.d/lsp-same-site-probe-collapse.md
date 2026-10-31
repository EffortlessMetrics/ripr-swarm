<!-- section: Fixed -->
- LSP: two probe families on one changed expression (for example a predicate
  and a return-value probe on `return a >= b;`) no longer publish two
  overlapping diagnostics when their findings carry no canonical gap and an
  identical repair signal. One diagnostic is published and every probe stays
  listed under `data.raw_findings` (`group_reason: same_site_and_repair_signal`).
  Findings with a canonical gap, a different expression, line or column, or any
  differing repair field are never merged, and actionability is unchanged (#11453).
