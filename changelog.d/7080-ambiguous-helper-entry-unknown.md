<!-- section: Fixed -->
- A helper chain refused only because an entry name is not unique in the
  workspace no longer reads as `no_static_path` when a test calls that
  name (or a non-unique caller of it). The relation stays refused; the
  finding is `static_unknown` and names the ambiguous function. A chain
  that no test enters, including a unique wrapper of a non-unique helper,
  still reads `no_static_path`, as does a receiver call that only shares
  the bare name (`req.parse()`), including when that line also contains
  the string or comment `parse(`, or an `fn parse()` item on the same
  `CallFact.text` line (#7080). Verdict-corpus rows
  `semver-op-greater-eq` and `semver-max-comparators` move from
  `no_static_path` to `static_unknown` because tests call non-unique
  `parse`/`from_str`; neighboring `semver-*` rows stay `no_static_path`.
  `rusqlite-singlethreaded-magic` drops the `no_static_path` sibling of
  its existing `static_unknown` finding for the same uniqueness-only
  refusal; the verdict stays `limited` / abstained.
