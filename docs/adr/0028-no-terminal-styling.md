# ADR 0028: ripr emits no terminal styling; color environment variables are inert

- Status: Accepted
- Date: 2026-11-11
- Related: #7441, #7442, #4971

## Context

`ripr` writes plain text on every route. Nothing under `crates/ripr/src` emits
an ANSI escape sequence; the only escape-adjacent code removes control
characters from text that came from the analyzed repository
(`crates/ripr/src/terminal_text.rs`, `crates/ripr/src/stderr_guard.rs`). The
package has no styling dependency, never asks whether a stream is a terminal,
and never reads `NO_COLOR`, `CLICOLOR`, `CLICOLOR_FORCE`, `FORCE_COLOR`, `TERM`
or `COLORTERM`.

That behavior was pinned for one route only: `ripr help --json` must be
byte-identical under those variables
(`help_json_is_deterministic_across_roots_env_and_side_effect_free` in
`crates/ripr/tests/cli_smoke.rs`). It was not written down as a decision, so
#7441 asked whether the flat output is deliberate. Tools such as `cargo`,
`rustc`, `ripgrep` and `gh` style terminal output and honor `NO_COLOR`, so a
reader can take the absence of styling for an unfinished feature.

## Decision

`ripr` emits no terminal styling, by contract:

- No ANSI color, weight, underline or cursor-control sequence is written to
  stdout or stderr by any command or output format, on a terminal or off one.
- `NO_COLOR`, `CLICOLOR`, `CLICOLOR_FORCE`, `FORCE_COLOR`, `TERM` and
  `COLORTERM` are intentionally inert. `NO_COLOR` is satisfied trivially. The
  force variables do not turn styling on, because there is none to turn on.
- Emphasis in human output is carried by text: headings, labels, indentation
  and the fixed vocabulary of exposure classes. The `✓` marks in `ripr doctor`
  are text, not styling.

`crates/ripr/tests/terminal_styling_inert.rs` pins this for human routes
(`ripr --help`, `ripr help --all`, and `ripr check` human output on the bundled
sample): each must produce the same stdout bytes with and without those
variables, and neither stream may contain an escape byte.

## Consequences

- Human output is the same bytes in a terminal, a pipe, a CI log, a golden and
  an agent's captured transcript. Goldens under `fixtures/*/expected/` and PR
  comments need no stripping step.
- On an interactive terminal a blocking gap, an advisory note and a suggested
  command have the same visual weight. Report readability has to be improved
  through structure and wording, not color.
- Setting `FORCE_COLOR` or `CLICOLOR_FORCE` has no effect. This is documented
  behavior, not a defect.
- The decision is reversible. If styling is added later, it needs a new ADR
  that supersedes this one and keeps these limits:
  - styling only when the destination stream is a terminal, decided per
    stream;
  - `NO_COLOR` (any non-empty value) and `CLICOLOR=0` turn it off;
  - output to a pipe or file stays byte-identical to today's, so the existing
    goldens and both pins keep passing unchanged;
  - machine formats (`json`, `sarif`, `github`, badge formats, `help --json`)
    are never styled, including under a force variable;
  - the meaning of a line never depends on its color.

## Alternatives Considered

- **Add terminal-gated styling now, honoring `NO_COLOR` and `CLICOLOR`.** Not
  chosen. It adds a terminal-detection branch and a styling dependency to
  every human renderer, and creates a second rendering of each report that the
  goldens do not cover. No user report on #7441 asks for it; the issue records
  maintainer appetite as unknown.
- **Honor `FORCE_COLOR`/`CLICOLOR_FORCE` only.** Rejected: forced styling in a
  pipe breaks the byte-identical contract in exactly the captured-output case
  that agents and CI depend on.
- **Leave the behavior unrecorded.** Rejected: the pin on `help --json` showed
  intent for one route but said nothing about human output, so the question
  would be raised again.
