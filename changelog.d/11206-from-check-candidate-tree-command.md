<!-- section: Fixed -->
- The recovery command that `ripr baseline create --from-check` prints (and its help) now reads `ripr check --candidate-tree $(git rev-parse HEAD^{tree})`: `--candidate-tree` takes an object id, so the earlier text, which passed the literal `HEAD^{tree}`, failed with "malformed object ID" when copied (#11206).
