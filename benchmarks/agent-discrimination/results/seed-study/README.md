# Seed study records (Nov 2-4) — provenance and caveats

This directory holds the retained run records of the Nov 2-4 agent-efficiency
seed study: **5 of 9 solver runs** (3 scenarios × 3 variants). The 4
unretained records (the three `ripr-context` variant cells and
`silent-return-vanilla`) were not written before the harness was reworked and
are not recoverable; that recording loss is defect 4 of issue #13113 and
`record-run.sh` exists so future runs cannot lose rows. Do not average over
the missing cells. All 5 retained records scored `discriminates: yes` with
`rounds: 1`; the uplift finding and small-n caveats live in the benchmark
`README.md`.

## Sanitization (encode time)

The captures were recorded on the study host and originally contained that
host's machine-specific path prefixes. Repository policy forbids tracking
real machine paths (check-local-context: "real machine paths ... must be
removed, not allowlisted"), so at encode time the following replacements were
applied to `contexts/*/ripr-context.txt`, `contexts/*/tool-note.txt`, and the
`.md` records in this directory; nothing else was altered:

| original (study host)        | placeholder        |
|------------------------------|--------------------|
| the study harness root       | `<study-harness>`  |
| the study cargo bin dir      | `<cargo-bin>`      |
| the host temp root           | `<host-temp>`      |
| the host user profile        | `<host-profile>`   |

Byte-count semantics:

- `contexts/*/vanilla.txt` were **not modified** — their checked-in bytes are
  the exact study captures (1178 / 1099 / 1762 bytes), matching the
  `context_bytes` recorded in the vanilla-cell reports.
- `ripr-context.txt` and `tool-note.txt` changed size under sanitization;
  `scenarios/<scenario>/scenario.json` carries both the original capture byte
  counts and the checked-in byte counts.
- The `context_bytes:` values inside the `.md` records are study-time facts
  about the files the solver saw (for `ripr-tool` cells, the solver's saved
  ripr output, which is not checked in) and are not re-measured here.

The unsanitized originals remain on the study host with the rest of the
retained study harness; the checked-in copies are the citable record.
