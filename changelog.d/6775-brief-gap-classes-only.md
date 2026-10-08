<!-- section: Fixed -->
- `ripr agent brief` no longer selects static-limitation seams (opaque and
  the four `*_unknown` classes) as repair targets: the class names evidence
  the analyzer could not establish, so naming it a repair target sent agents
  to fix a gap that may not exist. The brief now selects gap classes only
  (`weakly_gripped`, `ungripped`, `reachable_unrevealed`), matching pilot's
  ranking population. Omitted seams are named in `warnings`, never silently
  dropped, and review comments inherit the same selection through the shared
  brief authority. Explicitly requested packets and cards still render
  `inspect_static_limitation` packets for static-limitation seams, and LSP
  seam diagnostics keep their `INFORMATION` severity there (#6775).
