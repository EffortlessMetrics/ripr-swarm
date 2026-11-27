<!-- section: Docs -->
- `docs/ci/ripr-gate-acknowledgement.md` documents the downstream
  acknowledgement (waiver) contract for `ripr gate evaluate --mode
  acknowledgeable`, and `ripr help --all` now lists that mode,
  `--labels-json` and the default `ripr-waive` label (#1420).
- ADR 0021 and RIPR-SPEC-0135 keep static evidence movement, command
  availability, receipt issuance and external runtime mutation results on
  separate assurance axes, and add the design-only
  `schemas/ripr/repair-assurance.schema.json` with the typed `CommandSpecV1`
  shape. No output gains a stronger assurance state from this decision
  (#2207).
- `docs/README.md` is now a newcomer-first landing page that starts with
  the Quickstart and routes each goal to the doc that covers it, and the
  root README links it as the documentation entry point (#2512, #2427).
- `docs/COMMAND_HIERARCHY.md` now explains that the seam ID selects the gap
  while the repair-attempt ID selects its prepared attempt, and documents
  `ripr agent repair --seam-id ... --phase after` as a compatibility form
  that needs exactly one waiting attempt for that seam (#3689).
- Docs: the README, the crates.io and Marketplace READMEs, QUICKSTART,
  the command hierarchy and the agent, editor and PR/CI guides were brought
  up to date for the 0.11 commands and settings, and a new
  `docs/interop/mcp.md` documents `ripr mcp --stdio` (#3889).
- The README and Quickstart first-run examples now use a bare `ripr check`,
  which resolves the repository's own default branch, instead of
  `ripr check --base origin/main`, which fails in a clone whose default
  branch is not `main` (#3941, #3939).
- ADR 0023 records that the consumer evidence state (`actionable`,
  `already_observed`, `internal_only`, `static_limitation`, `unknown`) is a
  typed `EvidenceState`, separate from the producer analysis outcome. Only
  `actionable` grants an actionable state and `unknown` is never a clean
  result. Existing `gap_state` wire values are unchanged; renaming the field
  requires a versioned schema migration (#4189, #2955).
- Documentation and help now match shipped behavior: the crates.io README
  states the default-base order (`origin/HEAD`, `origin/main`,
  `origin/master`, `main`, `master`); the README, Quickstart and VS Code
  README say unsaved buffer content is never analyzed instead of implying a
  setting; `docs/README.md` links the exit-code, repair-attempt,
  server-provisioning, static-limits, install-channels and
  blocking-readiness pages; and the `ripr+` badge warning and `--format`
  help say no public ripr command writes test-efficiency JSON yet (#7412).
- `docs/QUICKSTART.md` now explains that the macOS release archives are
  not signed or notarized: a browser, AirDrop or mail download is
  quarantined and Gatekeeper blocks the extracted `ripr`. After the
  `.sha256` checksum matches, `xattr -d com.apple.quarantine ./ripr` clears
  it. The README points to that note (#8416).
- The README now leads with what ripr does, a short install and
  `ripr check` block with what to read, and the surfaces table. Install
  timings, the first-run replay and the agent repair-attempt details move to
  `docs/QUICKSTART.md` (#11189).
