<!-- section: Added -->
- LSP seam-inventory degradation is now forced through a production refresh
  by a regression test: an invalid `RIPR_REPO_EXPOSURE_SEAM_LIMIT` planted
  in a spawned `ripr lsp --stdio` server's environment must surface as the
  `seam_inventory`/`failed`/`seam_inventory_failed` component outcome with
  a `limited` run, exactly one WARNING naming the component and its
  recovery, and the seeded diff finding still published
  (RIPR-SPEC-0141, #7147). The retention assertions judge only the final
  diagnostics publish for the exact fixture URI, with oracle-integrity
  controls for a later clearing publish and a foreign-URI publish, so a
  finding the editor no longer displays cannot pass (#7189 review). Fixture
  git runs through the shared deadline-bounded helper. No behavior change.
