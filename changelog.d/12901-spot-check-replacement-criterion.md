<!-- section: Internal / CI -->
- The mutation spot-check receipt now carries a derived `replacement_criterion` row: a stated, per-criterion (`met`, `not_met`, `not_established`) answer to whether ripr usably replaces cargo-mutants, regenerated offline by `--rederive-summaries`. Today's verdict is `not_yet` on placeholder thresholds (#12901).
