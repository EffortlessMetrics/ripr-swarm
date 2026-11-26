# Report presentation

Use a compact primary report, usually about 400-800 words when that is enough
to preserve the evidence. Put long logs, fixture bytes and extended traces in
an appendix. Brevity must not remove prerequisites, uncertainty or an important
failed reproduction.

Start with the affected component, one-sentence impact and proposed severity.
Then use these sections:

- Affected subject: repository, commit, platform, enabled features and relevant
  tool versions; identify the entrypoint and source locations.
- Attacker prerequisites: what input the attacker controls, how the victim
  encounters it and what privileges or non-default options are assumed.
- Reproduction: minimal local fixture, literal command or message sequence,
  native exit/result, and observed versus expected behavior. Include test
  selection/execution counts when using a test harness.
- Root cause and impact: the failed boundary and the demonstrated consequence,
  separating observations from inferred extensions or unresolved questions.
- Fix and verification: proposed patch, regression test, before/after result,
  negative or near-miss controls, and any unexecuted platform or proof step.

Name the smallest evidence needed to reproduce the finding. Reference a
single retained log or fixture rather than repeating it in several sections.
Keep source identity, test execution, static-analysis output and impact as
separate statements. A hypothetical example should be labeled as such, not
presented as a successful reproduction.

When reports appear to share a root cause, state the relationship and distinct
reachable impacts clearly so maintainers can deduplicate without losing facts.
Use plain technical language rather than promotional certainty. This file
specifies report structure only; scope and severity guidance are in the
project's threat model.
