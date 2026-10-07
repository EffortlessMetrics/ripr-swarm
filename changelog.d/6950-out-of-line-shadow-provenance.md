<!-- section: Fixed -->
- Owner-return pins and related-test reach now refuse a receiver type
  declared by an out-of-line parent module. A test in a nested child file
  binds its parent module's same-name type, which the single-file shadow
  check could not see, so the test-local method was reported as direct
  production reach. The parent chain now refuses the pin and the relation
  stays name-only (`weak_token_substring`), while a parent root holding a
  root-level owner keeps its pin as the production scope (#6950).
