# check-release-targets

Status: pass

## Why This Matters

Milestone membership is the committed candidate denominator. When the manifest, the release-goal graph, and milestone objects drift apart, progress counts stop meaning what they claim and conditional or umbrella work silently inflates a release promise. This check keeps the checked-in membership graph internally coherent offline; it does not read GitHub and does not qualify or publish any candidate. It also classifies every retained artifact under docs/release-candidates/ through the digest-bound lifecycle registry, so a superseded receipt cannot be reused as current authority from its filename or wording.

## Violations

None detected.

## Rerun

```bash
cargo xtask check-release-targets
```
