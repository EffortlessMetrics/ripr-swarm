<!-- section: Internal / CI -->
- `windows-packaged-qualification.yml` gains a `rehearsal` input that accepts only `refs/heads/rehearsal/windows-packaged-<sha>` and marks the identity and receipt `rehearsal: true`, so the packaged Windows route can be rehearsed on a current head without a protected release tag. The qualifying path still requires `refs/tags/ripr-release-0.11.0-<sha>` (#2769).
