<!-- section: Fixed -->
- The remaining commands that read a ripr JSON artifact by path accept the
  encodings Windows PowerShell 5.1 writes (UTF-16 or UTF-8 with a
  byte-order mark): `check --gap-ledger`, `review-comments --gap-ledger`,
  `rerun`, `swarm queue` and `swarm ingest`, `agent packet`, `agent verify`
  and `agent receipt`, `receipt check`, `plus`, `outcome`, `annotations`,
  `feedback export`, `assistant-loop health`, `evidence-health
  --mutation-calibration`, `pr-comments plan`, `pr-review front-panel` and
  the other report inputs. `rerun`'s ledger hash and the agent receipt's
  verify hash still cover the file as written (#15936).
