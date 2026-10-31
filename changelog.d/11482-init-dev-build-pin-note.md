<!-- section: Fixed -->
- `ripr init --ci github` on an unreleased build no longer says "Nothing is needed now" while generating a workflow pinned to a published ripr that lacks `ripr reports ci-packet` and `ripr pr-comments summary`; the note now says the workflow's CI run fails until the new version is published, so the user does not commit a red workflow (#11482).
