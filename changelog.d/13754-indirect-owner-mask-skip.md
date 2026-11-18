<!-- section: Changed -->
- The indirect-owner limit no longer masks every test body for each withheld
  finding: a test whose source never spells the owner's name cannot name it
  as a value or wrap its call, so it is skipped before masking. On one warm
  check this took about 15.9 s down to 13.5 s, with byte-identical JSON
  (#13754).
