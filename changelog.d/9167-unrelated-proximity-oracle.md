<!-- section: Fixed -->
- A same-file test that cannot reach the changed function no longer supplies an
  oracle while another test reaches it. An owner called only by an
  assertion-free test now reads `reachable_unrevealed` instead of
  `weakly_exposed` with a strong oracle borrowed from an unrelated function; the
  unrelated assertion stays listed as `no_call_path` (#9167).
