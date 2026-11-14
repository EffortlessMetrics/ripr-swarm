<!-- section: Internal / CI -->
- The `Ripr Docs Gate` job gets a 45-minute budget on both the `pull_request`
  and `merge_group` routes, up from 20, because its cold build and full
  precommit table overran the old cap (#14732, #14644).
