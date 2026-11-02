<!-- section: Fixed -->
- A whitespace reformat beside a real edit no longer widens the changed
  span past an opaque callee. The #7956 repro (space before the index
  brackets, `50` becoming `51`) covers the literal `51`, not the whole
  indexed call. String-literal whitespace is still a change (#7956).
