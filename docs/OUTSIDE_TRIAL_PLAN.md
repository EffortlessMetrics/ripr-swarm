# Outside developer trial plan

Goal: three to five Rust developers who are not on this project run ripr on
their own code and tell us where it fails them. Everything so far was tested by
us on our repositories. This plan is a draft for the owner to act on; no
outreach has been sent.

## Who

Rust developers with an active workspace and a PR open or about to open, who
write tests: maintainers of mid-sized crates, people who already use
`cargo-mutants`, and one or two on a Windows or macOS machine, because most of
our own proof is Linux.

## What to ask them

Install ripr for the channel they would pick on their own, run it on a real
branch, and answer:

1. How did you install it and how long until `ripr check` printed something?
2. Was the first finding about code you actually changed?
3. Would you add the suggested test? If not, what was wrong or missing?
4. Did any "unknown" or refusal tell you what to do next?
5. Did it feel fast enough to run before every push?
6. Would you leave it in CI? What would make you remove it?

Ask for the command they ran, `ripr --version`, OS, and the output with source
redacted if needed. `ripr doctor` output is the preferred environment report.

## How to collect

One GitHub issue form titled "First run report" in the source repository with the
six questions plus version, OS and install channel as required fields, linked
from the invite. A form keeps answers comparable and countable. Each report
becomes a row in a trial scoreboard (install time, first-finding relevance,
would-keep-in-CI) with a trend, so progress is measured rather than recalled.

## Invite text (draft)

> I'm building ripr, a static analyzer that reads your diff and says whether the
> tests around it would notice if the changed behavior were wrong. It doesn't
> run mutants. Could you try it on a branch of your own project? It's
> `cargo install ripr` today (0.11 adds a prebuilt install). Fifteen minutes,
> six questions, and I'll fix what you hit.

## Needs the owner

- Pick the people and send the invites; this repository cannot contact anyone.
- Say when: invites are best after 0.11.0 assets exist so the install is the
  fast path, otherwise the first impression is an 11 minute compile.
- Approve creating the report issue form (a small follow-up PR, not drafted yet).
