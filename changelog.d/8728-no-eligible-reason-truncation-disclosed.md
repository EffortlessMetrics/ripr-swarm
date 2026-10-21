<!-- section: Fixed -->
- `ripr repair` with no eligible seam now discloses when its ineligibility-reason list is truncated: the bounded list of three is followed by "(and N more; run `ripr pilot --root ...` for per-seam detail)", mirroring the ambiguous-item wording, so the shown reasons no longer read as the whole story on repositories whose refusals span more than three reasons (#8728).
