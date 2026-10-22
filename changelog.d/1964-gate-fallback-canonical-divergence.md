<!-- section: Fixed -->
- `gate evaluate --mode baseline-check` no longer treats a gap as reviewed
  baseline debt when only the legacy `path:line:class` fallback matches a
  baseline entry whose canonical gap id differs from the gap's. The gate now
  shares the divergence rule with `ripr baseline diff`, so both call such a gap
  new. Baseline entries without a canonical id keep the disclosed legacy
  fallback match (#1964).
