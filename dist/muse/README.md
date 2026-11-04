# RIPR for Muse

Static mutation-exposure analysis as a Muse plugin. RIPR asks:
for the behavior changed in this diff, do the current tests appear to
contain a discriminator that would notice if that behavior were wrong?

This package is a thin native Muse plugin over RIPR's existing
interfaces. It adds no analyzer and no repair engine:

- MCP server: `ripr mcp --stdio` (read-only workspace status, refresh,
  gap evidence, repair cards; it never edits or executes).
- Skill: `ripr-repair-workflow`, the bounded workflow from static evidence to a
  focused test edit to separately executed proof.
- Commands: `/ripr-setup` (verify binary, server, and workspace wiring)
  and `/ripr-repair` (run one guided repair).

## Prerequisite

The `ripr` binary must be on `PATH` (`cargo install ripr`, or a pinned
internal build). The plugin never downloads or installs it.

## Evaluation

See `evals/README.md` for the compatibility / repair / effectiveness
contract, the pinned positive fixture, and the incorrect-repair
control. Static verdicts and executed-test verdicts are always reported
separately.

## Claim boundary

RIPR classes (`exposed`, `weakly_exposed`, `reachable_unrevealed`,
`no_static_path`, `infection_unknown`, `propagation_unknown`,
`static_unknown`) are static evidence. This plugin never presents them
as runtime mutation outcomes.
